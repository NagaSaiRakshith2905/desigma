//! Transactional document-level vector operations built on the renderer's
//! geometry routines. These helpers deliberately do not approximate curves.
use crate::{edit::Tx, layout};
use desigma_doc::*;
use desigma_render::vectorops::{flatten_nodes, node_paths, outline_stroke};

#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
pub enum VectorOpError {
    #[error("node not found")]
    MissingNode,
    #[error("invalid vector network")]
    InvalidNetwork,
    #[error("nodes must share a parent and be distinct")]
    InvalidSelection,
    #[error("selection has no vector geometry")]
    NoGeometry,
    #[error("unsupported linked component node")]
    LinkedNode,
    #[error("document has exhausted node IDs")]
    IdExhausted,
    #[error("document command failed: {0}")]
    Document(String),
}
impl From<DocError> for VectorOpError {
    fn from(e: DocError) -> Self {
        Self::Document(e.to_string())
    }
}

fn vector_node(id: NodeId, paths: &[Path], template: &Node) -> Result<Node, VectorOpError> {
    let mut network = VectorNetwork::from_paths(paths);
    if !network.is_valid() || network.is_empty() {
        return Err(VectorOpError::NoGeometry);
    }
    let bounds = network.bounds();
    if ![bounds.x, bounds.y, bounds.w, bounds.h].iter().all(|v| v.is_finite()) {
        return Err(VectorOpError::InvalidNetwork);
    }
    network.translate(Vec2::new(-bounds.x, -bounds.y));
    let mut n = Node::new(id, NodeKind::Vector { paths: network.to_paths(), network: Some(network) });
    n.transform = template.transform * Affine::translate(bounds.x, bounds.y);
    n.size = Vec2::new(bounds.w.max(1.0), bounds.h.max(1.0));
    n.fills = template.fills.clone();
    n.stroke = template.stroke.clone();
    n.effects = template.effects.clone();
    n.opacity = template.opacity;
    n.blend = if template.blend == BlendMode::PassThrough { BlendMode::Normal } else { template.blend };
    n.constraints = template.constraints;
    n.layout_child = template.layout_child;
    n.name = template.name.clone();
    Ok(n)
}

fn valid_network(n: &VectorNetwork) -> bool {
    let bounded = |v: f32| v.is_finite() && v.abs() <= 1_000_000.0;
    n.is_valid()
        && !n.is_empty()
        && n.vertices.len() <= 100_000
        && n.segments.len() <= 100_000
        && n.regions.len() <= 100_000
        && n.vertices.iter().all(|v| bounded(v.pos.x) && bounded(v.pos.y) && bounded(v.corner_radius) && v.corner_radius >= 0.0)
        && n.segments.iter().all(|s| bounded(s.ta.x) && bounded(s.ta.y) && bounded(s.tb.x) && bounded(s.tb.y))
}

/// Replace a vector node's editable network. Validation happens before touching
/// the transaction, so malformed input cannot leave a partial mutation.
pub fn set_vector_network(doc: &mut Document, tx: &mut Tx, id: NodeId, mut network: VectorNetwork) -> Result<(), VectorOpError> {
    if !valid_network(&network) {
        return Err(VectorOpError::InvalidNetwork);
    }
    let bounds = network.bounds();
    if ![bounds.x, bounds.y, bounds.w, bounds.h].iter().all(|v| v.is_finite()) || bounds.w < 0.0 || bounds.h < 0.0 {
        return Err(VectorOpError::InvalidNetwork);
    }
    let n = doc.get(id).ok_or(VectorOpError::MissingNode)?;
    if !matches!(n.kind, NodeKind::Vector { .. }) {
        return Err(VectorOpError::InvalidNetwork);
    }
    network.translate(Vec2::new(-bounds.x, -bounds.y));
    let node = tx.ed.node(doc, id).ok_or(VectorOpError::MissingNode)?;
    node.transform = node.transform * Affine::translate(bounds.x, bounds.y);
    node.size = Vec2::new(bounds.w.max(1.0), bounds.h.max(1.0));
    node.kind = NodeKind::Vector { paths: network.to_paths(), network: Some(network) };
    node.fill_geometry = None;
    node.stroke_geometry = None;
    if let Some(parent) = node.parent {
        layout::relayout_around(doc, &mut tx.ed, parent);
    }
    Ok(())
}

/// Creates a live boolean group around sibling nodes. Their transforms remain
/// in the same parent coordinate system, preserving the original appearance.
pub fn boolean_op(doc: &mut Document, tx: &mut Tx, ids: &[NodeId], op: BooleanOp) -> Result<NodeId, VectorOpError> {
    let ordered = sibling_selection(doc, ids, 2)?;
    if ordered.iter().any(|id| doc.get(*id).is_none_or(|n| matches!(n.kind, NodeKind::Text(_) | NodeKind::Page { .. } | NodeKind::Unknown { .. }))) {
        return Err(VectorOpError::NoGeometry);
    }
    let first = doc.get(ordered[0]).cloned().ok_or(VectorOpError::MissingNode)?;
    let parent = first.parent.ok_or(VectorOpError::InvalidSelection)?;
    let siblings = doc.get(parent).ok_or(VectorOpError::MissingNode)?.children.clone();
    let index = siblings.iter().position(|id| *id == ordered[0]).ok_or(VectorOpError::InvalidSelection)?;
    let id = doc.try_alloc_id().ok_or(VectorOpError::IdExhausted)?;
    let mut wrapper = Node::new(id, NodeKind::BooleanOp(op));
    wrapper.size = doc.get(parent).ok_or(VectorOpError::MissingNode)?.size;
    wrapper.fills = first.fills.clone();
    wrapper.stroke = first.stroke.clone();
    wrapper.effects = first.effects.clone();
    wrapper.name = match op {
        BooleanOp::Union => "Union",
        BooleanOp::Subtract => "Subtract",
        BooleanOp::Intersect => "Intersect",
        BooleanOp::Exclude => "Exclude",
    }
    .into();
    tx.cmd(doc, Command::Insert { parent, index, nodes: vec![wrapper] })?;
    for (i, child) in ordered.iter().enumerate() {
        tx.cmd(doc, Command::Reparent { id: *child, parent: id, index: i })?;
    }
    layout::relayout_around(doc, &mut tx.ed, parent);
    Ok(id)
}

fn sibling_selection(doc: &Document, ids: &[NodeId], min: usize) -> Result<Vec<NodeId>, VectorOpError> {
    if ids.len() < min || ids.iter().copied().collect::<std::collections::BTreeSet<_>>().len() != ids.len() {
        return Err(VectorOpError::InvalidSelection);
    }
    let first = doc.get(ids[0]).ok_or(VectorOpError::MissingNode)?;
    let parent = first.parent.ok_or(VectorOpError::InvalidSelection)?;
    let p = doc.get(parent).ok_or(VectorOpError::MissingNode)?;
    if !p.kind.is_container() || ids.iter().any(|id| doc.get(*id).is_none_or(|n| n.parent != Some(parent))) {
        return Err(VectorOpError::InvalidSelection);
    }
    let mut ordered = ids.to_vec();
    ordered.sort_by_key(|id| p.children.iter().position(|c| c == id).unwrap_or(usize::MAX));
    Ok(ordered)
}

/// Flatten sibling geometry to one editable vector, keeping the topmost
/// contributing node's appearance. Linked components are rejected explicitly.
pub fn flatten(doc: &mut Document, tx: &mut Tx, ids: &[NodeId]) -> Result<NodeId, VectorOpError> {
    let ordered = sibling_selection(doc, ids, 1)?;
    if ordered.iter().any(|id| doc.get(*id).is_none_or(|n| matches!(n.kind, NodeKind::Text(_) | NodeKind::Page { .. } | NodeKind::Unknown { .. }))) {
        return Err(VectorOpError::NoGeometry);
    }
    if ordered.iter().any(|id| {
        doc.get(*id)
            .is_some_and(|n| matches!(n.kind, NodeKind::Instance { .. } | NodeKind::Component(_) | NodeKind::ComponentSet(_)) || n.component.source.is_some())
    }) {
        return Err(VectorOpError::LinkedNode);
    }
    let parent = doc.get(ordered[0]).and_then(|n| n.parent).ok_or(VectorOpError::InvalidSelection)?;
    let flat = flatten_nodes(doc, &ordered, &doc.world_transform(parent)).ok_or(VectorOpError::NoGeometry)?;
    let template = doc.get(ordered[flat.style_from]).cloned().ok_or(VectorOpError::MissingNode)?;
    let siblings = doc.get(parent).ok_or(VectorOpError::MissingNode)?.children.clone();
    let index = ordered.iter().filter_map(|id| siblings.iter().position(|c| c == id)).max().map(|i| i + 1).unwrap_or(siblings.len());
    let id = doc.try_alloc_id().ok_or(VectorOpError::IdExhausted)?;
    let mut base = template.clone();
    base.transform = Affine::IDENTITY;
    let mut node = vector_node(id, &flat.paths, &base)?;
    node.component = ComponentLink::default();
    tx.cmd(doc, Command::Insert { parent, index, nodes: vec![node] })?;
    for old in ordered {
        tx.cmd(doc, Command::Remove { id: old })?;
    }
    layout::relayout_around(doc, &mut tx.ed, parent);
    Ok(id)
}

/// Expands visible stroke geometry into a filled editable vector. The original
/// node remains when it has visible fill/children; otherwise it is replaced.
pub fn outline_stroke_node(doc: &mut Document, tx: &mut Tx, id: NodeId) -> Result<NodeId, VectorOpError> {
    let original = doc.get(id).cloned().ok_or(VectorOpError::MissingNode)?;
    if original.stroke.weight <= 0.0 || !original.stroke.paints.iter().any(|p| p.visible) {
        return Err(VectorOpError::NoGeometry);
    }
    let paths = node_paths(doc, &original).ok_or(VectorOpError::NoGeometry)?;
    let net = match &original.kind {
        NodeKind::Vector { network: Some(net), .. } => Some(net),
        _ => None,
    };
    let outline = outline_stroke(&paths, &original.stroke, net);
    if outline.is_empty() {
        return Err(VectorOpError::NoGeometry);
    }
    let parent = original.parent.ok_or(VectorOpError::InvalidSelection)?;
    let siblings = doc.get(parent).ok_or(VectorOpError::MissingNode)?.children.clone();
    let index = siblings.iter().position(|c| *c == id).ok_or(VectorOpError::InvalidSelection)?;
    let keeps_source = original.fills.iter().any(|p| p.visible) || !original.children.is_empty();
    let new_id = doc.try_alloc_id().ok_or(VectorOpError::IdExhausted)?;
    let mut template = original.clone();
    template.fills = original.stroke.paints.clone();
    template.stroke.paints.clear();
    template.effects.clear();
    let mut node = vector_node(new_id, &outline, &template)?;
    node.component = ComponentLink::default();
    tx.cmd(doc, Command::Insert { parent, index: index + usize::from(keeps_source), nodes: vec![node] })?;
    if keeps_source {
        if let Some(source) = tx.ed.node(doc, id) {
            source.stroke.paints.clear();
        }
    } else {
        tx.cmd(doc, Command::Remove { id })?;
    }
    layout::relayout_around(doc, &mut tx.ed, parent);
    Ok(new_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (Document, NodeId) {
        let mut doc = Document::new("vectors");
        let page = doc.add_page("Page");
        (doc, page)
    }
    fn add_rect(doc: &mut Document, parent: NodeId, rect: Rect) -> NodeId {
        let id = doc.alloc_id();
        doc.insert(parent, Node::rectangle(id, rect)).unwrap();
        id
    }

    #[test]
    fn network_set_is_validated_and_undoable() {
        let (mut doc, page) = setup();
        let id = doc.alloc_id();
        let p = |x, y| Vec2::new(x, y);
        let path = Path {
            cmds: vec![PathCmd::MoveTo(p(0., 0.)), PathCmd::LineTo(p(10., 0.)), PathCmd::LineTo(p(10., 10.)), PathCmd::LineTo(p(0., 10.)), PathCmd::Close],
            fill_rule: FillRule::NonZero,
        };
        let network = VectorNetwork::from_paths(std::slice::from_ref(&path));
        assert!(network.is_valid(), "{network:?}");
        assert!(!network.is_empty());
        let mut original = Node::new(id, NodeKind::Vector { paths: network.to_paths(), network: Some(network.clone()) });
        original.size = Vec2::new(10., 10.);
        doc.insert(page, original).unwrap();
        let mut tx = Tx::new();
        let next = VectorNetwork::from_paths(&[Path {
            cmds: vec![PathCmd::MoveTo(p(0., 0.)), PathCmd::LineTo(p(20., 0.)), PathCmd::LineTo(p(20., 20.)), PathCmd::LineTo(p(0., 20.)), PathCmd::Close],
            fill_rule: FillRule::NonZero,
        }]);
        set_vector_network(&mut doc, &mut tx, id, next).unwrap();
        let mut history = History::default();
        assert!(tx.commit(&mut doc, &mut history).unwrap());
        assert!(matches!(doc.get(id).unwrap().kind, NodeKind::Vector { network: Some(_), .. }));
        history.undo(&mut doc).unwrap();
        assert!(matches!(doc.get(id).unwrap().kind, NodeKind::Vector { .. }));

        let before = doc.get(id).unwrap().clone();
        let bad = VectorNetwork { vertices: vec![], segments: vec![], regions: vec![] };
        let mut tx = Tx::new();
        assert_eq!(set_vector_network(&mut doc, &mut tx, id, bad), Err(VectorOpError::InvalidNetwork));
        assert_eq!(*doc.get(id).unwrap(), before);
    }

    #[test]
    fn boolean_group_and_flatten_are_single_undoable_edits() {
        let (mut doc, page) = setup();
        let a = add_rect(&mut doc, page, Rect::new(0., 0., 20., 20.));
        let b = add_rect(&mut doc, page, Rect::new(10., 0., 20., 20.));
        let mut history = History::default();
        let mut tx = Tx::new();
        let group = boolean_op(&mut doc, &mut tx, &[b, a], BooleanOp::Union).unwrap();
        assert_eq!(doc.get(group).unwrap().children, vec![a, b]);
        assert!(tx.commit(&mut doc, &mut history).unwrap());
        history.undo(&mut doc).unwrap();
        assert!(doc.get(group).is_none());
        assert_eq!(doc.get(page).unwrap().children, vec![a, b]);

        let mut tx = Tx::new();
        let vector = flatten(&mut doc, &mut tx, &[a, b]).unwrap();
        assert!(matches!(doc.get(vector).unwrap().kind, NodeKind::Vector { .. }));
        assert!(doc.get(a).is_none() && doc.get(b).is_none());
        assert!(tx.commit(&mut doc, &mut history).unwrap());
        history.undo(&mut doc).unwrap();
        assert!(doc.get(a).is_some() && doc.get(b).is_some() && doc.get(vector).is_none());
    }

    #[test]
    fn outline_stroke_converts_stroke_to_fill_and_undoes() {
        let (mut doc, page) = setup();
        let id = add_rect(&mut doc, page, Rect::new(0., 0., 20., 20.));
        {
            let node = doc.get_mut(id).unwrap();
            node.stroke.weight = 4.;
            node.stroke.paints = vec![Paint::solid(Color::rgb(0, 0, 0))];
            node.fills.clear();
        }
        let mut tx = Tx::new();
        let outlined = outline_stroke_node(&mut doc, &mut tx, id).unwrap();
        assert!(matches!(doc.get(outlined).unwrap().kind, NodeKind::Vector { .. }));
        assert!(doc.get(id).is_none());
        let mut history = History::default();
        assert!(tx.commit(&mut doc, &mut history).unwrap());
        history.undo(&mut doc).unwrap();
        assert!(doc.get(id).is_some() && doc.get(outlined).is_none());
    }
}
