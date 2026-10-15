//! Transactional comment-thread and whiteboard-connector operations.
use crate::edit::Tx;
use desigma_doc::*;

#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
pub enum BoardEditError {
    #[error("node or page not found")]
    MissingNode,
    #[error("operation requires a board document")]
    NotBoard,
    #[error("invalid board data: {0}")]
    Invalid(&'static str),
    #[error("document operation failed: {0}")]
    Document(String),
}
impl From<DocError> for BoardEditError {
    fn from(e: DocError) -> Self {
        Self::Document(e.to_string())
    }
}

fn finite(v: f32) -> bool {
    v.is_finite() && v.abs() <= 1_000_000.0
}
fn valid_timestamp(s: &str) -> bool {
    // Require the RFC3339 structural form and valid numeric fields. The model
    // deliberately stores timestamps as strings and has no time dependency.
    let b = s.as_bytes();
    if b.len() < 20 || b.get(4) != Some(&b'-') || b.get(7) != Some(&b'-') || b.get(10) != Some(&b'T') || b.get(13) != Some(&b':') || b.get(16) != Some(&b':') {
        return false;
    }
    let digits = |r: std::ops::Range<usize>| b.get(r).is_some_and(|x| x.iter().all(u8::is_ascii_digit));
    digits(0..4)
        && digits(5..7)
        && digits(8..10)
        && digits(11..13)
        && digits(14..16)
        && digits(17..19)
        && (s.ends_with('Z') || s.rfind(['+', '-']).is_some_and(|i| i >= 19))
}

fn valid_comments(doc: &Document, threads: &[CommentThread]) -> bool {
    if threads.len() > 10_000 {
        return false;
    }
    let mut ids = std::collections::BTreeSet::new();
    threads.iter().all(|t| {
        !t.id.trim().is_empty()
            && t.id.len() <= 128
            && ids.insert(t.id.as_str())
            && doc.get(t.anchor.page).is_some_and(|n| matches!(n.kind, NodeKind::Page { .. }))
            && t.anchor.node.is_none_or(|id| doc.get(id).is_some() && doc.page_of(id) == Some(t.anchor.page))
            && finite(t.anchor.offset.x)
            && finite(t.anchor.offset.y)
            && !t.messages.is_empty()
            && t.messages.len() <= 1000
            && t.messages.iter().all(|m| {
                !m.id.trim().is_empty()
                    && m.id.len() <= 128
                    && m.author.len() <= 256
                    && m.text.len() <= 32_768
                    && valid_timestamp(&m.created)
                    && m.edited.as_ref().is_none_or(|x| valid_timestamp(x))
            })
    })
}

/// Applies comment create/edit/reply/resolve operations to one undoable list.
pub fn edit_comments(doc: &mut Document, tx: &mut Tx, edit: impl FnOnce(&mut Vec<CommentThread>)) -> Result<bool, BoardEditError> {
    let mut threads = doc.comments.clone();
    edit(&mut threads);
    if !valid_comments(doc, &threads) {
        return Err(BoardEditError::Invalid("comment threads"));
    }
    if threads == doc.comments {
        return Ok(false);
    }
    tx.cmd(doc, Command::SetComments { comments: threads })?;
    Ok(true)
}

fn valid_end(doc: &Document, e: ConnectorEnd, parent: NodeId) -> bool {
    finite(e.point.x) && finite(e.point.y) && e.node.is_none_or(|id| doc.get(id).is_some() && doc.page_of(id) == doc.page_of(parent))
}
fn valid_connector(doc: &Document, p: &ConnectorProps, parent: NodeId) -> bool {
    valid_end(doc, p.start, parent)
        && valid_end(doc, p.end, parent)
        && !(p.start.node.is_some() && p.start.node == p.end.node)
        && p.label.as_ref().is_none_or(|t| {
            t.content.len() <= 32_768
                && t.runs
                    .iter()
                    .all(|r| r.start < r.end && r.end <= t.content.len() && t.content.is_char_boundary(r.start) && t.content.is_char_boundary(r.end))
        })
}

/// Inserts a connector into a board container and schedules routing in the same Tx.
pub fn create_connector(doc: &mut Document, tx: &mut Tx, parent: NodeId, props: ConnectorProps) -> Result<NodeId, BoardEditError> {
    if doc.kind != DocumentKind::Board {
        return Err(BoardEditError::NotBoard);
    }
    let p = doc.get(parent).ok_or(BoardEditError::MissingNode)?;
    if !p.kind.is_container() {
        return Err(BoardEditError::Invalid("connector parent"));
    }
    let index = p.children.len();
    if !valid_connector(doc, &props, parent) {
        return Err(BoardEditError::Invalid("connector ends or label"));
    }
    let id = doc.try_alloc_id().ok_or(BoardEditError::Invalid("node ID space exhausted"))?;
    tx.cmd(doc, Command::Insert { parent, index, nodes: vec![Node::connector(id, props)] })?;
    tx.ed.touch(doc, id);
    sync_connector(doc, id);
    Ok(id)
}

/// Changes connector routing, ends, caps or label while retaining the node ID.
pub fn set_connector(doc: &mut Document, tx: &mut Tx, id: NodeId, props: ConnectorProps) -> Result<bool, BoardEditError> {
    if doc.kind != DocumentKind::Board {
        return Err(BoardEditError::NotBoard);
    }
    let n = doc.get(id).ok_or(BoardEditError::MissingNode)?;
    let parent = n.parent.ok_or(BoardEditError::Invalid("connector must have a parent"))?;
    if !matches!(n.kind, NodeKind::Connector(_)) {
        return Err(BoardEditError::Invalid("target is not a connector"));
    }
    if !valid_connector(doc, &props, parent) {
        return Err(BoardEditError::Invalid("connector ends or label"));
    }
    if matches!(&n.kind,NodeKind::Connector(old) if old==&props) {
        return Ok(false);
    }
    if let NodeKind::Connector(c) = &mut tx.ed.node(doc, id).ok_or(BoardEditError::MissingNode)?.kind {
        *c = props;
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn comment_edits_are_validated_and_undoable() {
        let mut d = Document::new("board");
        d.kind = DocumentKind::Board;
        let page = d.add_page("P");
        let mut tx = Tx::new();
        edit_comments(&mut d, &mut tx, |v| {
            v.push(CommentThread {
                id: "t1".into(),
                anchor: CommentAnchor { page, node: None, offset: Vec2::new(2., 3.) },
                messages: vec![Comment { id: "m1".into(), author: "A".into(), text: "hello".into(), created: "2025-01-01T10:00:00Z".into(), edited: None }],
                resolved: false,
            })
        })
        .unwrap();
        let mut h = History::default();
        tx.commit(&mut d, &mut h).unwrap();
        assert_eq!(d.comments.len(), 1);
        h.undo(&mut d).unwrap();
        assert!(d.comments.is_empty());
        let mut tx = Tx::new();
        assert_eq!(
            edit_comments(&mut d, &mut tx, |v| v.push(CommentThread {
                id: "bad".into(),
                anchor: CommentAnchor { page, node: None, offset: Vec2::ZERO },
                messages: vec![],
                resolved: false
            })),
            Err(BoardEditError::Invalid("comment threads"))
        );
    }
    #[test]
    fn connector_creation_routes_and_undoes() {
        let mut d = Document::new("board");
        d.kind = DocumentKind::Board;
        let page = d.add_page("P");
        let a = d.alloc_id();
        let b = d.alloc_id();
        d.insert(page, Node::sticky(a, Rect::new(0., 0., 100., 80.), "A")).unwrap();
        d.insert(page, Node::sticky(b, Rect::new(300., 0., 100., 80.), "B")).unwrap();
        let mut tx = Tx::new();
        let id = create_connector(
            &mut d,
            &mut tx,
            page,
            ConnectorProps { start: ConnectorEnd::attached(a, Magnet::Right), end: ConnectorEnd::attached(b, Magnet::Left), ..Default::default() },
        )
        .unwrap();
        let mut h = History::default();
        tx.commit(&mut d, &mut h).unwrap();
        assert!(matches!(d.get(id).unwrap().kind, NodeKind::Connector(_)));
        h.undo(&mut d).unwrap();
        assert!(d.get(id).is_none());
    }
    #[test]
    fn connector_rejects_missing_endpoint_before_mutation() {
        let mut d = Document::new("board");
        d.kind = DocumentKind::Board;
        let page = d.add_page("P");
        let mut tx = Tx::new();
        let p = ConnectorProps { start: ConnectorEnd::attached(NodeId(777), Magnet::Auto), ..Default::default() };
        assert_eq!(create_connector(&mut d, &mut tx, page, p), Err(BoardEditError::Invalid("connector ends or label")));
        assert!(tx.ed.touched().is_empty());
    }
}
