//! Undo/redo. Every edit goes through [`Command`]; applying returns its inverse.

use crate::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Command {
    /// Insert subtree nodes (root first) under `parent` at `index`.
    Insert {
        parent: NodeId,
        index: usize,
        nodes: Vec<Node>,
    },
    Remove {
        id: NodeId,
    },
    Reparent {
        id: NodeId,
        parent: NodeId,
        index: usize,
    },
    /// Replace a node's own properties (children/parent are preserved).
    Update {
        node: Box<Node>,
    },
    /// Insert a page subtree (page node first) at `index` in `Document::pages`.
    InsertPage {
        index: usize,
        nodes: Vec<Node>,
    },
    RemovePage {
        id: NodeId,
    },
    /// Replace the document's prototype flow list (flow starting points).
    SetFlows {
        flows: Vec<Flow>,
    },
    /// Replace shared styles and their metadata (style create / edit / delete).
    SetStyles {
        styles: BTreeMap<String, Style>,
        style_meta: BTreeMap<String, StyleMeta>,
    },
    /// Replace all comment threads.
    SetComments {
        comments: Vec<CommentThread>,
    },
    /// Replace library bookkeeping (publish info, enabled libraries, asset copies).
    SetLibraryState {
        state: LibraryState,
        library_page: Option<NodeId>,
    },
    /// Replace the guides of one page.
    SetGuides {
        page: NodeId,
        guides: Vec<Guide>,
    },
    /// Replace the prototype device settings.
    SetPrototypeDevice {
        device: PrototypeDevice,
    },
    /// Replace all variables and collections (variable edits).
    SetVariables {
        variables: BTreeMap<String, Variable>,
        collections: BTreeMap<String, VariableCollection>,
    },
    /// Insert, replace or remove one embedded image asset.
    SetImage {
        key: String,
        bytes: Option<Vec<u8>>,
    },
    /// Insert/remove the hidden library page without adding it to visible pages.
    InsertLibraryPage {
        nodes: Vec<Node>,
    },
    RemoveLibraryPage {
        id: NodeId,
    },
    Batch(Vec<Command>),
}

impl Command {
    /// Applies the command and returns the command that undoes it.
    pub fn apply(self, doc: &mut Document) -> Result<Command, DocError> {
        Ok(match self {
            Command::Insert { parent, index, nodes } => {
                let root = nodes.first().map(|n| n.id).ok_or(DocError::NotFound(parent))?;
                let p = doc.get_mut(parent).ok_or(DocError::NotFound(parent))?;
                if !p.kind.is_container() {
                    return Err(DocError::NotAContainer(parent));
                }
                let index = index.min(p.children.len());
                p.children.insert(index, root);
                for mut n in nodes {
                    if n.id == root {
                        n.parent = Some(parent);
                    }
                    doc.nodes.insert(n.id, n);
                }
                doc.bump_ids();
                Command::Remove { id: root }
            }
            Command::Remove { id } => {
                let parent = doc.get(id).ok_or(DocError::NotFound(id))?.parent.ok_or(DocError::NotFound(id))?;
                let index = doc.get(parent).map(|p| p.children.iter().position(|c| *c == id).unwrap_or(0)).unwrap_or(0);
                let nodes = doc.remove(id)?;
                Command::Insert { parent, index, nodes }
            }
            Command::Reparent { id, parent, index } => {
                let n = doc.get(id).ok_or(DocError::NotFound(id))?;
                let old_parent = n.parent.ok_or(DocError::NotFound(id))?;
                let old_index = doc.get(old_parent).and_then(|p| p.children.iter().position(|c| *c == id)).unwrap_or(0);
                doc.reparent(id, parent, index)?;
                Command::Reparent { id, parent: old_parent, index: old_index }
            }
            Command::Update { mut node } => {
                let cur = doc.get_mut(node.id).ok_or(DocError::NotFound(node.id))?;
                node.parent = cur.parent;
                node.children = std::mem::take(&mut cur.children);
                let old = std::mem::replace(cur, *node);
                Command::Update { node: Box::new(Node { children: Vec::new(), ..old }) }
            }
            Command::InsertPage { index, nodes } => {
                let root = nodes.first().ok_or(DocError::NotFound(NodeId(0)))?;
                if !matches!(root.kind, NodeKind::Page { .. }) {
                    return Err(DocError::NotAContainer(root.id));
                }
                let root = root.id;
                for mut n in nodes {
                    if n.id == root {
                        n.parent = None;
                    }
                    doc.nodes.insert(n.id, n);
                }
                let index = index.min(doc.pages.len());
                doc.pages.insert(index, root);
                doc.bump_ids();
                Command::RemovePage { id: root }
            }
            Command::RemovePage { id } => {
                let index = doc.pages.iter().position(|p| *p == id).ok_or(DocError::NotFound(id))?;
                let nodes = doc.remove(id)?;
                Command::InsertPage { index, nodes }
            }
            Command::SetFlows { flows } => Command::SetFlows { flows: std::mem::replace(&mut doc.flows, flows) },
            Command::SetStyles { styles, style_meta } => {
                Command::SetStyles { styles: std::mem::replace(&mut doc.styles, styles), style_meta: std::mem::replace(&mut doc.style_meta, style_meta) }
            }
            Command::SetComments { comments } => Command::SetComments { comments: std::mem::replace(&mut doc.comments, comments) },
            Command::SetLibraryState { state, library_page } => Command::SetLibraryState {
                state: std::mem::replace(&mut doc.library, state),
                library_page: std::mem::replace(&mut doc.library_page, library_page),
            },
            Command::SetGuides { page, guides } => {
                let old = if guides.is_empty() { doc.guides.remove(&page) } else { doc.guides.insert(page, guides) };
                Command::SetGuides { page, guides: old.unwrap_or_default() }
            }
            Command::SetPrototypeDevice { device } => Command::SetPrototypeDevice { device: std::mem::replace(&mut doc.prototype_device, device) },
            Command::SetVariables { variables, collections } => Command::SetVariables {
                variables: std::mem::replace(&mut doc.variables, variables),
                collections: std::mem::replace(&mut doc.variable_collections, collections),
            },
            Command::SetImage { key, bytes } => {
                let old = match bytes {
                    Some(bytes) => doc.images.insert(key.clone(), bytes),
                    None => doc.images.remove(&key),
                };
                Command::SetImage { key, bytes: old }
            }
            Command::InsertLibraryPage { nodes } => {
                let root = nodes.first().ok_or(DocError::NotFound(NodeId(0)))?;
                if !matches!(root.kind, NodeKind::Page { .. }) || root.parent.is_some() {
                    return Err(DocError::NotAContainer(root.id));
                }
                let id = root.id;
                let old = doc.library_page.and_then(|old_id| doc.remove(old_id).ok());
                for node in nodes {
                    doc.nodes.insert(node.id, node);
                }
                doc.library_page = Some(id);
                if let Some(nodes) = old {
                    Command::InsertLibraryPage { nodes }
                } else {
                    Command::RemoveLibraryPage { id }
                }
            }
            Command::RemoveLibraryPage { id } => {
                let nodes = doc.remove(id)?;
                if doc.library_page == Some(id) {
                    doc.library_page = None;
                }
                Command::InsertLibraryPage { nodes }
            }
            Command::Batch(cmds) => {
                let mut inv = Vec::with_capacity(cmds.len());
                for c in cmds {
                    inv.push(c.apply(doc)?);
                }
                inv.reverse();
                Command::Batch(inv)
            }
        })
    }
}

impl Document {
    pub(crate) fn bump_ids(&mut self) {
        if let Some((id, _)) = self.nodes.last_key_value() {
            self.next_id = self.next_id.max(id.0);
        }
    }
}

/// Linear undo stack.
#[derive(Clone, Default)]
pub struct History {
    undo: Vec<Command>,
    redo: Vec<Command>,
    undo_labels: Vec<Option<String>>,
    redo_labels: Vec<Option<String>>,
    undo_revisions: Vec<u64>,
    redo_revisions: Vec<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HistoryEntry {
    pub revision: u64,
    pub label: Option<String>,
}

impl History {
    pub fn exec(&mut self, doc: &mut Document, cmd: Command) -> Result<(), DocError> {
        let revision = doc.revision.checked_add(1).ok_or(DocError::RevisionExhausted)?;
        let mut staged = doc.clone();
        let inv = cmd.apply(&mut staged)?;
        staged.revision = revision;
        *doc = staged;
        self.undo.push(inv);
        self.redo.clear();
        self.redo_labels.clear();
        self.redo_revisions.clear();
        self.undo_labels.push(None);
        self.undo_revisions.push(revision);
        Ok(())
    }
    pub fn undo(&mut self, doc: &mut Document) -> Result<bool, DocError> {
        let Some(c) = self.undo.last().cloned() else { return Ok(false) };
        let revision = doc.revision.checked_add(1).ok_or(DocError::RevisionExhausted)?;
        let mut staged = doc.clone();
        let inverse = c.apply(&mut staged)?;
        staged.revision = revision;
        *doc = staged;
        self.undo.pop();
        self.redo_labels.push(self.undo_labels.pop().flatten());
        self.redo_revisions.push(self.undo_revisions.pop().unwrap_or(revision));
        self.redo.push(inverse);
        Ok(true)
    }
    pub fn redo(&mut self, doc: &mut Document) -> Result<bool, DocError> {
        let Some(c) = self.redo.last().cloned() else { return Ok(false) };
        let revision = doc.revision.checked_add(1).ok_or(DocError::RevisionExhausted)?;
        let mut staged = doc.clone();
        let inverse = c.apply(&mut staged)?;
        staged.revision = revision;
        *doc = staged;
        self.redo.pop();
        self.undo_labels.push(self.redo_labels.pop().flatten());
        self.redo_revisions.pop();
        self.undo_revisions.push(revision);
        self.undo.push(inverse);
        Ok(true)
    }
    /// Attach a human-readable source label to the latest committed undo step.
    pub fn label_last(&mut self, label: impl Into<String>) {
        if let Some(slot) = self.undo_labels.last_mut() {
            *slot = Some(label.into());
        }
    }
    pub fn undo_label(&self) -> Option<&str> {
        self.undo_labels.last().and_then(|s| s.as_deref())
    }
    /// The committed steps currently available to undo, oldest first.
    pub fn entries(&self) -> Vec<HistoryEntry> {
        self.undo_revisions.iter().zip(&self.undo_labels).map(|(revision, label)| HistoryEntry { revision: *revision, label: label.clone() }).collect()
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undo_redo_update_and_remove() {
        let mut d = Document::new("t");
        let page = d.add_page("P");
        let id = d.alloc_id();
        d.insert(page, Node::rectangle(id, Rect::new(0.0, 0.0, 10.0, 10.0))).unwrap();
        let mut h = History::default();

        let mut n = d.get(id).unwrap().clone();
        n.name = "Renamed".into();
        h.exec(&mut d, Command::Update { node: Box::new(n) }).unwrap();
        assert_eq!(d.get(id).unwrap().name, "Renamed");
        h.undo(&mut d).unwrap();
        assert_eq!(d.get(id).unwrap().name, "Rectangle");

        h.exec(&mut d, Command::Remove { id }).unwrap();
        assert!(d.get(id).is_none());
        h.undo(&mut d).unwrap();
        assert_eq!(d.get(page).unwrap().children, vec![id]);
        h.redo(&mut d).unwrap();
        assert!(d.get(id).is_none());
    }

    #[test]
    fn undo_redo_flows() {
        let mut d = Document::new("t");
        let page = d.add_page("P");
        let fid = d.alloc_id();
        let f = d.insert(page, Node::frame(fid, Rect::new(0.0, 0.0, 10.0, 10.0))).unwrap();
        let flow = Flow { name: "Flow 1".into(), page, start: f, description: String::new() };
        let mut h = History::default();
        h.exec(&mut d, Command::SetFlows { flows: vec![flow.clone()] }).unwrap();
        assert_eq!(d.flows, vec![flow]);
        h.undo(&mut d).unwrap();
        assert!(d.flows.is_empty());
        h.redo(&mut d).unwrap();
        assert_eq!(d.flows.len(), 1);
    }

    #[test]
    fn undo_redo_pages() {
        let mut d = Document::new("t");
        let p1 = d.add_page("P1");
        let pid = d.alloc_id();
        let mut page = Node::new(pid, NodeKind::Page { background: Color::WHITE });
        page.name = "P2".into();
        let mut h = History::default();
        h.exec(&mut d, Command::InsertPage { index: 0, nodes: vec![page] }).unwrap();
        assert_eq!(d.pages, vec![pid, p1]);
        let rid = d.alloc_id();
        d.insert(pid, Node::rectangle(rid, Rect::new(0.0, 0.0, 1.0, 1.0))).unwrap();

        h.exec(&mut d, Command::RemovePage { id: pid }).unwrap();
        assert_eq!(d.pages, vec![p1]);
        assert!(d.get(rid).is_none());
        h.undo(&mut d).unwrap();
        assert_eq!(d.pages, vec![pid, p1]);
        assert_eq!(d.get(pid).unwrap().children, vec![rid]);
        h.undo(&mut d).unwrap();
        assert_eq!(d.pages, vec![p1]);
    }
}

#[cfg(test)]
mod style_tests {
    use crate::*;

    #[test]
    fn set_styles_undo() {
        let mut d = Document::new("t");
        d.add_page("P");
        let mut h = History::default();
        let mut styles = d.styles.clone();
        styles.insert("s1".into(), Style::Fill(vec![Paint::solid(Color::WHITE)]));
        let mut meta = d.style_meta.clone();
        meta.insert("s1".into(), StyleMeta { name: "Brand/White".into(), ..Default::default() });
        h.exec(&mut d, Command::SetStyles { styles, style_meta: meta }).unwrap();
        assert_eq!(d.style_name("s1"), "Brand/White");
        h.undo(&mut d).unwrap();
        assert!(d.styles.is_empty() && d.style_meta.is_empty());
        h.redo(&mut d).unwrap();
        assert_eq!(d.styles.len(), 1);
    }
}

#[cfg(test)]
mod transaction_safety_tests {
    use super::*;
    #[test]
    fn failing_batch_preserves_document_revision_and_redo() {
        let mut doc = Document::new("atomic");
        let page = doc.add_page("P");
        let id = doc.alloc_id();
        doc.insert(page, Node::rectangle(id, Rect::new(0.0, 0.0, 10.0, 10.0))).unwrap();
        let mut history = History::default();
        let mut node = doc.get(id).unwrap().clone();
        node.opacity = 0.3;
        history.exec(&mut doc, Command::Update { node: Box::new(node.clone()) }).unwrap();
        history.undo(&mut doc).unwrap();
        let before = doc.to_json().unwrap();
        assert!(history.exec(&mut doc, Command::Batch(vec![Command::Update { node: Box::new(node) }, Command::Remove { id: NodeId(9999) }])).is_err());
        assert_eq!(doc.to_json().unwrap(), before);
        assert!(history.can_redo());
        assert!(!history.can_undo());
        history.redo(&mut doc).unwrap();
        assert_eq!(doc.revision, 3);
    }
    #[test]
    fn revisions_and_labels_survive_undo_and_redo() {
        let mut doc = Document::new("labels");
        doc.add_page("P");
        let mut history = History::default();
        history.exec(&mut doc, Command::SetFlows { flows: vec![] }).unwrap();
        history.label_last("Agent edit");
        assert_eq!(doc.revision, 1);
        assert_eq!(history.undo_label(), Some("Agent edit"));
        assert_eq!(history.entries(), vec![HistoryEntry { revision: 1, label: Some("Agent edit".into()) }]);
        history.undo(&mut doc).unwrap();
        assert_eq!(doc.revision, 2);
        assert!(history.entries().is_empty());
        history.redo(&mut doc).unwrap();
        assert_eq!(doc.revision, 3);
        assert_eq!(history.entries(), vec![HistoryEntry { revision: 3, label: Some("Agent edit".into()) }]);
        assert_eq!(history.undo_label(), Some("Agent edit"));
        doc.revision = u64::MAX;
        let before = doc.to_json().unwrap();
        assert!(history.undo(&mut doc).is_err());
        assert_eq!(doc.to_json().unwrap(), before);
        assert!(history.can_undo());
    }
}

#[cfg(test)]
mod allocator_safety_tests {
    use super::*;
    #[test]
    fn ordinary_allocation_recovers_imported_counter() {
        let mut doc = Document::new("import");
        let page = doc.add_page("P");
        doc.nodes.insert(NodeId(1000), Node::rectangle(NodeId(1000), Rect::new(0.0, 0.0, 1.0, 1.0)));
        assert_eq!(doc.alloc_id(), NodeId(1001));
        assert!(doc.get(page).is_some());
        assert_eq!(doc.remaining_node_ids(), u64::MAX - 1001);
        doc.reserve_ids(NodeId(u64::MAX));
        assert_eq!(doc.remaining_node_ids(), 0);
        assert!(doc.try_alloc_id().is_none());
    }
}
