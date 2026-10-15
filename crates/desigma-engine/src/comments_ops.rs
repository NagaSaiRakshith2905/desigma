//! Focused, validated operations for comment threads.
use crate::edit::Tx;
use desigma_doc::*;

#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
pub enum CommentOpsError {
    #[error("comment thread, page, or node was not found")]
    Missing,
    #[error("invalid comment operation: {0}")]
    Invalid(&'static str),
    #[error("document comment validation failed: {0}")]
    Validation(String),
}

/// Return a bounded slice of comment threads, optionally restricted to one page.
pub fn list_comments(
    doc: &Document,
    page_id: Option<NodeId>,
    include_resolved: bool,
    offset: usize,
    limit: usize,
) -> Result<Vec<CommentThread>, CommentOpsError> {
    if limit == 0 || limit > 500 {
        return Err(CommentOpsError::Invalid("limit must be between 1 and 500"));
    }
    if page_id.is_some_and(|id| !doc.pages.contains(&id)) {
        return Err(CommentOpsError::Missing);
    }
    Ok(doc
        .comments
        .iter()
        .filter(|thread| page_id.is_none_or(|page| thread.anchor.page == page))
        .filter(|thread| include_resolved || !thread.resolved)
        .skip(offset)
        .take(limit)
        .cloned()
        .collect())
}

/// Add a new unresolved thread with its opening message.
#[allow(clippy::too_many_arguments)]
pub fn add_comment(
    doc: &mut Document,
    tx: &mut Tx,
    thread_id: String,
    message_id: String,
    anchor: CommentAnchor,
    author: String,
    text: String,
    created: String,
) -> Result<bool, CommentOpsError> {
    if thread_id.trim().is_empty() || message_id.trim().is_empty() || author.trim().is_empty() || text.trim().is_empty() {
        return Err(CommentOpsError::Invalid("thread ID, message ID, author and text are required"));
    }
    if doc.comments.iter().any(|thread| thread.id == thread_id || thread.messages.iter().any(|message| message.id == message_id)) {
        return Err(CommentOpsError::Invalid("thread or message ID already exists"));
    }
    let comment = Comment { id: message_id, author, text, created, edited: None };
    let thread = CommentThread { id: thread_id, anchor, messages: vec![comment], resolved: false };
    crate::board_ops::edit_comments(doc, tx, |threads| threads.push(thread)).map_err(|e| CommentOpsError::Validation(e.to_string()))
}

/// Append a reply to an unresolved thread. Resolved threads must be explicitly
/// reopened before they accept more replies.
pub fn reply_comment(
    doc: &mut Document,
    tx: &mut Tx,
    thread_id: &str,
    message_id: String,
    author: String,
    text: String,
    created: String,
) -> Result<bool, CommentOpsError> {
    if message_id.trim().is_empty() || author.trim().is_empty() || text.trim().is_empty() {
        return Err(CommentOpsError::Invalid("message ID, author and text are required"));
    }
    let thread = doc.comments.iter().find(|thread| thread.id == thread_id).ok_or(CommentOpsError::Missing)?;
    if thread.resolved {
        return Err(CommentOpsError::Invalid("resolved thread must be reopened before replying"));
    }
    if doc.comments.iter().any(|thread| thread.messages.iter().any(|message| message.id == message_id)) {
        return Err(CommentOpsError::Invalid("message ID already exists"));
    }
    crate::board_ops::edit_comments(doc, tx, |threads| {
        if let Some(thread) = threads.iter_mut().find(|thread| thread.id == thread_id) {
            thread.messages.push(Comment { id: message_id, author, text, created, edited: None });
        }
    })
    .map_err(|e| CommentOpsError::Validation(e.to_string()))
}

/// Resolve or reopen an existing thread as one undoable comment-list change.
pub fn resolve_comment(doc: &mut Document, tx: &mut Tx, thread_id: &str, resolved: bool) -> Result<bool, CommentOpsError> {
    if !doc.comments.iter().any(|thread| thread.id == thread_id) {
        return Err(CommentOpsError::Missing);
    }
    let mut found = false;
    let changed = crate::board_ops::edit_comments(doc, tx, |threads| {
        if let Some(thread) = threads.iter_mut().find(|thread| thread.id == thread_id) {
            found = true;
            thread.resolved = resolved;
        }
    })
    .map_err(|e| CommentOpsError::Validation(e.to_string()))?;
    if !found {
        return Err(CommentOpsError::Missing);
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (Document, NodeId, NodeId) {
        let mut doc = Document::new("comments");
        let page = doc.add_page("Page");
        let node = doc.alloc_id();
        doc.insert(page, Node::rectangle(node, Rect::new(0., 0., 20., 20.))).unwrap();
        (doc, page, node)
    }
    fn add(doc: &mut Document, history: &mut History, page: NodeId, node: NodeId) {
        let mut tx = Tx::new();
        add_comment(
            doc,
            &mut tx,
            "thread-1".into(),
            "message-1".into(),
            CommentAnchor { page, node: Some(node), offset: Vec2::new(2., 3.) },
            "Ada".into(),
            "Please review this.".into(),
            "2026-01-01T10:00:00Z".into(),
        )
        .unwrap();
        tx.commit(doc, history).unwrap();
    }

    #[test]
    fn list_add_reply_resolve_and_undo_roundtrip() {
        let (mut doc, page, node) = fixture();
        let mut history = History::default();
        add(&mut doc, &mut history, page, node);
        assert_eq!(list_comments(&doc, Some(page), false, 0, 10).unwrap().len(), 1);
        let mut tx = Tx::new();
        reply_comment(&mut doc, &mut tx, "thread-1", "message-2".into(), "Lin".into(), "Looks good.".into(), "2026-01-01T10:01:00Z".into()).unwrap();
        tx.commit(&mut doc, &mut history).unwrap();
        assert_eq!(doc.comments[0].messages.len(), 2);
        let mut tx = Tx::new();
        resolve_comment(&mut doc, &mut tx, "thread-1", true).unwrap();
        tx.commit(&mut doc, &mut history).unwrap();
        assert!(list_comments(&doc, None, false, 0, 10).unwrap().is_empty());
        assert_eq!(list_comments(&doc, None, true, 0, 10).unwrap().len(), 1);
        history.undo(&mut doc).unwrap();
        assert!(!doc.comments[0].resolved);
        history.undo(&mut doc).unwrap();
        assert_eq!(doc.comments[0].messages.len(), 1);
        history.undo(&mut doc).unwrap();
        assert!(doc.comments.is_empty());
    }

    #[test]
    fn invalid_anchors_duplicate_ids_and_resolved_replies_are_rejected() {
        let (mut doc, page, node) = fixture();
        let mut history = History::default();
        add(&mut doc, &mut history, page, node);
        let mut tx = Tx::new();
        assert!(reply_comment(&mut doc, &mut tx, "thread-1", "message-1".into(), "Lin".into(), "duplicate ID".into(), "2026-01-01T10:01:00Z".into()).is_err());
        assert!(tx.ed.touched().is_empty());
        let mut tx = Tx::new();
        resolve_comment(&mut doc, &mut tx, "thread-1", true).unwrap();
        tx.commit(&mut doc, &mut history).unwrap();
        let mut tx = Tx::new();
        assert!(reply_comment(&mut doc, &mut tx, "thread-1", "message-2".into(), "Lin".into(), "reply".into(), "2026-01-01T10:01:00Z".into()).is_err());
        assert!(tx.ed.touched().is_empty());
        let mut tx = Tx::new();
        let error = add_comment(
            &mut doc,
            &mut tx,
            "bad".into(),
            "m".into(),
            CommentAnchor { page: NodeId(999), node: None, offset: Vec2::ZERO },
            "Ada".into(),
            "text".into(),
            "2026-01-01T10:00:00Z".into(),
        );
        assert!(error.is_err());
        assert!(tx.ed.touched().is_empty());
    }
}
