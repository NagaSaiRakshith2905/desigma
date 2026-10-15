//! Undoable Dev-mode handoff metadata operations.
use crate::edit::Tx;
use desigma_doc::*;

#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
pub enum HandoffError {
    #[error("node was not found")]
    Missing,
    #[error("invalid handoff operation: {0}")]
    Invalid(&'static str),
    #[error("document operation failed: {0}")]
    Document(String),
}
impl From<DocError> for HandoffError {
    fn from(e: DocError) -> Self {
        Self::Document(e.to_string())
    }
}

fn timestamp_shape(s: &str) -> bool {
    // Dev metadata stores RFC3339 timestamps as strings, with no chrono dependency.
    let b = s.as_bytes();
    if b.len() < 20 || b.get(4) != Some(&b'-') || b.get(7) != Some(&b'-') || b.get(10) != Some(&b'T') || b.get(13) != Some(&b':') || b.get(16) != Some(&b':') {
        return false;
    }
    let digits = |a: usize, z: usize| b[a..z].iter().all(u8::is_ascii_digit);
    if !digits(0, 4) || !digits(5, 7) || !digits(8, 10) || !digits(11, 13) || !digits(14, 16) || !digits(17, 19) {
        return false;
    }
    let tail = &s[19..];
    let tail = if let Some(fraction) = tail.strip_prefix('.') {
        let end = fraction.find(['Z', '+', '-']).unwrap_or(fraction.len());
        if end == 0 || !fraction[..end].bytes().all(|c| c.is_ascii_digit()) {
            return false;
        }
        &fraction[end..]
    } else {
        tail
    };
    tail == "Z"
        || (tail.len() == 6
            && matches!(tail.as_bytes()[0], b'+' | b'-')
            && tail.as_bytes()[3] == b':'
            && tail[1..3].bytes().all(|c| c.is_ascii_digit())
            && tail[4..6].bytes().all(|c| c.is_ascii_digit()))
}

/// Mark a supported node Ready for dev, Completed, or clear its status.
/// Changed is derived from the saved subtree fingerprint and cannot be assigned directly.
pub fn set_dev_status(
    doc: &mut Document,
    tx: &mut Tx,
    id: NodeId,
    status: Option<DevStatus>,
    updated_at: Option<&str>,
    snapshot_version: Option<u32>,
) -> Result<bool, HandoffError> {
    let node = doc.get(id).ok_or(HandoffError::Missing)?;
    let command = match status {
        None => desigma_doc::clear_status(doc, id).ok_or(HandoffError::Missing)?,
        Some(DevStatus::ReadyForDev) => {
            if !desigma_doc::can_mark(&node.kind) {
                return Err(HandoffError::Invalid("node kind cannot be marked Ready for dev"));
            }
            let time = updated_at.ok_or(HandoffError::Invalid("status timestamp is required"))?;
            if !timestamp_shape(time) {
                return Err(HandoffError::Invalid("status timestamp must be RFC3339"));
            }
            desigma_doc::mark_ready(doc, id, time, snapshot_version).ok_or(HandoffError::Missing)?
        }
        Some(DevStatus::Completed) => {
            if node.dev.status.is_none() {
                return Err(HandoffError::Invalid("only tracked nodes can be marked Completed"));
            }
            let time = updated_at.ok_or(HandoffError::Invalid("status timestamp is required"))?;
            if !timestamp_shape(time) {
                return Err(HandoffError::Invalid("status timestamp must be RFC3339"));
            }
            desigma_doc::mark_completed(doc, id, time).ok_or(HandoffError::Missing)?
        }
        Some(DevStatus::Changed) => return Err(HandoffError::Invalid("Changed is derived from the Ready for dev snapshot")),
    };
    tx.cmd(doc, command)?;
    Ok(true)
}

/// Replace only a node's handoff annotations, preserving status, links and snapshots.
pub fn set_dev_annotations(doc: &mut Document, tx: &mut Tx, id: NodeId, annotations: Vec<Annotation>) -> Result<bool, HandoffError> {
    if annotations.len() > 256 {
        return Err(HandoffError::Invalid("at most 256 annotations are allowed"));
    }
    for a in &annotations {
        if a.text.trim().is_empty()
            || a.text.len() > 16_384
            || a.category.as_ref().is_some_and(|c| c.len() > 128)
            || a.properties.len() > 32
            || a.properties.iter().any(|p| p.trim().is_empty() || p.len() > 128)
        {
            return Err(HandoffError::Invalid("annotation text, category or properties are invalid"));
        }
    }
    let mut node = doc.get(id).cloned().ok_or(HandoffError::Missing)?;
    if node.dev.annotations == annotations {
        return Ok(false);
    }
    node.dev.annotations = annotations;
    tx.cmd(doc, Command::Update { node: Box::new(node) })?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (Document, NodeId, NodeId) {
        let mut doc = Document::new("handoff");
        let page = doc.add_page("p");
        let frame = doc.alloc_id();
        doc.insert(page, Node::frame(frame, Rect::new(0., 0., 100., 100.))).unwrap();
        let leaf = doc.alloc_id();
        doc.insert(frame, Node::rectangle(leaf, Rect::new(0., 0., 20., 20.))).unwrap();
        (doc, frame, leaf)
    }
    #[test]
    fn status_and_annotations_are_validated_and_undoable() {
        let (mut doc, frame, _) = fixture();
        let mut hist = History::default();
        let time = "2026-10-15T10:30:00Z";
        let mut tx = Tx::new();
        set_dev_status(&mut doc, &mut tx, frame, Some(DevStatus::ReadyForDev), Some(time), None).unwrap();
        set_dev_annotations(
            &mut doc,
            &mut tx,
            frame,
            vec![Annotation { text: "Use 8px radius".into(), properties: vec!["Corner radius".into()], category: Some("Layout".into()) }],
        )
        .unwrap();
        assert!(tx.commit(&mut doc, &mut hist).unwrap());
        assert_eq!(doc.get(frame).unwrap().dev.status, Some(DevStatus::ReadyForDev));
        assert_eq!(doc.get(frame).unwrap().dev.annotations.len(), 1);
        let mut tx = Tx::new();
        set_dev_status(&mut doc, &mut tx, frame, Some(DevStatus::Completed), Some("2026-10-15T11:00:00+05:30"), None).unwrap();
        tx.commit(&mut doc, &mut hist).unwrap();
        assert_eq!(doc.get(frame).unwrap().dev.status, Some(DevStatus::Completed));
        hist.undo(&mut doc).unwrap();
        assert_eq!(doc.get(frame).unwrap().dev.status, Some(DevStatus::ReadyForDev));
        hist.undo(&mut doc).unwrap();
        assert_eq!(doc.get(frame).unwrap().dev, DevInfo::default());
    }
    #[test]
    fn invalid_status_and_annotations_do_not_mutate_document() {
        let (mut doc, _, leaf) = fixture();
        let before = doc.get(leaf).unwrap().clone();
        let mut tx = Tx::new();
        assert!(set_dev_status(&mut doc, &mut tx, leaf, Some(DevStatus::ReadyForDev), Some("bad"), None).is_err());
        assert!(set_dev_status(&mut doc, &mut tx, leaf, Some(DevStatus::Changed), Some("2026-10-15T10:30:00Z"), None).is_err());
        assert!(set_dev_annotations(&mut doc, &mut tx, leaf, vec![Annotation { text: " \n".into(), ..Default::default() }]).is_err());
        assert_eq!(doc.get(leaf).unwrap(), &before);
        tx.cancel(&mut doc);
    }
}
