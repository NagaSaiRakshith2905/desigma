//! GUI-independent deck slide and export-preset operations.
use crate::edit::Tx;
use desigma_doc::*;

#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
pub enum DeckEditError {
    #[error("operation requires a deck")]
    NotDeck,
    #[error("slide or node not found")]
    MissingNode,
    #[error("invalid deck data: {0}")]
    Invalid(&'static str),
    #[error("document operation failed: {0}")]
    Document(String),
}
impl From<DocError> for DeckEditError {
    fn from(e: DocError) -> Self {
        Self::Document(e.to_string())
    }
}

fn is_slide(doc: &Document, id: NodeId) -> bool {
    doc.get(id).is_some_and(|n| n.slide.is_some() && matches!(n.kind, NodeKind::Frame(_)))
}
fn page_slides(doc: &Document, page: NodeId) -> Vec<NodeId> {
    doc.get(page).map(|p| p.children.iter().copied().filter(|id| is_slide(doc, *id)).collect()).unwrap_or_default()
}
fn valid_preset(p: &ExportPreset) -> bool {
    p.suffix.len() <= 128
        && !p.suffix.chars().any(char::is_control)
        && match p.constraint {
            ExportConstraint::Scale(v) => v.is_finite() && (0.01..=100.0).contains(&v),
            ExportConstraint::Width(v) | ExportConstraint::Height(v) => v.is_finite() && (1.0..=100_000.0).contains(&v),
        }
}

/// Adds a slide after `after` (or at the end), returning its new stable ID.
pub fn add_slide(doc: &mut Document, tx: &mut Tx, page: NodeId, after: Option<NodeId>, name: Option<String>) -> Result<NodeId, DeckEditError> {
    if doc.kind != DocumentKind::Deck {
        return Err(DeckEditError::NotDeck);
    }
    if !doc.get(page).is_some_and(|n| matches!(n.kind, NodeKind::Page { .. })) {
        return Err(DeckEditError::MissingNode);
    }
    let slides = page_slides(doc, page);
    let index = match after {
        Some(id) => slides.iter().position(|x| *x == id).map(|i| i + 1).ok_or(DeckEditError::MissingNode)?,
        None => slides.len(),
    };
    let name = name.unwrap_or_else(|| format!("Slide {}", slides.len() + 1));
    if name.trim().is_empty() || name.len() > 256 {
        return Err(DeckEditError::Invalid("slide name"));
    }
    let id = doc.try_alloc_id().ok_or(DeckEditError::Invalid("node ID space exhausted"))?;
    let mut slide = Node::frame(id, Rect::new(0., 0., 1280., 720.));
    slide.name = name;
    slide.slide = Some(SlideProps::default());
    if let NodeKind::Frame(f) = &mut slide.kind {
        f.clips_content = true;
    }
    let child_index =
        doc.get(page).unwrap().children.iter().position(|c| index < slides.len() && *c == slides[index]).unwrap_or(doc.get(page).unwrap().children.len());
    tx.cmd(doc, Command::Insert { parent: page, index: child_index, nodes: vec![slide] })?;
    Ok(id)
}

/// Deletes a slide while keeping at least one slide on its page.
pub fn delete_slide(doc: &mut Document, tx: &mut Tx, id: NodeId) -> Result<bool, DeckEditError> {
    if doc.kind != DocumentKind::Deck {
        return Err(DeckEditError::NotDeck);
    }
    if !is_slide(doc, id) {
        return Err(DeckEditError::MissingNode);
    }
    let page = doc.get(id).and_then(|n| n.parent).ok_or(DeckEditError::MissingNode)?;
    if page_slides(doc, page).len() <= 1 {
        return Err(DeckEditError::Invalid("a deck page must retain one slide"));
    }
    tx.cmd(doc, Command::Remove { id })?;
    Ok(true)
}

/// Reorders a slide among slide frames while retaining intervening ordinary layers.
pub fn reorder_slide(doc: &mut Document, tx: &mut Tx, id: NodeId, new_slide_index: usize) -> Result<bool, DeckEditError> {
    if doc.kind != DocumentKind::Deck {
        return Err(DeckEditError::NotDeck);
    }
    if !is_slide(doc, id) {
        return Err(DeckEditError::MissingNode);
    }
    let page = doc.get(id).and_then(|n| n.parent).ok_or(DeckEditError::MissingNode)?;
    let mut slides = page_slides(doc, page);
    let old = slides.iter().position(|x| *x == id).unwrap();
    let target = new_slide_index.min(slides.len() - 1);
    if old == target {
        return Ok(false);
    }
    slides.remove(old);
    slides.insert(target, id);
    let children = doc.get(page).unwrap().children.clone();
    let to = if target + 1 < slides.len() {
        children.iter().position(|x| *x == slides[target + 1]).unwrap()
    } else {
        children.iter().position(|x| *x == *slides.last().unwrap()).unwrap() + 1
    };
    tx.cmd(doc, Command::Reparent { id, parent: page, index: to })?;
    Ok(true)
}

/// Sets a slide's presenter notes in the existing slide metadata.
pub fn set_slide_notes(doc: &mut Document, tx: &mut Tx, id: NodeId, notes: String) -> Result<bool, DeckEditError> {
    if doc.kind != DocumentKind::Deck {
        return Err(DeckEditError::NotDeck);
    }
    if notes.len() > 100_000 {
        return Err(DeckEditError::Invalid("notes are too long"));
    }
    if !is_slide(doc, id) {
        return Err(DeckEditError::MissingNode);
    }
    let n = tx.ed.node(doc, id).ok_or(DeckEditError::MissingNode)?;
    let s = n.slide.as_mut().unwrap();
    if s.notes == notes {
        return Ok(false);
    }
    s.notes = notes;
    Ok(true)
}

/// Replaces export rows on a layer or slide as one property update.
pub fn set_export_presets(doc: &mut Document, tx: &mut Tx, id: NodeId, presets: Vec<ExportPreset>) -> Result<bool, DeckEditError> {
    if doc.get(id).is_none() {
        return Err(DeckEditError::MissingNode);
    }
    if presets.len() > 32 || !presets.iter().all(valid_preset) {
        return Err(DeckEditError::Invalid("export presets"));
    }
    let n = tx.ed.node(doc, id).ok_or(DeckEditError::MissingNode)?;
    if n.export_settings == presets {
        return Ok(false);
    }
    n.export_settings = presets;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn deck() -> (Document, NodeId, NodeId) {
        let mut d = Document::new("deck");
        d.kind = DocumentKind::Deck;
        let p = d.add_page("Slides");
        let a = add_slide(&mut d, &mut Tx::new(), p, None, None).unwrap();
        let b = add_slide(&mut d, &mut Tx::new(), p, Some(a), None).unwrap();
        (d, p, a.max(b))
    }
    #[test]
    fn slide_lifecycle_notes_and_undo() {
        let (mut d, page, _) = deck();
        let slides = page_slides(&d, page);
        let first = slides[0];
        let mut tx = Tx::new();
        set_slide_notes(&mut d, &mut tx, first, "Say hello".into()).unwrap();
        let mut h = History::default();
        tx.commit(&mut d, &mut h).unwrap();
        assert_eq!(d.get(first).unwrap().slide.as_ref().unwrap().notes, "Say hello");
        h.undo(&mut d).unwrap();
        assert!(d.get(first).unwrap().slide.as_ref().unwrap().notes.is_empty());
        let mut tx = Tx::new();
        delete_slide(&mut d, &mut tx, first).unwrap();
        let mut h = History::default();
        tx.commit(&mut d, &mut h).unwrap();
        assert_eq!(page_slides(&d, page).len(), 1);
        h.undo(&mut d).unwrap();
        assert_eq!(page_slides(&d, page).len(), 2);
    }
    #[test]
    fn slide_reorder_is_explicit_and_undoable() {
        let (mut d, page, _) = deck();
        let before = page_slides(&d, page);
        let mut tx = Tx::new();
        reorder_slide(&mut d, &mut tx, before[0], 1).unwrap();
        let mut h = History::default();
        tx.commit(&mut d, &mut h).unwrap();
        assert_eq!(page_slides(&d, page), vec![before[1], before[0]]);
        h.undo(&mut d).unwrap();
        assert_eq!(page_slides(&d, page), before);
    }
    #[test]
    fn invalid_export_presets_do_not_touch_node() {
        let (mut d, _, id) = deck();
        let mut tx = Tx::new();
        let bad = vec![ExportPreset { constraint: ExportConstraint::Scale(f32::NAN), ..Default::default() }];
        assert_eq!(set_export_presets(&mut d, &mut tx, id, bad), Err(DeckEditError::Invalid("export presets")));
        assert!(tx.ed.touched().is_empty());
    }
}
