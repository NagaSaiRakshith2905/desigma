//! Bounded revision snapshots, structural revision diffs, and named checkpoints.
//!
//! This module is intentionally independent of [`crate::engine::Engine`] so
//! transports can choose when to record snapshots and how to expose restore.
//! Restoring returns a replacement document with a monotonic revision; callers
//! should replace their undo history at the same time because the current
//! `desigma-doc::History` cannot represent a whole-document replacement.

use desigma_doc::{Document, History, NodeId};
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

const DEFAULT_SNAPSHOT_LIMIT: usize = 128;
const DEFAULT_SNAPSHOT_BYTES: usize = 64 * 1024 * 1024;
const DEFAULT_CHECKPOINT_BYTES: usize = 64 * 1024 * 1024;
const MAX_CHECKPOINTS: usize = 64;
const MAX_CHECKPOINT_NAME_BYTES: usize = 128;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct NodeRevisionDiff {
    pub node_id: NodeId,
    /// Top-level native node properties whose serialized values changed.
    pub changed_fields: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RevisionDiff {
    pub from_revision: u64,
    pub to_revision: u64,
    pub added_nodes: Vec<NodeId>,
    pub removed_nodes: Vec<NodeId>,
    pub changed_nodes: Vec<NodeRevisionDiff>,
    /// Changed document properties other than revision, node contents, and ID allocation state.
    pub changed_document_fields: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CheckpointInfo {
    pub name: String,
    pub revision: u64,
}

#[derive(Clone)]
pub struct RestoredCheckpoint {
    pub name: String,
    pub checkpoint_revision: u64,
    pub restored_revision: u64,
    pub document: Document,
    /// A whole-document restore starts a new undo lineage.
    pub history: History,
}

/// In-memory revision archive. Keep one per session and call [`capture`] after
/// successful commits, undo, redo, and checkpoint restore.
#[derive(Clone, Debug)]
pub struct RevisionArchive {
    snapshots: BTreeMap<u64, Document>,
    snapshot_sizes: BTreeMap<u64, usize>,
    checkpoints: BTreeMap<String, Document>,
    checkpoint_sizes: BTreeMap<String, usize>,
    snapshot_limit: usize,
    snapshot_byte_limit: usize,
    checkpoint_byte_limit: usize,
    snapshot_bytes: usize,
    checkpoint_bytes: usize,
}

impl Default for RevisionArchive {
    fn default() -> Self {
        Self::with_snapshot_limit(DEFAULT_SNAPSHOT_LIMIT)
    }
}

impl RevisionArchive {
    pub fn with_snapshot_limit(snapshot_limit: usize) -> Self {
        Self::with_limits(snapshot_limit, DEFAULT_SNAPSHOT_BYTES, DEFAULT_CHECKPOINT_BYTES)
    }

    pub fn with_limits(snapshot_limit: usize, snapshot_byte_limit: usize, checkpoint_byte_limit: usize) -> Self {
        Self {
            snapshots: BTreeMap::new(),
            snapshot_sizes: BTreeMap::new(),
            checkpoints: BTreeMap::new(),
            checkpoint_sizes: BTreeMap::new(),
            snapshot_limit: snapshot_limit.max(2),
            snapshot_byte_limit,
            checkpoint_byte_limit,
            snapshot_bytes: 0,
            checkpoint_bytes: 0,
        }
    }

    /// Record the exact state at a revision. Re-recording the same revision is
    /// allowed only when its document is identical.
    pub fn capture(&mut self, document: &Document) -> Result<(), String> {
        let serialized = serde_json::to_vec(document).map_err(|e| e.to_string())?;
        let byte_len = serialized.len();
        if let Some(existing) = self.snapshots.get(&document.revision) {
            if serde_json::to_vec(existing).map_err(|e| e.to_string())? != serialized {
                return Err(format!("revision_conflict: revision {} already has a different snapshot", document.revision));
            }
            return Ok(());
        }
        if byte_len > self.snapshot_byte_limit {
            return Err(format!("snapshot_too_large: serialized document is {byte_len} bytes; archive limit is {} bytes", self.snapshot_byte_limit));
        }
        self.snapshots.insert(document.revision, document.clone());
        self.snapshot_sizes.insert(document.revision, byte_len);
        self.snapshot_bytes = self.snapshot_bytes.saturating_add(byte_len);
        while self.snapshots.len() > self.snapshot_limit || self.snapshot_bytes > self.snapshot_byte_limit {
            if let Some(oldest) = self.snapshots.keys().next().copied() {
                self.snapshots.remove(&oldest);
                self.snapshot_bytes = self.snapshot_bytes.saturating_sub(self.snapshot_sizes.remove(&oldest).unwrap_or_default());
            }
        }
        Ok(())
    }

    pub fn snapshot_revisions(&self) -> Vec<u64> {
        self.snapshots.keys().copied().collect()
    }

    pub fn has_snapshot(&self, revision: u64) -> bool {
        self.snapshots.contains_key(&revision)
    }

    /// Start a fresh document session with only its initial snapshot.
    pub fn reset(&mut self, document: &Document) -> Result<(), String> {
        self.snapshots.clear();
        self.snapshot_sizes.clear();
        self.snapshot_bytes = 0;
        self.checkpoints.clear();
        self.checkpoint_sizes.clear();
        self.checkpoint_bytes = 0;
        self.capture(document)
    }

    pub fn diff_revisions(&self, from_revision: u64, to_revision: u64) -> Result<RevisionDiff, String> {
        let from = self.snapshots.get(&from_revision).ok_or_else(|| format!("revision_not_found: {from_revision}"))?;
        let to = self.snapshots.get(&to_revision).ok_or_else(|| format!("revision_not_found: {to_revision}"))?;
        diff_documents(from_revision, from, to_revision, to)
    }

    pub fn create_checkpoint(&mut self, name: &str, document: &Document) -> Result<CheckpointInfo, String> {
        let name = validate_name(name)?;
        if self.checkpoints.contains_key(&name) {
            return Err(format!("checkpoint_exists: {name}"));
        }
        if self.checkpoints.len() >= MAX_CHECKPOINTS {
            return Err(format!("checkpoint_limit: at most {MAX_CHECKPOINTS} named checkpoints are allowed"));
        }
        let byte_len = serde_json::to_vec(document).map_err(|e| e.to_string())?.len();
        if byte_len > self.checkpoint_byte_limit || self.checkpoint_bytes.saturating_add(byte_len) > self.checkpoint_byte_limit {
            return Err(format!("checkpoint_limit: checkpoint archive is limited to {} serialized bytes", self.checkpoint_byte_limit));
        }
        self.checkpoints.insert(name.clone(), document.clone());
        self.checkpoint_sizes.insert(name.clone(), byte_len);
        self.checkpoint_bytes = self.checkpoint_bytes.saturating_add(byte_len);
        Ok(CheckpointInfo { name, revision: document.revision })
    }

    pub fn list_checkpoints(&self) -> Vec<CheckpointInfo> {
        self.checkpoints.iter().map(|(name, document)| CheckpointInfo { name: name.clone(), revision: document.revision }).collect()
    }

    pub fn delete_checkpoint(&mut self, name: &str) -> Result<CheckpointInfo, String> {
        let name = validate_name(name)?;
        let document = self.checkpoints.remove(&name).ok_or_else(|| format!("checkpoint_not_found: {name}"))?;
        self.checkpoint_bytes = self.checkpoint_bytes.saturating_sub(self.checkpoint_sizes.remove(&name).unwrap_or_default());
        Ok(CheckpointInfo { name, revision: document.revision })
    }

    /// Restore a saved document while preserving monotonic revisions and
    /// avoiding reuse of node IDs allocated after the checkpoint. The returned
    /// history is empty because restoring a complete document starts a new
    /// undo lineage.
    pub fn restore_checkpoint(&self, name: &str, current: &Document) -> Result<RestoredCheckpoint, String> {
        let name = validate_name(name)?;
        let checkpoint = self.checkpoints.get(&name).ok_or_else(|| format!("checkpoint_not_found: {name}"))?;
        let restored_revision = current.revision.checked_add(1).ok_or_else(|| "revision_exhausted: cannot restore beyond u64::MAX".to_string())?;
        let mut document = checkpoint.clone();
        document.revision = restored_revision;
        let reserved = current.clone().try_alloc_id().ok_or_else(|| "id_exhausted: cannot preserve node ID allocation across restore".to_string())?;
        document.reserve_ids(reserved);
        Ok(RestoredCheckpoint { name, checkpoint_revision: checkpoint.revision, restored_revision, document, history: History::default() })
    }
}

fn validate_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() || name.len() > MAX_CHECKPOINT_NAME_BYTES || name.chars().any(char::is_control) {
        return Err(format!("invalid_checkpoint_name: name must contain 1..={MAX_CHECKPOINT_NAME_BYTES} non-control UTF-8 bytes"));
    }
    Ok(name.to_owned())
}

fn diff_documents(from_revision: u64, from: &Document, to_revision: u64, to: &Document) -> Result<RevisionDiff, String> {
    let from_nodes = serde_json::to_value(&from.nodes).map_err(|e| e.to_string())?;
    let to_nodes = serde_json::to_value(&to.nodes).map_err(|e| e.to_string())?;
    let from_map = from_nodes.as_object().ok_or("invalid_snapshot: nodes are not an object")?;
    let to_map = to_nodes.as_object().ok_or("invalid_snapshot: nodes are not an object")?;
    let from_ids: BTreeSet<_> = from_map.keys().cloned().collect();
    let to_ids: BTreeSet<_> = to_map.keys().cloned().collect();
    let added_nodes = to_ids.difference(&from_ids).map(|id| parse_node_id(id)).collect::<Result<Vec<_>, _>>()?;
    let removed_nodes = from_ids.difference(&to_ids).map(|id| parse_node_id(id)).collect::<Result<Vec<_>, _>>()?;
    let mut changed_nodes = Vec::new();
    for id in from_ids.intersection(&to_ids) {
        let old = from_map.get(id).ok_or("invalid_snapshot: missing source node")?;
        let new = to_map.get(id).ok_or("invalid_snapshot: missing target node")?;
        let changed_fields = changed_object_fields(old, new)?;
        if !changed_fields.is_empty() {
            changed_nodes.push(NodeRevisionDiff { node_id: parse_node_id(id)?, changed_fields });
        }
    }
    let from_doc = serde_json::to_value(from).map_err(|e| e.to_string())?;
    let to_doc = serde_json::to_value(to).map_err(|e| e.to_string())?;
    let mut changed_document_fields = changed_object_fields(&from_doc, &to_doc)?;
    changed_document_fields.retain(|field| !matches!(field.as_str(), "revision" | "nodes" | "next_id"));
    Ok(RevisionDiff { from_revision, to_revision, added_nodes, removed_nodes, changed_nodes, changed_document_fields })
}

fn changed_object_fields(old: &Value, new: &Value) -> Result<Vec<String>, String> {
    let old = old.as_object().ok_or("invalid_snapshot: expected object")?;
    let new = new.as_object().ok_or("invalid_snapshot: expected object")?;
    let keys: BTreeSet<_> = old.keys().chain(new.keys()).cloned().collect();
    Ok(keys.into_iter().filter(|key| old.get(key) != new.get(key)).collect())
}

fn parse_node_id(key: &str) -> Result<NodeId, String> {
    let raw = key.parse::<u64>().map_err(|_| format!("invalid_snapshot: invalid node key {key}"))?;
    Ok(NodeId(raw))
}

#[cfg(test)]
mod tests {
    use super::*;
    use desigma_doc::{Command, Node, Rect};

    fn document() -> (Document, NodeId) {
        let mut doc = Document::new("Archive test");
        let page = doc.add_page("Page 1");
        let id = doc.alloc_id();
        doc.insert(page, Node::rectangle(id, Rect::new(0.0, 0.0, 40.0, 20.0))).unwrap();
        (doc, id)
    }

    #[test]
    fn structural_diff_reports_added_removed_and_changed_nodes() {
        let (mut before, id) = document();
        let mut archive = RevisionArchive::default();
        archive.capture(&before).unwrap();
        let page = before.pages[0];
        let added_id = before.alloc_id();
        before.insert(page, Node::rectangle(added_id, Rect::new(0.0, 0.0, 8.0, 8.0))).unwrap();
        let mut changed = before.get(id).unwrap().clone();
        changed.name = "Updated".into();
        let mut history = History::default();
        history.exec(&mut before, Command::Update { node: Box::new(changed) }).unwrap();
        archive.capture(&before).unwrap();

        let diff = archive.diff_revisions(0, before.revision).unwrap();
        assert_eq!(diff.added_nodes, vec![added_id]);
        assert!(diff.removed_nodes.is_empty());
        let changed = diff.changed_nodes.iter().find(|entry| entry.node_id == id).unwrap();
        assert!(changed.changed_fields.contains(&"name".to_string()));
        let parent_change = diff.changed_nodes.iter().find(|entry| entry.node_id == page).unwrap();
        assert!(parent_change.changed_fields.contains(&"children".to_string()));
        assert!(diff.changed_document_fields.is_empty());
    }

    #[test]
    fn named_checkpoint_restores_state_with_monotonic_revision_and_fresh_history() {
        let (mut original, id) = document();
        let mut archive = RevisionArchive::default();
        let checkpoint = archive.create_checkpoint("  baseline ", &original).unwrap();
        assert_eq!(checkpoint.name, "baseline");
        let mut changed = original.get(id).unwrap().clone();
        changed.name = "After checkpoint".into();
        History::default().exec(&mut original, Command::Update { node: Box::new(changed) }).unwrap();
        let later_id = original.alloc_id();
        original.insert(original.pages[0], Node::rectangle(later_id, Rect::new(0.0, 0.0, 1.0, 1.0))).unwrap();

        let mut restored = archive.restore_checkpoint("baseline", &original).unwrap();
        assert_eq!(restored.checkpoint_revision, 0);
        assert_eq!(restored.restored_revision, original.revision + 1);
        assert_eq!(restored.document.get(id).unwrap().name, "Rectangle");
        assert!(!restored.history.can_undo());
        assert!(!restored.history.can_redo());
        assert!(restored.document.try_alloc_id().unwrap().0 > later_id.0);
        assert_eq!(archive.list_checkpoints().len(), 1);
    }

    #[test]
    fn rejects_conflicting_revisions_and_invalid_checkpoint_names() {
        let (doc, _) = document();
        let mut archive = RevisionArchive::with_snapshot_limit(1);
        archive.capture(&doc).unwrap();
        let mut conflict = doc.clone();
        conflict.name = "Different state".into();
        assert!(archive.capture(&conflict).unwrap_err().starts_with("revision_conflict:"));
        assert!(archive.create_checkpoint("\n", &doc).unwrap_err().starts_with("invalid_checkpoint_name:"));
        archive.create_checkpoint("safe", &doc).unwrap();
        assert!(archive.create_checkpoint("safe", &doc).unwrap_err().starts_with("checkpoint_exists:"));
        archive.delete_checkpoint("safe").unwrap();
        assert!(archive.restore_checkpoint("safe", &doc).err().unwrap().starts_with("checkpoint_not_found:"));
    }

    #[test]
    fn diff_reports_removed_nodes_and_prunes_old_snapshots() {
        let (mut doc, id) = document();
        let mut archive = RevisionArchive::with_snapshot_limit(2);
        archive.capture(&doc).unwrap();
        let mut history = History::default();
        history.exec(&mut doc, Command::Remove { id }).unwrap();
        archive.capture(&doc).unwrap();
        let diff = archive.diff_revisions(0, doc.revision).unwrap();
        assert_eq!(diff.removed_nodes, vec![id]);
        history.undo(&mut doc).unwrap();
        archive.capture(&doc).unwrap();
        assert_eq!(archive.snapshot_revisions(), vec![1, 2]);
        assert!(archive.diff_revisions(0, 2).unwrap_err().starts_with("revision_not_found:"));
    }

    #[test]
    fn byte_budgets_reject_oversized_snapshots_and_checkpoints() {
        let (doc, _) = document();
        let mut archive = RevisionArchive::with_limits(4, 1, 1);
        assert!(archive.capture(&doc).unwrap_err().starts_with("snapshot_too_large:"));
        assert!(archive.snapshot_revisions().is_empty());
        assert!(archive.create_checkpoint("large", &doc).unwrap_err().starts_with("checkpoint_limit:"));
        assert!(archive.list_checkpoints().is_empty());
    }
}
