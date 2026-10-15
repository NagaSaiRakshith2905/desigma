use crate::{edit::Edit, layout};
use desigma_doc::*;
pub fn apply_resize(doc: &mut Document, ed: &mut Edit, id: NodeId, size: Vec2, transform: Affine, relayout: bool) {
    let Some(old) = doc.get(id).map(|n| n.size) else { return };
    if let Some(n) = ed.node(doc, id) {
        n.size = size;
        n.transform = transform;
        if let NodeKind::Text(t) = &mut n.kind {
            if t.auto_resize == TextAutoResize::WidthAndHeight {
                t.auto_resize = TextAutoResize::None;
            }
        }
    }
    if doc.get(id).map(|n| n.kind.is_container()).unwrap_or(false) {
        layout::apply_constraints(doc, ed, id, old, size);
        layout::layout_node(doc, ed, id);
    }
    if relayout {
        layout::relayout_up(doc, ed, id);
    }
}
