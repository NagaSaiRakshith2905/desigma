use desigma_mcp::server::Session;
use serde_json::json;
use std::{fs, path::PathBuf};

struct Workspace(PathBuf);
impl Workspace {
    fn new() -> Self {
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let sequence = SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let p = std::env::temp_dir().join(format!("desigma-mcp-{}-{nonce}-{sequence}", std::process::id()));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn saved_file_roundtrip_and_unsaved_changes_guard() {
    let dir = Workspace::new();
    let mut s = Session::new(&dir.0).unwrap();
    let d = s.call_current("desigma_get_document", json!({})).unwrap();
    let page = d["pages"][0]["node_id"].clone();
    let node = s.call_current("desigma_create_node", json!({"parent_id":page,"kind":"text","name":"Heading","text":"Hello MCP"})).unwrap();
    assert!(s.call_current("desigma_new_document", json!({"name":"Oops"})).is_err());
    s.call_current("desigma_save_file", json!({"path":"first.desigma"})).unwrap();
    s.call_current("desigma_update_node", json!({"node_id":node["node"]["id"],"text":"Changed"})).unwrap();
    assert!(s.call_current("desigma_open_file", json!({"path":"first.desigma"})).is_err());
    s.call_current("desigma_open_file", json!({"path":"first.desigma","discard_changes":true})).unwrap();
    assert_eq!(s.call_current("desigma_get_node", json!({"node_id":node["node"]["id"]})).unwrap()["node"], node["node"]);
    let original = fs::read(dir.0.join("first.desigma")).unwrap();
    assert!(s.call_current("desigma_save_file", json!({"path":"first.desigma"})).is_err());
    assert_eq!(fs::read(dir.0.join("first.desigma")).unwrap(), original);
}

#[test]
fn paths_and_bad_imports_cannot_replace_the_session() {
    let dir = Workspace::new();
    let outside = Workspace::new();
    fs::write(outside.0.join("outside.desigma"), "{}").unwrap();
    let mut s = Session::new(&dir.0).unwrap();
    let before = s.call_current("desigma_get_document", json!({})).unwrap();
    assert!(s.call_current("desigma_open_file", json!({"path":outside.0.join("outside.desigma")})).is_err());
    assert!(s.call_current("desigma_save_file", json!({"path":outside.0.join("escape.desigma")})).is_err());
    assert!(s.call_current("desigma_save_file", json!({"path":"../escape.desigma"})).is_err());
    assert!(s.call_current("desigma_save_file", json!({"path":"bad.json"})).is_err());
    fs::write(dir.0.join("bad.desigma"), "not JSON").unwrap();
    assert!(s.call_current("desigma_open_file", json!({"path":"bad.desigma"})).is_err());
    let mut broken = s.engine.doc.clone();
    let id = broken.pages[0];
    broken.get_mut(id).unwrap().parent = Some(id);
    fs::write(dir.0.join("cycle.desigma"), broken.to_json().unwrap()).unwrap();
    assert!(s.call_current("desigma_open_file", json!({"path":"cycle.desigma"})).is_err());
    assert_eq!(s.call_current("desigma_get_document", json!({})).unwrap(), before);
    assert!(!outside.0.join("escape.desigma").exists());
}

#[test]
fn imported_stale_id_counter_does_not_overwrite_existing_nodes() {
    let dir = Workspace::new();
    let mut s = Session::new(&dir.0).unwrap();
    let mut native = serde_json::to_value(&s.engine.doc).unwrap();
    native["next_id"] = json!(0);
    fs::write(dir.0.join("stale.desigma"), native.to_string()).unwrap();
    let doc = s.call_current("desigma_open_file", json!({"path":"stale.desigma"})).unwrap();
    let page = doc["pages"][0]["node_id"].clone();
    let created = s.call_current("desigma_create_node", json!({"parent_id":page,"kind":"rectangle"})).unwrap();
    assert_ne!(created["node"]["id"], page);
    assert_eq!(s.call_current("desigma_get_document", json!({})).unwrap()["node_count"], 2);
    s.call_current("desigma_delete_node", json!({"node_id":created["node"]["id"]})).unwrap();
    let next = s.call_current("desigma_create_node", json!({"parent_id":page,"kind":"rectangle"})).unwrap();
    assert!(next["node"]["id"].as_u64().unwrap() > created["node"]["id"].as_u64().unwrap());

    let mut exhausted = serde_json::to_value(&s.engine.doc).unwrap();
    exhausted["next_id"] = json!(u64::MAX);
    fs::write(dir.0.join("exhausted.desigma"), exhausted.to_string()).unwrap();
    s.call_current("desigma_open_file", json!({"path":"exhausted.desigma","discard_changes":true})).unwrap();
    let before = s.engine.doc.to_json().unwrap();
    assert!(s.call_current("desigma_create_node", json!({"parent_id":page,"kind":"rectangle"})).unwrap_err().contains("exhausted"));
    assert_eq!(s.engine.doc.to_json().unwrap(), before);
}

#[test]
fn checkpoint_metadata_does_not_mark_the_design_dirty() {
    let dir = Workspace::new();
    let mut session = Session::new(&dir.0).unwrap();
    let created = session.call_current("desigma_checkpoint", json!({"name":"Starting point"})).unwrap();
    assert_eq!(created["checkpoint"]["revision"], 0);
    assert!(desigma_mcp::server::is_read_tool("desigma_diff_revisions"));
    assert!(desigma_mcp::server::is_read_tool("desigma_list_checkpoints"));

    // This would be rejected if checkpoint metadata were treated as an unsaved document edit.
    let replaced = session.call_current("desigma_new_document", json!({"name":"Fresh document"})).unwrap();
    assert_eq!(replaced["name"], "Fresh document");
}

trait Current {
    fn call_current(&mut self, name: &str, args: serde_json::Value) -> Result<serde_json::Value, String>;
}
impl Current for Session {
    fn call_current(&mut self, name: &str, mut args: serde_json::Value) -> Result<serde_json::Value, String> {
        if matches!(
            name,
            "desigma_create_node"
                | "desigma_update_node"
                | "desigma_delete_node"
                | "desigma_undo"
                | "desigma_redo"
                | "desigma_checkpoint"
                | "desigma_restore_checkpoint"
        ) {
            args["expected_revision"] = json!(self.engine.doc.revision);
        }
        self.call(name, args)
    }
}
