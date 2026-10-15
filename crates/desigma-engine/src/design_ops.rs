//! Validated, document-only design property operations used by interactive and
//! agent clients. These helpers deliberately leave transaction ownership to
//! callers: mutate through `Tx::ed` so the normal sync/undo path remains intact.
use crate::{edit::Tx, layout};
use desigma_doc::*;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct TextPatch {
    pub style: Option<TextStyle>,
    pub runs: Option<Vec<TextRun>>,
    pub auto_resize: Option<TextAutoResize>,
    pub max_lines: Option<Option<u32>>,
    pub paragraph: Option<ParagraphProps>,
    pub lists: Option<Vec<ListRange>>,
    pub direction: Option<TextDirection>,
}

#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct VisualPatch {
    pub stroke: Option<Stroke>,
    pub effects: Option<Vec<Effect>>,
    pub blend: Option<BlendMode>,
    pub corner_smoothing: Option<f32>,
    pub is_mask: Option<bool>,
    pub constraints: Option<Constraints>,
    pub layout_child: Option<LayoutChild>,
    pub clips_content: Option<bool>,
    /// Rotation in radians; preserves translation and scale magnitudes.
    pub rotation: Option<f32>,
}

#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
pub enum DesignOpError {
    #[error("node not found")]
    MissingNode,
    #[error("operation requires a text node")]
    NotText,
    #[error("invalid design property: {0}")]
    Invalid(&'static str),
}

fn finite(x: f32) -> bool {
    x.is_finite() && x.abs() <= 1_000_000.0
}
fn valid_style(s: &TextStyle) -> bool {
    (100..=900).contains(&s.font_weight)
        && finite(s.font_size)
        && s.font_size > 0.0
        && finite(s.letter_spacing)
        && finite(s.paragraph_spacing)
        && match s.line_height {
            LineHeight::Auto => true,
            LineHeight::Pixels(v) => finite(v) && v >= 0.0,
            LineHeight::Percent(v) => finite(v) && v >= 0.0,
        }
}

/// Applies text-level settings. Run offsets are UTF-8 byte ranges and must be
/// ordered, non-overlapping grapheme boundaries, matching the native model.
pub fn apply_text_patch(doc: &mut Document, tx: &mut Tx, id: NodeId, patch: &TextPatch) -> Result<(), DesignOpError> {
    let n = doc.get(id).ok_or(DesignOpError::MissingNode)?;
    let NodeKind::Text(t) = &n.kind else { return Err(DesignOpError::NotText) };
    if patch.style.as_ref().is_some_and(|s| !valid_style(s)) {
        return Err(DesignOpError::Invalid("text style"));
    }
    if let Some(runs) = &patch.runs {
        let mut end = 0;
        for r in runs {
            if r.start >= r.end
                || r.start < end
                || r.end > t.content.len()
                || !t.content.is_char_boundary(r.start)
                || !t.content.is_char_boundary(r.end)
                || !t.content.grapheme_indices(true).any(|(i, _)| i == r.start)
                || (r.end != t.content.len() && !t.content.grapheme_indices(true).any(|(i, _)| i == r.end))
                || !valid_style(&r.style)
            {
                return Err(DesignOpError::Invalid("text run ranges or style"));
            }
            end = r.end;
        }
    }
    if patch.max_lines.flatten().is_some_and(|n| n == 0) {
        return Err(DesignOpError::Invalid("max_lines must be positive"));
    }
    if let Some(p) = &patch.paragraph {
        if !finite(p.indent) || !finite(p.list_spacing) {
            return Err(DesignOpError::Invalid("paragraph spacing"));
        }
    }
    if let Some(lists) = &patch.lists {
        let mut end = 0;
        for l in lists {
            if l.start > l.end
                || l.end > t.content.len()
                || !t.content.is_char_boundary(l.start)
                || !t.content.is_char_boundary(l.end)
                || l.start < end
                || l.level > MAX_LIST_LEVEL
            {
                return Err(DesignOpError::Invalid("list ranges"));
            }
            end = l.end;
        }
    }
    let n = tx.ed.node(doc, id).ok_or(DesignOpError::MissingNode)?;
    let NodeKind::Text(t) = &mut n.kind else { return Err(DesignOpError::NotText) };
    if let Some(v) = &patch.style {
        t.style = v.clone();
    }
    if let Some(v) = &patch.runs {
        t.runs = v.clone();
    }
    if let Some(v) = patch.auto_resize {
        t.auto_resize = v;
    }
    if let Some(v) = patch.max_lines {
        t.max_lines = v;
    }
    if let Some(v) = &patch.paragraph {
        t.paragraph = v.clone();
    }
    if let Some(v) = &patch.lists {
        t.lists = v.clone();
    }
    if let Some(v) = patch.direction {
        t.direction = v;
    }
    let fitted = layout::fit_text(t, n.size);
    if t.auto_resize != TextAutoResize::None {
        n.size = fitted;
    }
    layout::relayout_around(doc, &mut tx.ed, id);
    Ok(())
}

fn valid_visual(p: &VisualPatch) -> bool {
    if p.corner_smoothing.is_some_and(|v| !finite(v) || !(0.0..=1.0).contains(&v)) || p.rotation.is_some_and(|v| !finite(v)) {
        return false;
    }
    if let Some(s) = &p.stroke {
        if !finite(s.weight)
            || s.weight < 0.0
            || !finite(s.miter_limit)
            || s.miter_limit < 0.0
            || s.dashes.iter().any(|v| !finite(*v) || *v < 0.0)
            || s.side_weights.is_some_and(|w| w.iter().any(|v| !finite(*v) || *v < 0.0))
        {
            return false;
        }
    }
    if p.layout_child.is_some_and(|v| {
        !finite(v.grow)
            || v.grow < 0.0
            || [v.min_w, v.max_w, v.min_h, v.max_h].into_iter().flatten().any(|x| !finite(x) || x < 0.0)
            || v.min_w.zip(v.max_w).is_some_and(|(a, b)| a > b)
            || v.min_h.zip(v.max_h).is_some_and(|(a, b)| a > b)
    }) {
        return false;
    }
    p.effects.as_ref().is_none_or(|effects| {
        effects.len() <= 64
            && effects.iter().all(|e| match e {
                Effect::DropShadow { color, offset, radius, spread, .. } | Effect::InnerShadow { color, offset, radius, spread, .. } => {
                    [color.r, color.g, color.b, color.a, offset.x, offset.y, *radius, *spread].iter().all(|v| finite(*v))
                        && (0.0..=1.0).contains(&color.r)
                        && (0.0..=1.0).contains(&color.g)
                        && (0.0..=1.0).contains(&color.b)
                        && (0.0..=1.0).contains(&color.a)
                        && *radius >= 0.0
                }
                Effect::LayerBlur { radius, .. } | Effect::BackgroundBlur { radius, .. } => finite(*radius) && *radius >= 0.0,
                _ => true,
            })
    })
}

/// Applies visual and layout properties with numeric bounds checked first.
pub fn apply_visual_patch(doc: &mut Document, tx: &mut Tx, id: NodeId, patch: &VisualPatch) -> Result<(), DesignOpError> {
    let original = doc.get(id).ok_or(DesignOpError::MissingNode)?;
    if !valid_visual(patch) {
        return Err(DesignOpError::Invalid("visual properties"));
    }
    if patch.clips_content.is_some() && original.kind.frame_props().is_none() {
        return Err(DesignOpError::Invalid("clipping requires a frame"));
    }
    let node = tx.ed.node(doc, id).ok_or(DesignOpError::MissingNode)?;
    if let Some(v) = &patch.stroke {
        node.stroke = v.clone();
    }
    if let Some(v) = &patch.effects {
        node.effects = v.clone();
    }
    if let Some(v) = patch.blend {
        node.blend = v;
    }
    if let Some(v) = patch.corner_smoothing {
        node.corner_smoothing = v;
    }
    if let Some(v) = patch.is_mask {
        node.is_mask = v;
    }
    if let Some(v) = patch.constraints {
        node.constraints = v;
    }
    if let Some(v) = patch.layout_child {
        node.layout_child = v;
    }
    if let Some(v) = patch.rotation {
        let sx = node.transform.a.hypot(node.transform.b);
        let sy = node.transform.c.hypot(node.transform.d);
        node.transform = Affine::translate(node.transform.tx, node.transform.ty) * Affine::rotate(v) * Affine::scale(sx, sy);
    }
    if let Some(v) = patch.clips_content {
        node.kind.frame_props_mut().expect("prevalidated frame property").clips_content = v;
    }
    layout::relayout_around(doc, &mut tx.ed, id);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_doc() -> (Document, NodeId) {
        let mut doc = Document::new("design ops");
        let page = doc.add_page("Page");
        let id = doc.alloc_id();
        doc.insert(page, Node::text(id, Rect::new(0., 0., 40., 20.), "éclair", TextStyle::default())).unwrap();
        (doc, id)
    }

    #[test]
    fn text_patch_preserves_valid_runs_and_records_undo() {
        let (mut doc, id) = text_doc();
        let mut tx = Tx::new();
        apply_text_patch(
            &mut doc,
            &mut tx,
            id,
            &TextPatch { runs: Some(vec![TextRun { start: 0, end: 2, style: TextStyle { font_weight: 700, ..Default::default() } }]), ..Default::default() },
        )
        .unwrap();
        let mut history = History::default();
        assert!(tx.commit(&mut doc, &mut history).unwrap());
        assert_eq!(doc.get(id).unwrap().kind.any_text().unwrap().runs[0].end, 2);
        history.undo(&mut doc).unwrap();
        assert!(doc.get(id).unwrap().kind.any_text().unwrap().runs.is_empty());
    }

    #[test]
    fn rejects_split_grapheme_without_touching_document() {
        let (mut doc, id) = text_doc();
        let mut tx = Tx::new();
        let patch = TextPatch { runs: Some(vec![TextRun { start: 1, end: 2, style: TextStyle::default() }]), ..Default::default() };
        assert_eq!(apply_text_patch(&mut doc, &mut tx, id, &patch), Err(DesignOpError::Invalid("text run ranges or style")));
        assert!(tx.ed.touched().is_empty());
    }

    #[test]
    fn visual_patch_checks_bounds_before_mutation() {
        let (mut doc, id) = text_doc();
        let mut tx = Tx::new();
        let patch = VisualPatch { corner_smoothing: Some(1.1), blend: Some(BlendMode::Multiply), ..Default::default() };
        assert_eq!(apply_visual_patch(&mut doc, &mut tx, id, &patch), Err(DesignOpError::Invalid("visual properties")));
        assert_eq!(doc.get(id).unwrap().blend, BlendMode::PassThrough);
        assert!(tx.ed.touched().is_empty());
    }
}
