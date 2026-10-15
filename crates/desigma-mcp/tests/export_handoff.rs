use desigma_mcp::server::Session;
use serde_json::json;

#[test]
fn workspace_export_creates_new_file_without_changing_revision() {
    let root = std::env::temp_dir().join(format!(
        "desigma-mcp-export-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let mut session = Session::new(&root).unwrap();
    let page = session.engine.doc.pages[0].0;
    let created = session.call("desigma_create_node", json!({"expected_revision":0,"parent_id":page,"kind":"frame","width":32,"height":20})).unwrap();
    let id = created["node"]["id"].as_u64().unwrap();
    let revision = session.engine.doc.revision;
    let result = session.call("desigma_export_node", json!({"node_id":id,"path":"frame.png","format":"png"})).unwrap();
    assert_eq!(result["revision"], revision);
    assert_eq!(session.engine.doc.revision, revision);
    assert!(std::fs::read(root.join("frame.png")).unwrap().starts_with(b"\x89PNG\r\n\x1a\n"));
    assert!(session.call("desigma_export_node", json!({"node_id":id,"path":"frame.png","format":"png"})).is_err());
    assert!(session.call("desigma_export_node", json!({"node_id":id,"path":"../escape.svg","format":"svg"})).is_err());
    std::fs::remove_dir_all(root).unwrap();
}
