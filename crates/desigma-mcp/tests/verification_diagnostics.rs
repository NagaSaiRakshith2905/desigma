use desigma_mcp::engine::Engine;
use serde_json::{json, Value};

#[test]
fn overflow_suggests_a_revision_checked_preview() {
    let mut engine = Engine::default();
    let id = engine
        .call("desigma_create_node", json!({"expected_revision":0,"parent_id":1,"kind":"text","text":"A rather long line of text","width":2,"height":2}))
        .unwrap()["node"]["id"]
        .as_u64()
        .unwrap();
    let result = engine.call("desigma_get_diagnostics", json!({"node_id":id})).unwrap();
    let finding = result["findings"].as_array().unwrap().iter().find(|f| f["code"] == "text_overflow").unwrap();
    let preview = &finding["suggested_fix"];
    assert_eq!(preview["expected_revision"], engine.doc.revision);
    assert_eq!(preview["dry_run"], true);
    assert_eq!(preview["ops"][0]["id"], id);
    let before = engine.doc.to_json().unwrap();
    engine.call("desigma_apply_ops", preview.clone()).unwrap();
    assert_eq!(engine.doc.to_json().unwrap(), before);
}

#[test]
fn region_and_node_diff_are_bounded_and_report_revision() {
    let mut engine = Engine::default();
    let first = engine.call("desigma_create_node", json!({"expected_revision":0,"parent_id":1,"kind":"rectangle","width":20,"height":20})).unwrap()["node"]
        ["id"]
        .as_u64()
        .unwrap();
    let second = engine
        .call("desigma_create_node", json!({"expected_revision":engine.doc.revision,"parent_id":1,"kind":"rectangle","width":20,"height":20}))
        .unwrap()["node"]["id"]
        .as_u64()
        .unwrap();
    let region = engine.call("desigma_render_region", json!({"page_id":1,"x":0,"y":0,"width":32,"height":24})).unwrap();
    assert_eq!(region["revision"], engine.doc.revision);
    assert_eq!(region["width"], 32);
    assert!(region["data"].as_str().unwrap().starts_with("iVBOR"));
    let diff = engine.call("desigma_render_diff", json!({"node_a":first,"node_b":second})).unwrap();
    assert_eq!(diff["revision"], engine.doc.revision);
    assert_eq!(diff["changed_pixel_ratio"], 0.0);
    assert!(engine.call("desigma_render_region", json!({"page_id":1,"x":0,"y":0,"width":5000,"height":2})).is_err());
}

#[test]
fn font_inventory_contains_bundled_faces() {
    let mut engine = Engine::default();
    let result = engine.call("desigma_list_fonts", json!({})).unwrap();
    assert_eq!(result["revision"], engine.doc.revision);
    assert!(result["faces"].as_array().unwrap().iter().any(|face| face["family"] == "Inter" && face["bundled"] == true));
}

#[test]
fn contrast_finding_has_safe_revision_checked_suggestion() {
    let mut engine = Engine::default();
    let frame = engine.call("desigma_create_node", json!({"expected_revision":0,"parent_id":1,"kind":"frame","width":120,"height":80})).unwrap()["node"]["id"]
        .as_u64()
        .unwrap();
    let text = engine
        .call("desigma_create_node", json!({"expected_revision":engine.doc.revision,"parent_id":frame,"kind":"text","text":"Label","width":80,"height":24}))
        .unwrap()["node"]["id"]
        .as_u64()
        .unwrap();
    let bg = desigma_doc::Color::rgb(153, 153, 153);
    engine.doc.get_mut(desigma_doc::NodeId(frame)).unwrap().fills = vec![desigma_doc::Paint::solid(bg)];
    let text_node = engine.doc.get_mut(desigma_doc::NodeId(text)).unwrap();
    text_node.kind.any_text_mut().unwrap().style.fills = vec![desigma_doc::Paint::solid(desigma_doc::Color::rgb(119, 119, 119))];
    let result = engine.call("desigma_get_diagnostics", json!({"node_id":text})).unwrap();
    let finding = result["findings"].as_array().unwrap().iter().find(|f| f["code"] == "low_text_contrast").unwrap();
    let preview = finding["suggested_fix"].clone();
    assert_eq!(preview["expected_revision"], engine.doc.revision);
    assert_eq!(preview["dry_run"], true);
    let before = engine.doc.to_json().unwrap();
    engine.call("desigma_apply_ops", preview.clone()).unwrap();
    assert_eq!(engine.doc.to_json().unwrap(), before);
    let mut fix = preview;
    fix["dry_run"] = json!(false);
    engine.call("desigma_apply_ops", fix).unwrap();
    assert_eq!(engine.doc.revision, 3);
}

#[test]
fn overlap_diagnostic_is_bounded_and_informational() {
    let mut engine = Engine::default();
    engine.call("desigma_create_node", json!({"expected_revision":0,"parent_id":1,"kind":"rectangle","width":40,"height":40})).unwrap();
    engine.call("desigma_create_node", json!({"expected_revision":engine.doc.revision,"parent_id":1,"kind":"ellipse","width":40,"height":40})).unwrap();
    let report = engine.call("desigma_get_diagnostics", json!({})).unwrap();
    let overlap = report["findings"].as_array().unwrap().iter().find(|f| f["code"] == "overlapping_siblings").unwrap();
    assert_eq!(overlap["severity"], "info");
    assert_eq!(overlap["suggested_fix"], Value::Null);
}
