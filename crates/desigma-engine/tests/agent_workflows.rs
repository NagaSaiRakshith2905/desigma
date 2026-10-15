//! Workflow checks compare public agent transactions with the shared UI resize
//! entry point, including actual CPU pixels rather than only requested values.
use desigma_doc::*;
use desigma_engine::{apply_ops, apply_resize, edit::Tx, ApplyRequest, ErrorCode, NodeProperties, Operation};
use desigma_render::{render_page, tiny_skia::Pixmap, Viewport};

fn fixture() -> (Document, NodeId, NodeId, NodeId) {
    let mut doc = Document::new("Agent workflow");
    let page = doc.add_page("Page");
    let frame = doc.alloc_id();
    let mut node = Node::frame(frame, Rect::new(10.0, 10.0, 200.0, 80.0));
    node.kind.frame_props_mut().unwrap().auto_layout = Some(AutoLayout { padding: [10.0; 4], item_spacing: 10.0, ..Default::default() });
    doc.insert(page, node).unwrap();
    let child = doc.alloc_id();
    let mut node = Node::rectangle(child, Rect::new(0.0, 0.0, 20.0, 20.0));
    node.layout_child.horizontal = Sizing::Fill;
    node.fills = vec![Paint::solid(Color::BLACK)];
    doc.insert(frame, node).unwrap();
    let fixed = doc.alloc_id();
    doc.insert(frame, Node::rectangle(fixed, Rect::new(0.0, 0.0, 30.0, 20.0))).unwrap();
    let mut edit = desigma_engine::edit::Edit::new();
    desigma_engine::layout::layout_node(&mut doc, &mut edit, frame);
    (doc, page, frame, child)
}

fn resized(frame: NodeId, width: f32) -> Operation {
    Operation::SetProperties { id: frame, properties: NodeProperties { width: Some(width), ..Default::default() } }
}

fn pixels(doc: &Document, page: NodeId) -> Vec<u8> {
    let mut target = Pixmap::new(400, 120).unwrap();
    render_page(doc, page, &Viewport::default(), &mut target);
    target.data().to_vec()
}

#[test]
fn resize_matches_ui_entry_point_and_undo_redo_pixels() {
    let (mut agent, page, frame, child) = fixture();
    let before = agent.nodes.clone();
    let original_pixels = pixels(&agent, page);
    let mut ui = agent.clone();
    let transform = ui.get(frame).unwrap().transform;
    let mut ui_history = History::default();
    let mut tx = Tx::new();
    apply_resize(&mut ui, &mut tx.ed, frame, Vec2::new(320.0, 80.0), transform, true);
    tx.commit(&mut ui, &mut ui_history).unwrap();
    let mut history = History::default();
    let revision = agent.revision;
    let summary = apply_ops(&mut agent, &mut history, ApplyRequest { expected_revision: revision, dry_run: false, ops: vec![resized(frame, 320.0)] }).unwrap();
    assert_eq!(agent.nodes, ui.nodes);
    assert_eq!(agent.get(child).unwrap().size.x, 260.0);
    assert!(summary.changes.iter().any(|change| change.id == child && change.derived));
    let changed_pixels = pixels(&agent, page);
    assert_eq!(changed_pixels, pixels(&ui, page));
    assert_ne!(changed_pixels, original_pixels);
    assert!(history.undo(&mut agent).unwrap());
    assert!(!history.can_undo(), "the resize and derived layout must share one undo step");
    assert_eq!(agent.nodes, before);
    assert_eq!(pixels(&agent, page), original_pixels);
    assert!(history.redo(&mut agent).unwrap());
    assert_eq!(agent.nodes, ui.nodes);
    assert_eq!(pixels(&agent, page), changed_pixels);
}

#[test]
fn dry_run_and_failed_batch_preserve_document_and_redo() {
    let (mut doc, _, frame, _) = fixture();
    let mut history = History::default();
    let revision = doc.revision;
    apply_ops(&mut doc, &mut history, ApplyRequest { expected_revision: revision, dry_run: false, ops: vec![resized(frame, 240.0)] }).unwrap();
    history.undo(&mut doc).unwrap();
    let before = doc.to_json().unwrap();
    let revision = doc.revision;
    let preview = apply_ops(&mut doc, &mut history, ApplyRequest { expected_revision: revision, dry_run: true, ops: vec![resized(frame, 300.0)] }).unwrap();
    assert!(preview.dry_run);
    assert_eq!(doc.to_json().unwrap(), before);
    assert!(!history.can_undo());
    assert!(history.can_redo());
    let error = apply_ops(
        &mut doc,
        &mut history,
        ApplyRequest { expected_revision: revision, dry_run: false, ops: vec![resized(frame, 300.0), resized(NodeId(u64::MAX), 100.0)] },
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::MissingNode);
    assert_eq!(doc.to_json().unwrap(), before);
    assert!(!history.can_undo());
    assert!(history.can_redo());
    history.redo(&mut doc).unwrap();
    assert_eq!(doc.get(frame).unwrap().size.x, 240.0);
}

#[test]
fn stale_context_is_rejected_after_edit_and_undo() {
    let (mut doc, _, frame, _) = fixture();
    let mut history = History::default();
    let revision = doc.revision;
    apply_ops(&mut doc, &mut history, ApplyRequest { expected_revision: revision, dry_run: false, ops: vec![resized(frame, 250.0)] }).unwrap();
    history.undo(&mut doc).unwrap();
    let before = doc.to_json().unwrap();
    assert!(doc.revision > revision);
    let error = apply_ops(&mut doc, &mut history, ApplyRequest { expected_revision: revision, dry_run: false, ops: vec![resized(frame, 270.0)] }).unwrap_err();
    assert_eq!(error.code, ErrorCode::StaleRevision);
    assert_eq!(doc.to_json().unwrap(), before);
    assert!(history.can_redo());
}

#[test]
fn native_save_reopen_preserves_agent_node_identity() {
    let (doc, page, frame, child) = fixture();
    let mut reopened = Document::from_json(&doc.to_json().unwrap()).unwrap();
    assert_eq!(reopened.nodes, doc.nodes);
    assert_eq!(reopened.get(frame).unwrap().parent, Some(page));
    assert!(reopened.get(frame).unwrap().children.contains(&child));
    assert!(reopened.try_alloc_id().unwrap().0 > reopened.nodes.keys().map(|id| id.0).max().unwrap());
}

#[test]
fn create_instance_and_switch_variant_preserve_override_and_undo() {
    let mut doc = Document::new("Variants");
    let page = doc.add_page("Page");
    let mut components = Vec::new();
    for (name, width) in [("Size=Small", 80.0), ("Size=Large", 160.0)] {
        let id = doc.alloc_id();
        let mut node = Node::frame(id, Rect::new(0.0, 0.0, width, 40.0));
        node.kind = NodeKind::Component(FrameProps::default());
        node.name = name.into();
        doc.insert(page, node).unwrap();
        let child = doc.alloc_id();
        let mut node = Node::rectangle(child, Rect::new(5.0, 5.0, width - 10.0, 30.0));
        node.name = "background".into();
        doc.insert(id, node).unwrap();
        components.push(id);
    }
    let command = combine_as_variants(&mut doc, &components);
    command.apply(&mut doc).unwrap();
    let mut history = History::default();
    let revision = doc.revision;
    let created = apply_ops(
        &mut doc,
        &mut history,
        ApplyRequest {
            expected_revision: revision,
            dry_run: false,
            ops: vec![Operation::CreateInstance { component: components[0], parent: page, at: Vec2::new(10.0, 80.0) }],
        },
    )
    .unwrap();
    let instance = created.created_node_ids.iter().copied().find(|id| doc.get(*id).is_some_and(is_top_instance)).unwrap();
    let child = doc.get(instance).unwrap().children[0];
    let revision = doc.revision;
    apply_ops(
        &mut doc,
        &mut history,
        ApplyRequest {
            expected_revision: revision,
            dry_run: false,
            ops: vec![Operation::SetProperties { id: child, properties: NodeProperties { opacity: Some(0.4), ..Default::default() } }],
        },
    )
    .unwrap();
    let before = doc.nodes.clone();
    let revision = doc.revision;
    apply_ops(
        &mut doc,
        &mut history,
        ApplyRequest { expected_revision: revision, dry_run: false, ops: vec![Operation::SetVariant { id: instance, component: components[1] }] },
    )
    .unwrap();
    assert_eq!(instance_component(doc.get(instance).unwrap()), Some(components[1]));
    let variant_child = doc.get(instance).unwrap().children[0];
    assert_eq!(doc.get(variant_child).unwrap().size.x, 150.0);
    assert_eq!(doc.get(variant_child).unwrap().opacity, 0.4);
    let after = doc.nodes.clone();
    history.undo(&mut doc).unwrap();
    assert_eq!(doc.nodes, before);
    history.redo(&mut doc).unwrap();
    assert_eq!(doc.nodes, after);
}

#[test]
fn variable_bound_fill_changes_render_and_undo_restores_binding_and_color() {
    use std::collections::BTreeMap;
    let (mut doc, page, _, child) = fixture();
    doc.variable_collections.insert(
        "theme".into(),
        VariableCollection {
            id: "theme".into(),
            name: "Theme".into(),
            modes: vec![VariableMode { id: "light".into(), name: "Light".into() }],
            default_mode: "light".into(),
            variables: vec!["brand".into()],
            key: None,
        },
    );
    doc.variables.insert(
        "brand".into(),
        Variable {
            id: "brand".into(),
            name: "Brand".into(),
            collection: "theme".into(),
            kind: VariableKind::Color,
            values: BTreeMap::from([("light".into(), VariableValue::Color(Color::WHITE))]),
            description: String::new(),
            scopes: Vec::new(),
            hidden_from_publishing: false,
            key: None,
        },
    );
    let mut history = History::default();
    let before = doc.nodes.clone();
    let revision = doc.revision;
    apply_ops(
        &mut doc,
        &mut history,
        ApplyRequest {
            expected_revision: revision,
            dry_run: false,
            ops: vec![Operation::SetFills {
                id: child,
                fills: vec![Paint::solid(Color::BLACK)],
                bindings: BTreeMap::from([("fills/0".into(), "brand".into())]),
            }],
        },
    )
    .unwrap();
    assert_eq!(doc.get(child).unwrap().variables.bindings["fills/0"], "brand");
    assert!(matches!(resolve_node(&doc, doc.get(child).unwrap()).fills[0].kind, PaintKind::Solid(color) if color == Color::WHITE));
    let white = pixels(&doc, page);
    let revision = doc.revision;
    apply_ops(
        &mut doc,
        &mut history,
        ApplyRequest {
            expected_revision: revision,
            dry_run: false,
            ops: vec![Operation::EditVariable { id: "brand".into(), mode: "light".into(), value: VariableValue::Color(Color::BLACK) }],
        },
    )
    .unwrap();
    assert!(matches!(resolve_node(&doc, doc.get(child).unwrap()).fills[0].kind, PaintKind::Solid(color) if color == Color::BLACK));
    assert_ne!(pixels(&doc, page), white);
    history.undo(&mut doc).unwrap();
    assert_eq!(pixels(&doc, page), white);
    assert_eq!(doc.get(child).unwrap().variables.bindings["fills/0"], "brand");
    history.undo(&mut doc).unwrap();
    assert_eq!(doc.nodes, before);
    assert!(!history.can_undo());
}

#[test]
fn multi_node_batch_has_one_undo_and_locked_ancestor_blocks_entire_batch() {
    let (mut doc, _, frame, child) = fixture();
    let mut history = History::default();
    let before = doc.nodes.clone();
    let revision = doc.revision;
    let ops = vec![
        Operation::SetProperties { id: frame, properties: NodeProperties { name: Some("Container".into()), ..Default::default() } },
        Operation::SetProperties { id: child, properties: NodeProperties { name: Some("Content".into()), ..Default::default() } },
    ];
    apply_ops(&mut doc, &mut history, ApplyRequest { expected_revision: revision, dry_run: false, ops: ops.clone() }).unwrap();
    assert_eq!(doc.get(frame).unwrap().name, "Container");
    assert_eq!(doc.get(child).unwrap().name, "Content");
    history.undo(&mut doc).unwrap();
    assert_eq!(doc.nodes, before);
    assert!(!history.can_undo());
    doc.get_mut(frame).unwrap().locked = true;
    let before = doc.to_json().unwrap();
    let revision = doc.revision;
    let error = apply_ops(&mut doc, &mut history, ApplyRequest { expected_revision: revision, dry_run: false, ops }).unwrap_err();
    assert_eq!(error.code, ErrorCode::PermissionDenied);
    assert_eq!(doc.to_json().unwrap(), before);
    assert!(history.can_redo());
}
