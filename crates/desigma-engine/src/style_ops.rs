//! Transactional shared style CRUD and application.
use crate::edit::Tx;
use desigma_doc::*;

#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
pub enum StyleOpError {
    #[error("style was not found")]
    Missing,
    #[error("invalid shared style: {0}")]
    Invalid(&'static str),
    #[error("document operation failed: {0}")]
    Document(String),
}
impl From<DocError> for StyleOpError {
    fn from(e: DocError) -> Self {
        Self::Document(e.to_string())
    }
}

fn valid_name(s: &str) -> bool {
    !s.trim().is_empty() && s.trim().len() <= 256 && !s.chars().any(char::is_control)
}
fn finite_json<T: serde::Serialize>(value: &T) -> bool {
    fn walk(v: &serde_json::Value) -> bool {
        match v {
            serde_json::Value::Number(n) => n.as_f64().is_none_or(f64::is_finite),
            serde_json::Value::Array(xs) => xs.iter().all(walk),
            serde_json::Value::Object(xs) => xs.values().all(walk),
            _ => true,
        }
    }
    serde_json::to_value(value).is_ok_and(|v| walk(&v))
}
fn style_property(style: &Style) -> StyleProperty {
    match style {
        Style::Text(_) => StyleProperty::Text,
        Style::Effect(_) => StyleProperty::Effect,
        Style::Grid(_) => StyleProperty::Grid,
        Style::Fill(_) => StyleProperty::Fill,
    }
}
fn valid_style(style: &Style, property: StyleProperty) -> bool {
    finite_json(style)
        && matches!(
            (style, property),
            (Style::Fill(_), StyleProperty::Fill | StyleProperty::Stroke)
                | (Style::Text(_), StyleProperty::Text)
                | (Style::Effect(_), StyleProperty::Effect)
                | (Style::Grid(_), StyleProperty::Grid)
        )
}
fn ref_key(prop: StyleProperty) -> &'static str {
    match prop {
        StyleProperty::Fill => "fill",
        StyleProperty::Stroke => "stroke",
        StyleProperty::Text => "text",
        StyleProperty::Effect => "effect",
        StyleProperty::Grid => "grid",
    }
}

/// Bounded style listing, sorted by key for stable API output.
pub fn list_styles(doc: &Document, property: Option<StyleProperty>, offset: usize, limit: usize) -> Result<Vec<(String, Style, StyleMeta)>, StyleOpError> {
    if limit == 0 || limit > 500 {
        return Err(StyleOpError::Invalid("limit must be between 1 and 500"));
    }
    Ok(doc
        .styles
        .iter()
        .filter_map(|(key, style)| {
            let meta =
                doc.style_meta.get(key).cloned().unwrap_or_else(|| StyleMeta { name: key.clone(), property: style_property(style), ..Default::default() });
            property.is_none_or(|p| meta.property == p).then(|| (key.clone(), style.clone(), meta))
        })
        .skip(offset)
        .take(limit)
        .collect())
}

/// Create a style under a stable key. Key and display name are both validated.
pub fn create_style(doc: &mut Document, tx: &mut Tx, key: String, style: Style, meta: StyleMeta) -> Result<bool, StyleOpError> {
    if !valid_name(&key) || !valid_name(&meta.name) || meta.description.len() > 16_384 || !valid_style(&style, meta.property) {
        return Err(StyleOpError::Invalid("key, metadata, property, or style payload"));
    }
    if doc.styles.contains_key(&key) {
        return Err(StyleOpError::Invalid("style key already exists"));
    }
    let mut styles = doc.styles.clone();
    let mut metas = doc.style_meta.clone();
    styles.insert(key.clone(), style);
    metas.insert(key, meta);
    tx.cmd(doc, Command::SetStyles { styles, style_meta: metas })?;
    Ok(true)
}

/// Replace a style and refresh all live references in the same undo step.
pub fn update_style(doc: &mut Document, tx: &mut Tx, key: &str, style: Style, meta: Option<StyleMeta>) -> Result<bool, StyleOpError> {
    if !doc.styles.contains_key(key) {
        return Err(StyleOpError::Missing);
    }
    let old_meta = doc.style_meta.get(key).cloned().unwrap_or_else(|| StyleMeta {
        name: key.to_owned(),
        property: doc.styles.get(key).map(style_property).unwrap_or_default(),
        ..Default::default()
    });
    let new_meta = meta.unwrap_or(old_meta);
    if doc.style_meta.get(key).is_some_and(|old| old.property != new_meta.property) {
        return Err(StyleOpError::Invalid("changing a style's property requires delete and recreate"));
    }
    if !valid_name(&new_meta.name) || new_meta.description.len() > 16_384 || !valid_style(&style, new_meta.property) {
        return Err(StyleOpError::Invalid("metadata, property, or style payload"));
    }
    let prop = new_meta.property;
    let mut styles = doc.styles.clone();
    let mut metas = doc.style_meta.clone();
    styles.insert(key.to_owned(), style.clone());
    metas.insert(key.to_owned(), new_meta);
    tx.cmd(doc, Command::SetStyles { styles, style_meta: metas })?;
    let ids: Vec<_> = doc.nodes.values().filter(|n| n.style_refs.get(ref_key(prop)).is_some_and(|k| k == key)).map(|n| n.id).collect();
    for id in ids {
        apply_to_node(doc, tx, id, key, &style, prop);
    }
    Ok(true)
}

/// Delete a style while preserving its current baked appearance and clearing references.
pub fn delete_style(doc: &mut Document, tx: &mut Tx, key: &str) -> Result<bool, StyleOpError> {
    if !doc.styles.contains_key(key) {
        return Err(StyleOpError::Missing);
    }
    let mut styles = doc.styles.clone();
    let mut metas = doc.style_meta.clone();
    styles.remove(key);
    metas.remove(key);
    tx.cmd(doc, Command::SetStyles { styles, style_meta: metas })?;
    let ids: Vec<_> = doc.nodes.values().filter(|n| n.style_refs.values().any(|k| k == key)).map(|n| n.id).collect();
    for id in ids {
        if let Some(n) = tx.ed.node(doc, id) {
            n.style_refs.retain(|_, k| k != key);
        }
    }
    Ok(true)
}

/// Apply a style to compatible nodes. Fill styles may be applied as fill or stroke.
pub fn apply_style(doc: &mut Document, tx: &mut Tx, ids: &[NodeId], key: &str, property: Option<StyleProperty>) -> Result<bool, StyleOpError> {
    let style = doc.styles.get(key).cloned().ok_or(StyleOpError::Missing)?;
    let prop = property.or_else(|| doc.style_meta.get(key).map(|m| m.property)).unwrap_or_else(|| style_property(&style));
    if !valid_style(&style, prop) {
        return Err(StyleOpError::Invalid("style/property mismatch"));
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut changed = false;
    for id in ids {
        if !seen.insert(*id) {
            continue;
        }
        if doc.get(*id).is_none() {
            return Err(StyleOpError::Missing);
        }
        changed |= apply_to_node(doc, tx, *id, key, &style, prop);
    }
    Ok(changed)
}

fn apply_to_node(doc: &mut Document, tx: &mut Tx, id: NodeId, key: &str, style: &Style, prop: StyleProperty) -> bool {
    let mut applicable = true;
    {
        let Some(n) = tx.ed.node(doc, id) else {
            return false;
        };
        match (style, prop) {
            (Style::Fill(paints), StyleProperty::Stroke) => n.stroke.paints = paints.clone(),
            (Style::Fill(paints), StyleProperty::Fill) => {
                if let Some(t) = n.kind.any_text_mut() {
                    t.style.fills = paints.clone();
                } else {
                    n.fills = paints.clone();
                }
            }
            (Style::Text(ts), StyleProperty::Text) => {
                if let Some(t) = n.kind.any_text_mut() {
                    t.style = TextStyle { fills: t.style.fills.clone(), ..ts.clone() };
                    for r in &mut t.runs {
                        r.style = TextStyle { fills: r.style.fills.clone(), ..ts.clone() };
                    }
                    n.size = crate::layout::fit_text(t, n.size);
                } else {
                    applicable = false;
                }
            }
            (Style::Effect(effects), StyleProperty::Effect) => n.effects = effects.clone(),
            (Style::Grid(grids), StyleProperty::Grid) => {
                if let Some(f) = n.kind.frame_props_mut() {
                    f.grids = grids.clone();
                } else {
                    applicable = false;
                }
            }
            _ => applicable = false,
        }
        if applicable {
            n.style_refs.insert(ref_key(prop).into(), key.into());
        }
    }
    if applicable {
        crate::layout::relayout_around(doc, &mut tx.ed, id);
    }
    applicable
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (Document, NodeId, NodeId) {
        let mut d = Document::new("styles");
        let p = d.add_page("p");
        let a = d.alloc_id();
        d.insert(p, Node::rectangle(a, Rect::new(0., 0., 20., 20.))).unwrap();
        let b = d.alloc_id();
        d.insert(p, Node::text(b, Rect::new(0., 0., 20., 20.), "hello", TextStyle::default())).unwrap();
        (d, a, b)
    }
    #[test]
    fn style_crud_apply_update_delete_is_undoable() {
        let (mut d, a, _) = fixture();
        let mut h = History::default();
        let style = Style::Fill(vec![Paint::solid(Color::WHITE)]);
        let meta = StyleMeta { name: "Brand/Primary".into(), property: StyleProperty::Fill, ..Default::default() };
        let mut tx = Tx::new();
        create_style(&mut d, &mut tx, "brand.primary".into(), style.clone(), meta).unwrap();
        tx.commit(&mut d, &mut h).unwrap();
        assert_eq!(list_styles(&d, Some(StyleProperty::Fill), 0, 10).unwrap().len(), 1);
        let mut tx = Tx::new();
        apply_style(&mut d, &mut tx, &[a], "brand.primary", None).unwrap();
        tx.commit(&mut d, &mut h).unwrap();
        assert_eq!(d.get(a).unwrap().style_refs.get("fill").map(String::as_str), Some("brand.primary"));
        let updated = Style::Fill(vec![Paint::solid(Color::BLACK)]);
        let mut tx = Tx::new();
        update_style(&mut d, &mut tx, "brand.primary", updated.clone(), None).unwrap();
        tx.commit(&mut d, &mut h).unwrap();
        assert_eq!(
            d.get(a).unwrap().fills[0],
            match &updated {
                Style::Fill(p) => p[0].clone(),
                _ => unreachable!(),
            }
        );
        let mut tx = Tx::new();
        delete_style(&mut d, &mut tx, "brand.primary").unwrap();
        tx.commit(&mut d, &mut h).unwrap();
        assert!(!d.get(a).unwrap().style_refs.contains_key("fill"));
        h.undo(&mut d).unwrap();
        assert!(d.styles.contains_key("brand.primary"));
    }
    #[test]
    fn rejects_incompatible_apply_without_changes_and_applies_text_style() {
        let (mut d, a, b) = fixture();
        let mut h = History::default();
        d.styles.insert("type".into(), Style::Text(TextStyle { font_size: 22., ..Default::default() }));
        let before = d.get(a).unwrap().clone();
        let mut tx = Tx::new();
        assert!(!apply_style(&mut d, &mut tx, &[a], "type", None).unwrap());
        assert_eq!(d.get(a).unwrap().style_refs.get("text"), None);
        tx.cancel(&mut d);
        assert_eq!(d.get(a).unwrap(), &before);
        let mut tx = Tx::new();
        apply_style(&mut d, &mut tx, &[b], "type", None).unwrap();
        tx.commit(&mut d, &mut h).unwrap();
        assert_eq!(d.get(b).unwrap().kind.any_text().unwrap().style.font_size, 22.);
        assert!(apply_style(&mut d, &mut Tx::new(), &[a], "missing", None).is_err());
    }
}
