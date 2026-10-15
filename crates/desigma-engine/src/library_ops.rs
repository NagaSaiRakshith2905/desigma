//! Safe Tx wrappers around the document's shared-library import/update model.
use crate::edit::Tx;
use desigma_doc::*;
use std::collections::BTreeSet;

#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
pub enum LibraryEditError {
    #[error("invalid library operation: {0}")]
    Invalid(&'static str),
    #[error("document operation failed: {0}")]
    Document(String),
}
impl From<DocError> for LibraryEditError {
    fn from(e: DocError) -> Self {
        Self::Document(e.to_string())
    }
}

/// Read-only list of publishable local components, sets, styles and variables.
pub fn list_publishable_assets(doc: &Document) -> Vec<(String, LibraryAssetKind, String)> {
    publishable_assets(doc)
}

/// Builds a self-contained library snapshot from the current document.
pub fn create_snapshot(doc: &Document, library_id: &str, name: &str, version: u32) -> Result<LibrarySnapshot, LibraryEditError> {
    if library_id.trim().is_empty() || library_id.len() > 256 || name.trim().is_empty() || name.len() > 256 || version == 0 {
        return Err(LibraryEditError::Invalid("library identity or version"));
    }
    Ok(snapshot(doc, library_id, name, version))
}

/// Local component copies suitable for a library browser.
pub fn list_local_components(doc: &Document) -> Vec<(NodeId, String)> {
    local_component_roots(doc)
}

/// Existing copies imported from libraries.
pub fn list_library_components(doc: &Document) -> Vec<(String, NodeId, String)> {
    library_component_copies(doc)
}

/// Pending changes available from one snapshot.
pub fn list_pending_updates(doc: &Document, snap: &LibrarySnapshot) -> Result<Vec<Update>, LibraryEditError> {
    if snap.library_id.trim().is_empty() {
        return Err(LibraryEditError::Invalid("snapshot library id"));
    }
    Ok(pending_updates(doc, snap))
}

fn import_command(doc: &Document, staged: &Document, mut command: Command) -> Command {
    let mut commands = Vec::new();
    // Library imports can create this hidden page directly as a legacy side effect.
    if doc.library_page.is_none() {
        if let Some(page) = staged.library_page {
            let mut nodes = Vec::new();
            staged.walk(page, |n, _| nodes.push(n.clone()));
            if !nodes.is_empty() {
                commands.push(Command::InsertLibraryPage { nodes });
            }
        }
    }
    // Embedded image insertion is also a legacy side effect. Convert it into
    // individually undoable commands before the document operations use refs.
    for (key, bytes) in &staged.images {
        if doc.images.get(key) != Some(bytes) {
            commands.push(Command::SetImage { key: key.clone(), bytes: Some(bytes.clone()) });
        }
    }
    match command {
        Command::Batch(mut nested) => commands.append(&mut nested),
        other => commands.push(other),
    }
    command = if commands.len() == 1 { commands.pop().unwrap() } else { Command::Batch(commands) };
    command
}

/// Imports a library asset (plus dependencies) and optionally places a component instance.
pub fn import_library_asset(
    doc: &mut Document,
    tx: &mut Tx,
    snap: &LibrarySnapshot,
    key: &str,
    placement: Option<Placement>,
) -> Result<bool, LibraryEditError> {
    if snap.library_id.trim().is_empty() || snap.asset(key).is_none() {
        return Err(LibraryEditError::Invalid("unknown asset"));
    }
    if let Some(p) = placement {
        if !doc.get(p.parent).is_some_and(|n| n.kind.is_container())
            || !p.at.x.is_finite()
            || !p.at.y.is_finite()
            || p.at.x.abs() > 1_000_000.
            || p.at.y.abs() > 1_000_000.
        {
            return Err(LibraryEditError::Invalid("asset placement"));
        }
    }
    let mut staged = doc.clone();
    let command = import_asset(&mut staged, snap, key, placement);
    let command = import_command(doc, &staged, command);
    if matches!(&command, Command::Batch(v) if v.is_empty()) {
        return Ok(false);
    }
    tx.cmd(doc, command)?;
    Ok(true)
}

/// Imports selected new or changed assets from one library version in one undo step.
pub fn apply_library_updates(doc: &mut Document, tx: &mut Tx, snap: &LibrarySnapshot, keys: &[String]) -> Result<bool, LibraryEditError> {
    if keys.len() > 512 || keys.iter().any(|k| snap.asset(k).is_none()) {
        return Err(LibraryEditError::Invalid("update keys"));
    }
    let pending = pending_updates(doc, snap);
    let eligible: BTreeSet<_> = pending.iter().filter(|u| u.change != UpdateKind::Removed).map(|u| u.key.clone()).collect();
    if keys.iter().any(|k| !eligible.contains(k)) {
        return Err(LibraryEditError::Invalid("asset has no applicable update"));
    }
    let mut staged = doc.clone();
    let command = apply_updates(&mut staged, snap, keys);
    let command = import_command(doc, &staged, command);
    if matches!(&command, Command::Batch(v) if v.is_empty()) {
        return Ok(false);
    }
    tx.cmd(doc, command)?;
    Ok(true)
}

pub fn enable_library(doc: &mut Document, tx: &mut Tx, snap: &LibrarySnapshot) -> Result<bool, LibraryEditError> {
    if snap.library_id.trim().is_empty() || snap.version == 0 {
        return Err(LibraryEditError::Invalid("library snapshot"));
    }
    if doc.library.enabled.iter().any(|l| l.id == snap.library_id && l.name == snap.name && l.version == snap.version) {
        return Ok(false);
    }
    tx.cmd(doc, desigma_doc::enable_library(doc, snap))?;
    Ok(true)
}

pub fn disable_library(doc: &mut Document, tx: &mut Tx, library_id: &str) -> Result<bool, LibraryEditError> {
    if library_id.trim().is_empty() {
        return Err(LibraryEditError::Invalid("library id"));
    }
    let enabled = doc.library.enabled.iter().any(|l| l.id == library_id);
    if !enabled {
        return Ok(false);
    }
    tx.cmd(doc, desigma_doc::disable_library(doc, library_id))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn source_snapshot() -> (LibrarySnapshot, String) {
        let mut src = Document::new("source");
        let page = src.add_page("P");
        let id = src.alloc_id();
        let mut n = Node::frame(id, Rect::new(0., 0., 80., 60.));
        n.kind = NodeKind::Component(FrameProps::default());
        n.name = "Button".into();
        src.images.insert("pixel".into(), vec![137, 80, 78, 71]);
        n.fills = vec![Paint {
            kind: PaintKind::Image { image_ref: "pixel".into(), scale_mode: ImageScaleMode::Fit },
            opacity: 1.0,
            visible: true,
            blend: BlendMode::Normal,
        }];
        src.insert(page, n).unwrap();
        let key = format!("c{}", id.0);
        (create_snapshot(&src, "lib", "Library", 1).unwrap(), key)
    }
    #[test]
    fn importing_component_creates_hidden_page_and_undoes_atomically() {
        let (snap, key) = source_snapshot();
        let mut doc = Document::new("consumer");
        let page = doc.add_page("P");
        let mut tx = Tx::new();
        import_library_asset(&mut doc, &mut tx, &snap, &key, Some(Placement { parent: page, at: Vec2::new(10., 10.) })).unwrap();
        let mut h = History::default();
        tx.commit(&mut doc, &mut h).unwrap();
        assert!(doc.library_page.is_some());
        assert_eq!(doc.pages.len(), 1);
        assert_eq!(list_library_components(&doc).len(), 1);
        assert!(doc.images.contains_key("pixel"));
        h.undo(&mut doc).unwrap();
        assert!(doc.library_page.is_none());
        assert_eq!(doc.get(page).unwrap().children.len(), 0);
        assert!(doc.images.is_empty());
    }
    #[test]
    fn invalid_placement_rejected_before_transaction_touch() {
        let (snap, key) = source_snapshot();
        let mut doc = Document::new("consumer");
        let page = doc.add_page("P");
        let mut tx = Tx::new();
        assert_eq!(
            import_library_asset(&mut doc, &mut tx, &snap, &key, Some(Placement { parent: page, at: Vec2::new(f32::NAN, 0.) })),
            Err(LibraryEditError::Invalid("asset placement"))
        );
        assert!(tx.ed.touched().is_empty());
    }
    #[test]
    fn snapshot_identity_is_validated() {
        let doc = Document::new("x");
        assert_eq!(create_snapshot(&doc, "", "x", 1).unwrap_err(), LibraryEditError::Invalid("library identity or version"));
    }
}
