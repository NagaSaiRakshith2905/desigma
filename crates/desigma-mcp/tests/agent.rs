use desigma_mcp::engine::Engine;
use serde_json::json;
#[test]
fn typed_contract_revisions_preview_atomic_failure_and_undo() {
    let mut e = Engine::default();
    let node = e.call("desigma_create_node", json!({"expected_revision":0,"parent_id":1,"kind":"frame"})).unwrap()["node"]["id"].as_u64().unwrap();
    let revision = e.doc.revision;
    let before = e.doc.to_json().unwrap();
    let preview = e.call("desigma_set_properties", json!({"expected_revision":revision,"dry_run":true,"id":node,"properties":{"width":250}})).unwrap();
    assert_eq!(preview["dry_run"], true);
    assert_eq!(e.doc.to_json().unwrap(), before);
    assert!(e.call("desigma_apply_ops",json!({"expected_revision":revision,"ops":[{"op":"set_properties","id":node,"properties":{"width":250}},{"op":"set_properties","id":999,"properties":{"width":1}}]})).unwrap_err().starts_with("missing_node:"));
    assert_eq!(e.doc.to_json().unwrap(), before);
    e.call("desigma_set_properties", json!({"expected_revision":revision,"id":node,"properties":{"width":250}})).unwrap();
    assert!(e.doc.revision > revision);
    assert!(e
        .call("desigma_set_properties", json!({"expected_revision":revision,"id":node,"properties":{"width":300}}))
        .unwrap_err()
        .starts_with("stale_revision:"));
    e.call("desigma_undo", json!({"expected_revision":e.doc.revision})).unwrap();
    assert_eq!(e.doc.get(desigma_doc::NodeId(node)).unwrap().size.x, 100.0);
    assert!(e.doc.revision > revision);
}
#[test]
fn bounded_inspection_and_png_render_do_not_mutate() {
    let mut e = Engine::default();
    let node = e.call("desigma_create_node", json!({"expected_revision":0,"parent_id":1,"kind":"text","text":"Hello","width":100,"height":50})).unwrap()
        ["node"]["id"]
        .as_u64()
        .unwrap();
    let before = e.doc.to_json().unwrap();
    for name in ["desigma_get_layout", "desigma_get_diagnostics", "desigma_get_outline"] {
        let result = e.call(name, json!({"node_id":node,"limit":1})).unwrap();
        assert_eq!(result["revision"], e.doc.revision);
        assert!(e.call(name, json!({"limit":1001})).is_err());
    }
    let image = e.call("desigma_render_node", json!({"node_id":node})).unwrap();
    assert_eq!(image["mime_type"], "image/png");
    assert!(image["data"].as_str().unwrap().starts_with("iVBOR"));
    assert!(e.call("desigma_render_node", json!({"node_id":node,"scale":9})).is_err());
    assert_eq!(e.doc.to_json().unwrap(), before);
}

#[test]
fn variable_crud_operations_are_published_in_typed_contract() {
    let mut engine = Engine::default();
    let direct = engine.call("desigma_create_variable_collection", json!({"expected_revision":0,"temp_id":"$new1","name":"Direct"})).unwrap();
    assert_eq!(direct["temp_variable_ids"]["$new1"], "VariableCollectionId:d1");
    let result = engine
        .call(
            "desigma_apply_ops",
            json!({
                "expected_revision":engine.doc.revision,
                "ops":[
                    {"op":"create_variable_collection","temp_id":"$new1","name":"Theme"},
                    {"op":"create_variable_mode","temp_id":"$new2","collection_id":"$new1","name":"Dark"},
                    {"op":"create_variable","temp_id":"$new3","collection_id":"$new1","name":"Gap","kind":"Number"},
                    {"op":"edit_variable","id":"$new3","mode":"$new1_mode","value":{"Number":8.0}}
                ]
            }),
        )
        .unwrap();
    assert_eq!(result["temp_variable_ids"]["$new3"], "VariableID:d1");
    assert_eq!(engine.doc.variables.len(), 1);
}

#[test]
fn comment_workflow_operations_are_published_in_typed_contract() {
    let mut engine = Engine::default();
    let result = engine.call("desigma_apply_ops", json!({
        "expected_revision":0,
        "ops":[
            {"op":"add_comment","thread_id":"thread-1","message_id":"message-1","anchor":{"page":1,"node":null,"offset":{"x":2.0,"y":3.0}},"author":"Ada","text":"Review this","created":"2026-05-04T10:00:00Z"},
            {"op":"reply_comment","thread_id":"thread-1","message_id":"message-2","author":"Lin","text":"Done","created":"2026-05-04T10:01:00Z"},
            {"op":"resolve_comment","thread_id":"thread-1","resolved":true}
        ]
    })).unwrap();
    assert_eq!(result["revision"], 1);
    assert_eq!(engine.doc.comments[0].messages.len(), 2);
    assert!(engine.doc.comments[0].resolved);
    let reopened = engine.call("desigma_resolve_comment", json!({"expected_revision":engine.doc.revision,"thread_id":"thread-1","resolved":false})).unwrap();
    assert_eq!(reopened["revision"], 2);
    assert_eq!(engine.call("desigma_list_comments", json!({})).unwrap()["comments"][0]["resolved"], false);
}

#[test]
fn style_and_vector_direct_tools_use_atomic_engine() {
    let mut engine = Engine::default();
    let style = serde_json::to_value(desigma_doc::Style::Fill(vec![desigma_doc::Paint::solid(desigma_doc::Color::WHITE)])).unwrap();
    let meta = serde_json::to_value(desigma_doc::StyleMeta { name: "Brand/Primary".into(), property: desigma_doc::StyleProperty::Fill, ..Default::default() })
        .unwrap();
    let created = engine.call("desigma_create_style", json!({"expected_revision":0,"key":"brand.primary","style":style,"meta":meta})).unwrap();
    assert_eq!(created["revision"], 1);
    assert!(engine.doc.styles.contains_key("brand.primary"));
    let rectangle =
        engine.call("desigma_create_node", json!({"expected_revision":1,"parent_id":1,"kind":"rectangle"})).unwrap()["node"]["id"].as_u64().unwrap();
    let styled = engine.call("desigma_apply_style", json!({"expected_revision":2,"ids":[rectangle],"key":"brand.primary"})).unwrap();
    assert_eq!(styled["revision"], 3);
    assert_eq!(engine.doc.get(desigma_doc::NodeId(rectangle)).unwrap().style_refs.get("fill").map(String::as_str), Some("brand.primary"));
    assert!(engine.call("desigma_outline_stroke", json!({"expected_revision":3,"temp_id":"$new1","id":rectangle})).is_err());
    assert_eq!(engine.doc.revision, 3);
}

#[test]
fn slide_transition_tool_sets_metadata_as_one_undo_step() {
    let mut engine = Engine::default();
    engine.doc.kind = desigma_doc::DocumentKind::Deck;
    let slide = engine.call("desigma_add_slide", json!({"expected_revision":engine.doc.revision,"page":1,"temp_id":"$new1"})).unwrap()["temp_ids"]["$new1"]
        .as_u64()
        .unwrap();
    let revision = engine.doc.revision;
    let result = engine
        .call(
            "desigma_set_slide_transition",
            json!({"expected_revision":revision,"id":slide,"transition":{"kind":"Dissolve","easing":"EaseInOut","duration_ms":300}}),
        )
        .unwrap();
    assert_eq!(result["revision"], revision + 1);
    assert_eq!(engine.doc.get(desigma_doc::NodeId(slide)).unwrap().slide.as_ref().unwrap().transition.unwrap().duration_ms, 300);
    engine.history.undo(&mut engine.doc).unwrap();
    assert!(engine.doc.get(desigma_doc::NodeId(slide)).unwrap().slide.as_ref().unwrap().transition.is_none());
}

#[test]
fn diagnostics_exposes_missing_font_substitution() {
    let mut e = Engine::default();
    let node =
        e.call("desigma_create_node", json!({"expected_revision":0,"parent_id":1,"kind":"text","text":"Font test"})).unwrap()["node"]["id"].as_u64().unwrap();
    if let desigma_doc::NodeKind::Text(t) = &mut e.doc.get_mut(desigma_doc::NodeId(node)).unwrap().kind {
        t.style.font_family = "Nonexistent font 09c46d73".into();
    }
    let diagnostics = e.call("desigma_get_diagnostics", json!({"node_id":node})).unwrap();
    let finding =
        diagnostics["findings"].as_array().unwrap().iter().find(|f| f["code"] == "missing_font").expect("missing family must report substituted rendering");
    assert_eq!(finding["font_family"], "Nonexistent font 09c46d73");
    assert!(finding["resolved_family"].is_string());
}

#[test]
fn legacy_writes_require_revision_and_respect_locked_ancestors() {
    let mut e = Engine::default();
    assert!(e.call("desigma_create_node", json!({"parent_id":1,"kind":"frame"})).is_err());
    let id = e.call("desigma_create_node", json!({"expected_revision":0,"parent_id":1,"kind":"frame"})).unwrap()["node"]["id"].as_u64().unwrap();
    e.doc.get_mut(desigma_doc::NodeId(id)).unwrap().locked = true;
    let revision = e.doc.revision;
    let before = e.doc.to_json().unwrap();
    for (tool, args) in [
        ("desigma_create_node", json!({"expected_revision":revision,"parent_id":id,"kind":"rectangle"})),
        ("desigma_delete_node", json!({"expected_revision":revision,"node_id":id})),
    ] {
        assert!(e.call(tool, args).unwrap_err().starts_with("permission_denied:"));
        assert_eq!(e.doc.to_json().unwrap(), before);
    }
}
#[test]
fn nested_schemas_reject_unknown_native_properties() {
    let mut e = Engine::default();
    let before = e.doc.to_json().unwrap();
    let error = e
        .call(
            "desigma_set_fills",
            json!({"expected_revision":0,"id":1,"fills":[{"kind":{"Solid":{"r":0,"g":0,"b":0,"a":1,"typo":3}},"opacity":1,"visible":true,"blend":"Normal"}]}),
        )
        .unwrap_err();
    assert!(error.starts_with("invalid_operation:"));
    assert_eq!(e.doc.to_json().unwrap(), before);
}
