use desigma_mcp::engine::Engine;
use serde_json::json;

#[test]
fn capabilities_report_stable_catalog_version_and_revision() {
    let mut engine = Engine::default();
    let response = engine.call("desigma_get_capabilities", json!({})).unwrap();
    assert_eq!(response["revision"], engine.doc.revision);
    assert_eq!(response["protocol_version"], desigma_agent_core::LIVE_PROTOCOL_VERSION);
    assert_eq!(response["tool_catalog_version"], desigma_agent_core::TOOL_CATALOG_VERSION);
    assert!(response["tools"].as_array().unwrap().iter().any(|name| name == "desigma_apply_ops"));
    assert!(engine.call("desigma_get_capabilities", json!({"extra":true})).is_err());
}
