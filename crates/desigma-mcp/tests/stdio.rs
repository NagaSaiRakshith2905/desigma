//! Real MCP client/server lifecycle across a subprocess, not a mocked dispatcher.
use rmcp::{
    model::{CallToolRequestParams, GetPromptRequestParams, ProtocolVersion, ReadResourceRequestParams, ServerNotification, SubscriptionFilter},
    transport::TokioChildProcess,
    ClientLifecycleMode, ClientServiceExt,
};
use serde_json::{json, Value};

#[tokio::test]
async fn stdio_handshake_discovery_edit_and_error_recovery() {
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_desigma-mcp"));
        command.arg("--workspace").arg(env!("CARGO_MANIFEST_DIR"));
        let client = ()
            .serve_with_lifecycle(
                TokioChildProcess::new(command).unwrap(),
                ClientLifecycleMode::Discover { preferred_versions: vec![ProtocolVersion::V_2026_07_28] },
            )
            .await
            .unwrap();
        assert_eq!(client.peer_info().unwrap().server_info.as_ref().unwrap().name, "desigma-mcp");
        let tools = client.list_all_tools().await.unwrap();
        assert!(tools.len() >= 28);
        assert!(tools.iter().all(|t| t.annotations.is_some()));
        let resources = client.list_resources(None).await.unwrap();
        assert_eq!(resources.resources.len(), 4);
        let document_resource = client.read_resource(ReadResourceRequestParams::new("desigma://document")).await.unwrap();
        let resource_json = serde_json::to_value(document_resource).unwrap();
        assert_eq!(resource_json["contents"][0]["mimeType"], "application/json");
        let prompts = client.list_prompts(None).await.unwrap();
        assert_eq!(prompts.prompts.len(), 4);
        let prompt = client.get_prompt(GetPromptRequestParams::new("fix_diagnostics")).await.unwrap();
        assert!(!serde_json::to_value(prompt).unwrap()["messages"].as_array().unwrap().is_empty());
        let call = |name: &'static str, args: Value| CallToolRequestParams::new(name).with_arguments(args.as_object().unwrap().clone());
        let schema = client.call_tool(call("desigma_describe_schema", json!({"tool":"desigma_set_fills"}))).await.unwrap();
        assert_eq!(schema.structured_content.unwrap()["tool"], "desigma_set_fills");
        let doc = client.call_tool(call("desigma_get_document", json!({}))).await.unwrap();
        assert_eq!(doc.is_error, Some(false));
        let page = doc.structured_content.unwrap()["pages"][0]["node_id"].clone();
        let mut subscription =
            client.listen(SubscriptionFilter::builder().resource_subscriptions(["desigma://document", "desigma://pages"]).build()).await.unwrap();
        assert_eq!(subscription.acknowledged().resource_subscriptions.as_ref().unwrap().len(), 2);
        let created = client
            .call_tool(call("desigma_create_node", json!({"expected_revision":0,"parent_id":page,"kind":"rectangle","name":"MCP test","width":120})))
            .await
            .unwrap();
        let update = tokio::time::timeout(std::time::Duration::from_secs(3), subscription.next()).await.unwrap().unwrap().unwrap();
        assert!(matches!(update, ServerNotification::ResourceUpdatedNotification(_)));
        assert!(matches!(subscription.next().await.unwrap().unwrap(), ServerNotification::ResourceUpdatedNotification(_)));
        subscription.cancel().await.unwrap();
        let id = created.structured_content.unwrap()["node"]["id"].clone();
        let invalid = client.call_tool(call("desigma_update_node", json!({"expected_revision":1,"node_id":id,"width":-1}))).await.unwrap();
        assert_eq!(invalid.is_error, Some(true));
        let code = client.call_tool(call("desigma_generate_code", json!({"node_id":id,"language":"css"}))).await.unwrap();
        assert!(code.structured_content.unwrap()["code"].as_str().unwrap().contains("120px"));
        let rendered = client.call_tool(call("desigma_render_node", json!({"node_id":id}))).await.unwrap();
        assert_eq!(rendered.is_error, Some(false));
        assert_eq!(rendered.structured_content.as_ref().unwrap()["revision"], 1);
        assert!(rendered.structured_content.as_ref().unwrap().get("data").is_none());
        let content = serde_json::to_value(&rendered.content).unwrap();
        assert_eq!(content[0]["type"], "image");
        assert_eq!(content[0]["mimeType"], "image/png");
        assert!(content[0]["data"].as_str().unwrap().starts_with("iVBOR"));
        let jpeg = client.call_tool(call("desigma_render_node", json!({"node_id":id,"scale":0.5,"format":"jpeg"}))).await.unwrap();
        let jpeg_content = serde_json::to_value(&jpeg.content).unwrap();
        assert_eq!(jpeg_content[0]["mimeType"], "image/jpeg");
        assert!(jpeg_content[0]["data"].as_str().unwrap().starts_with("/9j/"));
        client.call_tool(call("desigma_undo", json!({"expected_revision":1}))).await.unwrap();
        let gone = client.call_tool(call("desigma_get_node", json!({"node_id":id}))).await.unwrap();
        assert_eq!(gone.is_error, Some(true));
        client.call_tool(call("desigma_redo", json!({"expected_revision":2}))).await.unwrap();
        let restored = client.call_tool(call("desigma_get_node", json!({"node_id":id}))).await.unwrap();
        assert_eq!(restored.is_error, Some(false));
        client.cancel().await.unwrap();
    })
    .await
    .expect("MCP subprocess must finish within 30 seconds");
}
