//! Deep, undoable subtree duplication.
use crate::edit::Tx;
use desigma_doc::*;
use std::collections::BTreeMap;

#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
pub enum DuplicateError {
    #[error("source node or destination parent was not found")]
    Missing,
    #[error("invalid duplicate operation: {0}")]
    Invalid(&'static str),
    #[error("document operation failed: {0}")]
    Document(String),
}
impl From<DocError> for DuplicateError {
    fn from(e: DocError) -> Self {
        Self::Document(e.to_string())
    }
}

/// Duplicate a node and its descendants, preserving component-instance links.
/// Returns the new root ID and the mapping from every original subtree ID.
pub fn duplicate_node(
    doc: &mut Document,
    tx: &mut Tx,
    source: NodeId,
    parent: NodeId,
    index: Option<usize>,
    offset: Vec2,
) -> Result<(NodeId, BTreeMap<NodeId, NodeId>), DuplicateError> {
    let original = doc.get(source).ok_or(DuplicateError::Missing)?;
    if matches!(original.kind, NodeKind::Page { .. }) || original.parent.is_none() {
        return Err(DuplicateError::Invalid("pages and detached roots cannot be duplicated as nodes"));
    }
    let target = doc.get(parent).ok_or(DuplicateError::Missing)?;
    if !target.kind.is_container() {
        return Err(DuplicateError::Invalid("destination must be a container"));
    }
    if !offset.x.is_finite() || !offset.y.is_finite() || offset.x.abs() > 1_000_000. || offset.y.abs() > 1_000_000. {
        return Err(DuplicateError::Invalid("offset must be finite and within bounds"));
    }
    let at = index.unwrap_or(target.children.len());
    if at > target.children.len() {
        return Err(DuplicateError::Invalid("destination index is out of bounds"));
    }

    let mut old_ids = Vec::new();
    doc.walk(source, |node, _| old_ids.push(node.id));
    if old_ids.len() as u64 > doc.remaining_node_ids() {
        return Err(DuplicateError::Invalid("document has exhausted node IDs"));
    }
    let mut copied = desigma_doc::duplicate_subtree(doc, source);
    if copied.is_empty() || copied.len() != old_ids.len() {
        return Err(DuplicateError::Invalid("source subtree could not be copied"));
    }
    let root = copied[0].id;
    copied[0].parent = Some(parent);
    copied[0].transform.tx += offset.x;
    copied[0].transform.ty += offset.y;
    let mapping = old_ids.into_iter().zip(copied.iter().map(|node| node.id)).collect();
    tx.cmd(doc, Command::Insert { parent, index: at, nodes: copied })?;
    Ok((root, mapping))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deep_duplicate_maps_ids_offsets_and_preserves_instance_links_with_undo() {
        let mut d = Document::new("duplicate");
        let p = d.add_page("p");
        let source = d.alloc_id();
        d.insert(p, Node::frame(source, Rect::new(1., 2., 50., 60.))).unwrap();
        let child = d.alloc_id();
        d.insert(source, Node::rectangle(child, Rect::new(0., 0., 10., 10.))).unwrap();
        let mut hist = History::default();
        let mut tx = Tx::new();
        let (new_root, mapping) = duplicate_node(&mut d, &mut tx, source, p, None, Vec2::new(12., -4.)).unwrap();
        assert_eq!(mapping.len(), 2);
        assert_ne!(mapping[&source], source);
        assert_ne!(mapping[&child], child);
        assert_eq!(d.get(new_root).unwrap().transform.tx, 13.);
        assert_eq!(d.get(new_root).unwrap().transform.ty, -2.);
        assert_eq!(d.get(mapping[&child]).unwrap().parent, Some(new_root));
        tx.commit(&mut d, &mut hist).unwrap();
        hist.undo(&mut d).unwrap();
        assert!(d.get(new_root).is_none());
        assert!(d.get(source).is_some());
    }
    #[test]
    fn rejects_invalid_destination_without_document_changes() {
        let mut d = Document::new("duplicate");
        let p = d.add_page("p");
        let a = d.alloc_id();
        d.insert(p, Node::rectangle(a, Rect::new(0., 0., 10., 10.))).unwrap();
        let b = d.alloc_id();
        d.insert(p, Node::rectangle(b, Rect::new(0., 0., 10., 10.))).unwrap();
        let before = d.to_json().unwrap();
        let mut tx = Tx::new();
        assert!(duplicate_node(&mut d, &mut tx, a, b, None, Vec2::ZERO).is_err());
        assert_eq!(d.to_json().unwrap(), before);
        tx.cancel(&mut d);
    }
    #[test]
    fn duplicate_instance_keeps_link_to_original_component() {
        let mut d = Document::new("duplicate");
        let p = d.add_page("p");
        let component = d.alloc_id();
        d.insert(p, Node::frame(component, Rect::new(0., 0., 40., 30.))).unwrap();
        desigma_doc::create_component(&mut d, component).apply(&mut d).unwrap();
        desigma_doc::create_instance(&mut d, component, p, Vec2::new(60., 0.)).apply(&mut d).unwrap();
        let instance = *d.get(p).unwrap().children.last().unwrap();
        let mut tx = Tx::new();
        let (copy, _) = duplicate_node(&mut d, &mut tx, instance, p, None, Vec2::ZERO).unwrap();
        assert!(matches!(d.get(copy).unwrap().kind,NodeKind::Instance{component:Some(id),..} if id==component));
        assert_eq!(d.get(copy).unwrap().component.source, Some(component));
        tx.cancel(&mut d);
    }
}
