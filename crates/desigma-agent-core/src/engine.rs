//! Typed, local document operations shared by the MCP transport and future bridges.
use crate::revision_history::RevisionArchive;
use desigma_doc::{Document, History, Node, NodeId, NodeKind};
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{json, Value};
use std::path::Path;

pub struct Engine {
    pub doc: Document,
    pub history: History,
    pub revision_archive: RevisionArchive,
}

impl Default for Engine {
    fn default() -> Self {
        let mut doc = Document::new("Untitled");
        doc.add_page("Page 1");
        let mut revision_archive = RevisionArchive::default();
        revision_archive.capture(&doc).expect("new document can be snapshotted");
        Self { doc, history: History::default(), revision_archive }
    }
}

impl Engine {
    pub fn call(&mut self, name: &str, args: Value) -> Result<Value, String> {
        self.call_with_workspace(name, args, None)
    }

    pub fn call_with_workspace(&mut self, name: &str, args: Value, workspace: Option<&Path>) -> Result<Value, String> {
        // Keep the archive aligned when an embedding replaces the document
        // between calls. A revision collision means a new document session,
        // since committed revisions are monotonic within one document.
        if let Err(error) = self.revision_archive.capture(&self.doc) {
            if error.starts_with("revision_conflict:") {
                self.revision_archive.reset(&self.doc)?;
            } else if !error.starts_with("snapshot_too_large:") {
                return Err(error);
            }
        }
        let before_revision = self.doc.revision;
        let dry_run = args.get("dry_run").and_then(Value::as_bool) == Some(true);
        let mutates_document = !dry_run
            && !matches!(
                name,
                "desigma_get_capabilities"
                    | "desigma_get_document"
                    | "desigma_list_styles"
                    | "desigma_get_history"
                    | "desigma_diff_revisions"
                    | "desigma_list_checkpoints"
                    | "desigma_checkpoint"
                    | "desigma_list_images"
                    | "desigma_list_publishable_assets"
                    | "desigma_create_library_snapshot"
                    | "desigma_list_local_components"
                    | "desigma_list_library_components"
                    | "desigma_list_pending_updates"
                    | "desigma_list_nodes"
                    | "desigma_get_node"
                    | "desigma_get_variables"
                    | "desigma_generate_code"
                    | "desigma_get_outline"
                    | "desigma_get_layout"
                    | "desigma_get_diagnostics"
                    | "desigma_render_node"
                    | "desigma_render_region"
                    | "desigma_render_diff"
                    | "desigma_list_fonts"
            );
        let before_doc = mutates_document.then(|| self.doc.clone());
        let before_history = mutates_document.then(|| self.history.clone());
        let result = self.call_inner(name, args, workspace);
        if result.is_ok() && self.doc.revision != before_revision {
            if let Err(error) = self.revision_archive.capture(&self.doc) {
                if !error.starts_with("snapshot_too_large:") {
                    if let Some(before_doc) = before_doc {
                        self.doc = before_doc;
                    }
                    if let Some(before_history) = before_history {
                        self.history = before_history;
                    }
                    return Err(error);
                }
            }
        }
        result
    }

    fn call_inner(&mut self, name: &str, args: Value, workspace: Option<&Path>) -> Result<Value, String> {
        if serde_json::to_vec(&args).map_err(|e| e.to_string())?.len() > 1_048_576 {
            return Err("invalid_operation: request exceeds 1 MiB".into());
        }
        if let Some(tool) = tools().iter().find(|tool| tool["name"] == name) {
            crate::contract::validate(&args, &tool["inputSchema"]).map_err(|e| format!("invalid_operation: {e}"))?;
        }
        let mut args = args;
        if matches!(name, "desigma_create_node" | "desigma_update_node" | "desigma_delete_node" | "desigma_undo" | "desigma_redo") {
            let fields = args.as_object_mut().ok_or("invalid_operation: expected object")?;
            let expected = fields
                .remove("expected_revision")
                .and_then(|v| v.as_u64())
                .ok_or("invalid_operation: expected_revision is required and must be a nonnegative integer")?;
            if expected != self.doc.revision {
                return Err(format!("stale_revision: expected {expected}, current {}", self.doc.revision));
            }
        }
        if matches!(
            name,
            "desigma_get_outline"
                | "desigma_get_layout"
                | "desigma_get_diagnostics"
                | "desigma_render_node"
                | "desigma_render_region"
                | "desigma_render_diff"
                | "desigma_list_fonts"
        ) {
            let mut result = crate::verification::call(&self.doc, name, args)?;
            result["revision"] = json!(self.doc.revision);
            return Ok(result);
        }
        if matches!(name, "desigma_import_image" | "desigma_import_svg") {
            let Some(workspace) = workspace else {
                return Err("unsupported_operation: workspace file imports require the MCP workspace context".into());
            };
            let (expected_revision, dry_run, path, parent) = if name == "desigma_import_image" {
                let a: AssetPathArgs = parse(args)?;
                (a.expected_revision, a.dry_run, a.path, None)
            } else {
                let a: SvgPathArgs = parse(args)?;
                (a.expected_revision, a.dry_run, a.path, Some(a.parent_id))
            };
            if expected_revision != self.doc.revision {
                return Err(format!("stale_revision: expected {expected_revision}, current {}", self.doc.revision));
            }
            if let Some(parent) = parent {
                self.editable(parent)?;
            }
            let mut trial = self.doc.clone();
            let mut history = self.history.clone();
            let mut tx = desigma_engine::edit::Tx::new();
            let asset = if name == "desigma_import_image" {
                let reference = desigma_engine::assets_ops::import_image_path(&mut trial, &mut tx, workspace, Path::new(&path))
                    .map_err(|e| format!("invalid_operation: {e}"))?;
                json!({"image_ref":reference})
            } else {
                let id = desigma_engine::assets_ops::import_svg_path(&mut trial, &mut tx, workspace, Path::new(&path), desigma_doc::NodeId(parent.unwrap()))
                    .map_err(|e| format!("invalid_operation: {e}"))?;
                json!({"node_id":id,"node":trial.get(id)})
            };
            tx.commit(&mut trial, &mut history).map_err(|e| e.to_string())?;
            let revision = if dry_run { self.doc.revision } else { trial.revision };
            if !dry_run {
                self.doc = trial;
                self.history = history;
            }
            return Ok(json!({"revision":revision,"dry_run":dry_run,"changed":!dry_run,"asset":asset}));
        }
        if matches!(
            name,
            "desigma_apply_ops"
                | "desigma_set_properties"
                | "desigma_set_fills"
                | "desigma_set_auto_layout"
                | "desigma_create_instance"
                | "desigma_set_variant"
                | "desigma_edit_variable"
                | "desigma_create_variable_collection"
                | "desigma_create_variable_mode"
                | "desigma_create_variable"
                | "desigma_rename_variable"
                | "desigma_delete_variable"
                | "desigma_bind_variable"
                | "desigma_unbind_variable"
                | "desigma_set_node_mode"
                | "desigma_move_node"
                | "desigma_reorder_children"
                | "desigma_set_text_patch"
                | "desigma_set_visual_patch"
                | "desigma_create_page"
                | "desigma_delete_page"
                | "desigma_reorder_pages"
                | "desigma_group_nodes"
                | "desigma_ungroup"
                | "desigma_wrap_in_frame"
                | "desigma_wrap_in_auto_layout"
                | "desigma_replace_variables"
                | "desigma_set_styles"
                | "desigma_create_style"
                | "desigma_update_style"
                | "desigma_delete_style"
                | "desigma_apply_style"
                | "desigma_set_vector_network"
                | "desigma_boolean_op"
                | "desigma_flatten"
                | "desigma_outline_stroke"
                | "desigma_set_dev_status"
                | "desigma_set_dev_annotations"
                | "desigma_set_variable_expression"
                | "desigma_combine_as_variants"
                | "desigma_duplicate_node"
                | "desigma_create_component"
                | "desigma_detach_instance"
                | "desigma_reset_instance"
                | "desigma_reset_overrides"
                | "desigma_set_component_properties"
                | "desigma_set_instance_property"
                | "desigma_set_reactions"
                | "desigma_set_flows"
                | "desigma_set_prototype_device"
                | "desigma_set_comments"
                | "desigma_add_comment"
                | "desigma_reply_comment"
                | "desigma_resolve_comment"
                | "desigma_create_connector"
                | "desigma_set_connector"
                | "desigma_add_slide"
                | "desigma_delete_slide"
                | "desigma_reorder_slide"
                | "desigma_set_slide_notes"
                | "desigma_set_export_presets"
                | "desigma_import_library_asset"
                | "desigma_apply_library_updates"
                | "desigma_enable_library"
                | "desigma_disable_library"
        ) {
            let request = if name == "desigma_apply_ops" {
                parse(args)?
            } else {
                let mut fields = args.as_object().cloned().ok_or("invalid_operation: expected object")?;
                let expected_revision = fields.remove("expected_revision").ok_or("invalid_operation: expected_revision is required")?;
                let dry_run = fields.remove("dry_run").unwrap_or(json!(false));
                fields.insert("op".into(), json!(name.trim_start_matches("desigma_")));
                parse(json!({"expected_revision":expected_revision,"dry_run":dry_run,"ops":[fields]}))?
            };
            return desigma_engine::apply_ops(&mut self.doc, &mut self.history, request).map(|v| json!(v)).map_err(engine_error);
        }
        match name {
            "desigma_get_capabilities" => {
                let _: Empty = parse(args)?;
                let tool_names: Vec<_> = tools().iter().filter_map(|tool| tool["name"].as_str().map(str::to_owned)).collect();
                Ok(
                    json!({"revision":self.doc.revision,"protocol_version":crate::LIVE_PROTOCOL_VERSION,"tool_catalog_version":crate::TOOL_CATALOG_VERSION,"tools":tool_names}),
                )
            }
            "desigma_get_document" => {
                let _: Empty = parse(args)?;
                Ok(json!({"revision":self.doc.revision,"name":self.doc.name,"kind":self.doc.kind,
                    "pages":self.doc.pages.iter().filter_map(|id|self.doc.get(*id)).map(summary).collect::<Vec<_>>(),
                    "node_count":self.doc.nodes.len(),"variable_count":self.doc.variables.len(),
                    "can_undo":self.history.can_undo(),"can_redo":self.history.can_redo(),"undo_label":self.history.undo_label()}))
            }
            "desigma_list_styles" => {
                let _: Empty = parse(args)?;
                Ok(json!({"revision":self.doc.revision,"styles":self.doc.styles,"style_meta":self.doc.style_meta}))
            }
            "desigma_list_components" => {
                let _: Empty = parse(args)?;
                let local = self
                    .doc
                    .nodes
                    .values()
                    .filter(|node| matches!(node.kind, NodeKind::Component(_) | NodeKind::ComponentSet(_)))
                    .map(|node| {
                        json!({"node_id":node.id,"name":node.name,"kind":node.kind.default_name(),
                        "variants":desigma_doc::components_of(&self.doc,node.id),"properties":node.component.props})
                    })
                    .collect::<Vec<_>>();
                let libraries = desigma_engine::library_ops::list_library_components(&self.doc);
                Ok(json!({"revision":self.doc.revision,"local":local,"libraries":libraries}))
            }
            "desigma_go_to_main_component" => {
                let a: NodeArgs = parse(args)?;
                let node = self.node(a.node_id)?;
                let main = desigma_doc::instance_component(node).ok_or("unsupported_operation: node is not a linked component instance")?;
                if self.doc.get(main).is_none() {
                    return Err(format!("missing_node: main component {} does not exist", main.0));
                }
                Ok(json!({"revision":self.doc.revision,"component_id":main.0}))
            }
            "desigma_list_flows" => {
                let _: Empty = parse(args)?;
                Ok(json!({"revision":self.doc.revision,"flows":self.doc.flows}))
            }
            "desigma_list_comments" => {
                let a: CommentListArgs = parse(args)?;
                if a.limit == 0 || a.limit > 500 {
                    return Err("invalid_operation: limit must be between 1 and 500".into());
                }
                let offset = list_cursor_offset(a.cursor.as_deref(), a.offset, self.doc.revision)?;
                let filtered = self
                    .doc
                    .comments
                    .iter()
                    .filter(|thread| a.page_id.is_none_or(|page| thread.anchor.page.0 == page))
                    .filter(|thread| a.include_resolved || !thread.resolved)
                    .count();
                let comments = desigma_engine::comments_ops::list_comments(&self.doc, a.page_id.map(NodeId), a.include_resolved, offset, a.limit)
                    .map_err(|e| format!("invalid_operation: {e}"))?;
                let next = offset.saturating_add(comments.len());
                Ok(json!({"revision":self.doc.revision,"comments":comments,"offset":offset,"limit":a.limit,"total":filtered,
                    "next_offset":(a.cursor.is_none() && next < filtered).then_some(next),
                    "next_cursor":(next < filtered).then(|| format!("{}:{}", self.doc.revision, next))}))
            }
            "desigma_get_history" => {
                let a: HistoryArgs = parse(args)?;
                if a.limit == 0 || a.limit > 500 {
                    return Err("limit must be between 1 and 500".into());
                }
                let entries = self.history.entries();
                let start = entries.len().saturating_sub(a.limit);
                Ok(json!({"revision":self.doc.revision,"entries":entries[start..],"total":entries.len()}))
            }
            "desigma_diff_revisions" => {
                let a: RevisionDiffArgs = parse(args)?;
                let diff = self.revision_archive.diff_revisions(a.from_revision, a.to_revision)?;
                Ok(json!({"revision":self.doc.revision,"diff":diff}))
            }
            "desigma_checkpoint" => {
                let a: CheckpointArgs = parse(args)?;
                self.check_revision(a.expected_revision)?;
                let checkpoint = self.revision_archive.create_checkpoint(&a.name, &self.doc)?;
                Ok(json!({"revision":self.doc.revision,"checkpoint":checkpoint,"changed":false}))
            }
            "desigma_list_checkpoints" => {
                let _: Empty = parse(args)?;
                Ok(json!({"revision":self.doc.revision,"checkpoints":self.revision_archive.list_checkpoints()}))
            }
            "desigma_restore_checkpoint" => {
                let a: CheckpointArgs = parse(args)?;
                self.check_revision(a.expected_revision)?;
                let restored = self.revision_archive.restore_checkpoint(&a.name, &self.doc)?;
                let revision = restored.restored_revision;
                let checkpoint_revision = restored.checkpoint_revision;
                let name = restored.name;
                self.doc = restored.document;
                self.history = restored.history;
                Ok(
                    json!({"revision":revision,"checkpoint_revision":checkpoint_revision,"name":name,"restored":true,"changed":true,"can_undo":false,"can_redo":false}),
                )
            }
            "desigma_list_images" => {
                let _: Empty = parse(args)?;
                let images = desigma_engine::assets_ops::list_images(&self.doc)
                    .into_iter()
                    .map(|v| json!({"reference":v.reference,"bytes":v.bytes,"format":v.format,"width":v.width,"height":v.height}))
                    .collect::<Vec<_>>();
                Ok(json!({"revision":self.doc.revision,"images":images}))
            }
            "desigma_list_publishable_assets" => {
                let _: Empty = parse(args)?;
                Ok(json!({"revision":self.doc.revision,"assets":desigma_engine::library_ops::list_publishable_assets(&self.doc)}))
            }
            "desigma_create_library_snapshot" => {
                let a: SnapshotArgs = parse(args)?;
                let snapshot = desigma_engine::library_ops::create_snapshot(&self.doc, &a.library_id, &a.name, a.version)
                    .map_err(|e| format!("invalid_operation: {e}"))?;
                Ok(json!({"revision":self.doc.revision,"snapshot":snapshot}))
            }
            "desigma_list_local_components" => {
                let _: Empty = parse(args)?;
                Ok(json!({"revision":self.doc.revision,"components":desigma_engine::library_ops::list_local_components(&self.doc)}))
            }
            "desigma_list_library_components" => {
                let _: Empty = parse(args)?;
                Ok(json!({"revision":self.doc.revision,"components":desigma_engine::library_ops::list_library_components(&self.doc)}))
            }
            "desigma_list_libraries" => {
                let _: Empty = parse(args)?;
                Ok(json!({"revision":self.doc.revision,"published":self.doc.library.published,"enabled":self.doc.library.enabled}))
            }
            "desigma_list_library_items" => {
                let a: LibraryItemsArgs = parse(args)?;
                if a.limit == 0 || a.limit > 500 {
                    return Err("invalid_operation: limit must be 1..500".into());
                }
                let items = self.doc.library.external.iter().filter(|(_,item)| a.library_id.as_ref().is_none_or(|id| &item.library_id == id))
                    .map(|(key,item)| json!({"key":key,"library_id":item.library_id,"version":item.version,"kind":item.kind,"node_id":item.node,"local_key":item.local_key}))
                    .collect::<Vec<_>>();
                let offset = list_cursor_offset(a.cursor.as_deref(), a.offset, self.doc.revision)?;
                let page = items.iter().skip(offset).take(a.limit).cloned().collect::<Vec<_>>();
                let next = offset.saturating_add(page.len());
                Ok(json!({"revision":self.doc.revision,"items":page,"total":items.len(),"offset":offset,
                    "next_offset":(a.cursor.is_none() && next<items.len()).then_some(next),
                    "next_cursor":(next<items.len()).then(|| format!("{}:{}", self.doc.revision, next))}))
            }
            "desigma_list_pending_updates" => {
                let a: SnapshotReadArgs = parse(args)?;
                let updates = desigma_engine::library_ops::list_pending_updates(&self.doc, &a.snapshot).map_err(|e| format!("invalid_operation: {e}"))?;
                let updates = updates
                    .into_iter()
                    .map(|u| json!({"key":u.key,"kind":format!("{:?}",u.kind),"name":u.name,"change":format!("{:?}",u.change)}))
                    .collect::<Vec<_>>();
                Ok(json!({"revision":self.doc.revision,"updates":updates}))
            }
            "desigma_list_nodes" => {
                let a: ListArgs = parse(args)?;
                if a.limit == 0 || a.limit > 500 {
                    return Err("limit must be between 1 and 500".into());
                }
                if a.cursor.is_some() && a.offset != 0 {
                    return Err("invalid_operation: use cursor or offset, not both".into());
                }
                let after_id = if let Some(cursor) = &a.cursor {
                    let (revision, id) = cursor.split_once(':').ok_or("invalid_operation: cursor must contain revision:id")?;
                    let revision = revision.parse::<u64>().map_err(|_| "invalid_operation: cursor revision is invalid")?;
                    if revision != self.doc.revision {
                        return Err(format!("stale_revision: cursor revision {revision}, current {}", self.doc.revision));
                    }
                    Some(id.parse::<u64>().map_err(|_| "invalid_operation: cursor node ID is invalid")?)
                } else {
                    None
                };
                if let Some(id) = a.parent_id {
                    self.node(id)?;
                }
                if let Some(q) = &a.query {
                    bounded_text("query", q, 1024)?;
                }
                let query = a.query.unwrap_or_default().to_lowercase();
                let matching: Vec<_> = self
                    .doc
                    .nodes
                    .values()
                    .filter(|n| {
                        a.parent_id.is_none_or(|id| n.parent == Some(NodeId(id)))
                            && n.name.to_lowercase().contains(&query)
                            && a.kind.as_ref().is_none_or(|kind| n.kind.default_name().eq_ignore_ascii_case(kind))
                            && a.style_id.as_ref().is_none_or(|id| n.style_refs.values().any(|value| value == id))
                            && a.variable_id.as_ref().is_none_or(|id| n.variables.bindings.values().any(|value| value == id))
                            && a.component_id.is_none_or(|id| matches!(&n.kind, NodeKind::Instance { component: Some(source), .. } if source.0 == id))
                    })
                    .collect();
                let page: Vec<_> = matching.iter().filter(|n| after_id.is_none_or(|id| n.id.0 > id)).skip(a.offset).take(a.limit).copied().collect();
                let nodes: Vec<_> = page.iter().map(|n| summary(n)).collect();
                let next = a.offset.saturating_add(nodes.len());
                let next_cursor =
                    page.last().and_then(|last| matching.iter().any(|n| n.id.0 > last.id.0).then(|| format!("{}:{}", self.doc.revision, last.id.0)));
                Ok(json!({"revision":self.doc.revision,"nodes":nodes,"total":matching.len(),"offset":a.offset,
                    "next_offset":(a.cursor.is_none() && next < matching.len()).then_some(next),"next_cursor":next_cursor}))
            }
            "desigma_get_node" => {
                let a: NodeArgs = parse(args)?;
                let node = self.node(a.node_id)?;
                let mut value = serde_json::to_value(node).map_err(|e| e.to_string())?;
                if let Some(fields) = a.fields {
                    if fields.is_empty() || fields.len() > 32 {
                        return Err("invalid_operation: fields must contain 1..32 property names".into());
                    }
                    let original = value.as_object().ok_or("operation_conflict: node cannot be projected")?;
                    let mut projected = serde_json::Map::new();
                    for field in fields {
                        if field.is_empty() || field.len() > 64 {
                            return Err("invalid_operation: field name must contain 1..64 bytes".into());
                        }
                        let v = original.get(&field).ok_or_else(|| format!("invalid_operation: fields/{field} is not a native node property"))?;
                        projected.insert(field, v.clone());
                    }
                    value = Value::Object(projected);
                }
                Ok(json!({"revision":self.doc.revision,"node":value}))
            }
            "desigma_get_variables" => {
                let a: VariablesArgs = parse(args)?;
                let effective = if let Some(id) = a.node_id {
                    let n = self.node(id)?;
                    Some(
                        n.variables
                            .bindings
                            .iter()
                            .map(|(property, variable)| {
                                json!({
                                    "property":property,"variable_id":variable,
                                    "mode_id":self.doc.variables.get(variable).and_then(|v|self.doc.variable_mode(n.id,&v.collection)),
                                    "value":self.doc.resolve_variable(n.id,variable)
                                })
                            })
                            .collect::<Vec<_>>(),
                    )
                } else {
                    None
                };
                Ok(
                    json!({"revision":self.doc.revision,"variables":self.doc.variables,"collections":self.doc.variable_collections,"effective_bindings":effective}),
                )
            }
            "desigma_generate_code" => {
                let a: CodeArgs = parse(args)?;
                let id = self.node(a.node_id)?.id;
                let doc = desigma_doc::apply_bound_variables(&self.doc);
                let code = match a.language.as_str() {
                    "css" => desigma_export::css_for(&doc, id),
                    "tailwind" => desigma_export::tailwind_for(&doc, id),
                    "react" => desigma_export::react_for(&doc, id),
                    "swiftui" => desigma_export::swiftui_for(&doc, id),
                    "compose" => desigma_export::compose_for(&doc, id),
                    "svg" => desigma_export::export_svg(&doc, id, &Default::default()),
                    _ => return Err("language must be css, tailwind, react, swiftui, compose, or svg".into()),
                };
                Ok(json!({"revision":self.doc.revision,"node_id":id,"language":a.language,"code":code}))
            }
            "desigma_create_node" => self.create(parse(args)?),
            "desigma_update_node" => self.update(parse(args)?),
            "desigma_delete_node" => {
                let a: NodeArgs = parse(args)?;
                let id = NodeId(a.node_id);
                let revision = self.doc.revision;
                let result = desigma_engine::apply_ops(
                    &mut self.doc,
                    &mut self.history,
                    desigma_engine::ApplyRequest {
                        expected_revision: revision,
                        dry_run: false,
                        ops: vec![desigma_engine::Operation::DeleteNode { id: desigma_engine::NodeRef::Id(id) }],
                    },
                )
                .map_err(engine_error)?;
                Ok(json!({"revision":result.revision,"node_id":id,"deleted":true,"changed":!result.changes.is_empty()}))
            }
            "desigma_undo" | "desigma_redo" => {
                let _: Empty = parse(args)?;
                let changed =
                    if name == "desigma_undo" { self.history.undo(&mut self.doc) } else { self.history.redo(&mut self.doc) }.map_err(|e| e.to_string())?;
                Ok(
                    json!({"revision":self.doc.revision,"changed":changed,"can_undo":self.history.can_undo(),"can_redo":self.history.can_redo(),"undo_label":self.history.undo_label()}),
                )
            }
            _ => Err(format!("unsupported_operation: unknown tool {name}")),
        }
    }

    fn node(&self, id: u64) -> Result<&Node, String> {
        if id == 0 {
            return Err("node IDs must be positive integers".into());
        }
        self.doc.get(NodeId(id)).ok_or_else(|| format!("missing_node: node {id} not found"))
    }

    fn check_revision(&self, expected: u64) -> Result<(), String> {
        if expected == self.doc.revision {
            Ok(())
        } else {
            Err(format!("stale_revision: expected {expected}, current {}", self.doc.revision))
        }
    }

    fn editable(&self, id: u64) -> Result<(), String> {
        let mut current = Some(NodeId(id));
        let mut seen = std::collections::BTreeSet::new();
        while let Some(id) = current {
            if !seen.insert(id) || seen.len() > 256 {
                return Err("operation_conflict: invalid ancestry".into());
            }
            let node = self.node(id.0)?;
            if node.locked {
                return Err(format!("permission_denied: node {} is locked", id.0));
            }
            current = node.parent;
        }
        Ok(())
    }

    fn create(&mut self, a: CreateArgs) -> Result<Value, String> {
        self.editable(a.parent_id)?;
        let parent = self.node(a.parent_id)?;
        if !parent.kind.is_container() {
            return Err("parent_id must identify a container".into());
        }
        let index = parent.children.len();
        geometry(a.x, a.y, a.width, a.height)?;
        if let Some(name) = &a.name {
            bounded_text("name", name, 1024)?;
        }
        if let Some(text) = &a.text {
            bounded_text("text", text, 100_000)?;
        }
        if a.kind != "text" && a.text.is_some() {
            return Err("text can only be set on text nodes".into());
        }
        let revision = self.doc.revision;
        let mut properties = json!({"x":a.x,"y":a.y,"width":a.width,"height":a.height});
        if let Some(name) = a.name {
            properties["name"] = json!(name);
        }
        if let Some(text) = a.text {
            properties["characters"] = json!(text);
        }
        let result = desigma_engine::apply_ops(
            &mut self.doc,
            &mut self.history,
            desigma_engine::ApplyRequest {
                expected_revision: revision,
                dry_run: false,
                ops: vec![desigma_engine::Operation::CreateNode {
                    temp_id: "$new1".into(),
                    parent: desigma_engine::NodeRef::Id(NodeId(a.parent_id)),
                    index: Some(index),
                    kind: a.kind,
                    properties: serde_json::from_value(properties).map_err(|e| e.to_string())?,
                    board: a.board,
                }],
            },
        )
        .map_err(engine_error)?;
        let id = result.temp_ids["$new1"];
        Ok(json!({"revision":self.doc.revision,"node":self.doc.get(id),"changed":true}))
    }

    fn update(&mut self, a: UpdateArgs) -> Result<Value, String> {
        self.editable(a.node_id)?;
        let mut node = self.node(a.node_id)?.clone();
        let properties = desigma_engine::NodeProperties {
            name: a.name.clone(),
            visible: a.visible,
            opacity: a.opacity,
            x: a.x,
            y: a.y,
            width: a.width,
            height: a.height,
            characters: a.text.clone().filter(|text| node.kind.any_text().is_none_or(|t| t.content != *text)),
            ..Default::default()
        };
        if node.parent.is_none() {
            return Err("page roots cannot be updated by this tool".into());
        }
        if let Some(name) = a.name {
            bounded_text("name", &name, 1024)?;
            node.name = name;
        }
        if let Some(x) = a.x {
            node.transform.tx = x;
        }
        if let Some(y) = a.y {
            node.transform.ty = y;
        }
        if let Some(width) = a.width {
            node.size.x = width;
        }
        if let Some(height) = a.height {
            node.size.y = height;
        }
        geometry(node.transform.tx, node.transform.ty, node.size.x, node.size.y)?;
        if let Some(visible) = a.visible {
            node.visible = visible;
        }
        if let Some(opacity) = a.opacity {
            if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {
                return Err("opacity must be between 0 and 1".into());
            }
            node.opacity = opacity;
        }
        if let Some(text) = a.text {
            bounded_text("text", &text, 100_000)?;
            let NodeKind::Text(props) = &mut node.kind else {
                return Err("text can only be set on text nodes".into());
            };
            if props.content != text {
                props.content = text;
                props.runs.clear();
                props.lists.clear();
            }
        }
        let changed = self.doc.get(node.id) != Some(&node);
        if changed {
            let revision = self.doc.revision;
            desigma_engine::apply_ops(
                &mut self.doc,
                &mut self.history,
                desigma_engine::ApplyRequest {
                    expected_revision: revision,
                    dry_run: false,
                    ops: vec![desigma_engine::Operation::SetProperties { id: node.id, properties }],
                },
            )
            .map_err(engine_error)?;
        }
        Ok(json!({"revision":self.doc.revision,"node":self.doc.get(NodeId(a.node_id)),"changed":changed}))
    }
}

fn engine_error(e: desigma_engine::EngineError) -> String {
    format!("{}: {}", serde_json::to_value(e.code).unwrap().as_str().unwrap_or("operation_conflict"), e.message)
}

fn summary(n: &Node) -> Value {
    json!({"node_id":n.id,"name":n.name,"kind":n.kind.default_name(),"parent_id":n.parent,
        "child_count":n.children.len(),"visible":n.visible,"x":n.transform.tx,"y":n.transform.ty,
        "width":n.size.x,"height":n.size.y})
}

fn parse<T: DeserializeOwned>(args: Value) -> Result<T, String> {
    serde_json::from_value(args).map_err(|e| format!("invalid arguments: {e}"))
}
fn bounded_text(field: &str, value: &str, max: usize) -> Result<(), String> {
    if value.len() > max {
        Err(format!("{field} exceeds {max} UTF-8 bytes"))
    } else {
        Ok(())
    }
}
fn geometry(x: f32, y: f32, width: f32, height: f32) -> Result<(), String> {
    if [x, y, width, height].iter().any(|v| !v.is_finite() || v.abs() > 1_000_000.0) {
        return Err("geometry must be finite and within +/-1000000".into());
    }
    if width < 0.0 || height < 0.0 {
        return Err("width and height must be non-negative".into());
    }
    Ok(())
}
fn default_limit() -> usize {
    100
}
fn default_size() -> f32 {
    100.0
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Empty {}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HistoryArgs {
    #[serde(default = "history_limit")]
    limit: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CommentListArgs {
    page_id: Option<u64>,
    #[serde(default)]
    include_resolved: bool,
    cursor: Option<String>,
    #[serde(default)]
    offset: usize,
    #[serde(default = "history_limit")]
    limit: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RevisionDiffArgs {
    from_revision: u64,
    to_revision: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CheckpointArgs {
    name: String,
    expected_revision: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SnapshotArgs {
    library_id: String,
    name: String,
    version: u32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SnapshotReadArgs {
    snapshot: desigma_doc::LibrarySnapshot,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LibraryItemsArgs {
    library_id: Option<String>,
    cursor: Option<String>,
    #[serde(default)]
    offset: usize,
    #[serde(default = "history_limit")]
    limit: usize,
}
fn history_limit() -> usize {
    100
}

fn list_cursor_offset(cursor: Option<&str>, offset: usize, revision: u64) -> Result<usize, String> {
    let Some(cursor) = cursor else { return Ok(offset) };
    if offset != 0 {
        return Err("invalid_operation: use cursor or offset, not both".into());
    }
    let (cursor_revision, position) = cursor.split_once(':').ok_or("invalid_operation: cursor must contain revision:offset")?;
    let cursor_revision = cursor_revision.parse::<u64>().map_err(|_| "invalid_operation: cursor revision is invalid")?;
    if cursor_revision != revision {
        return Err(format!("stale_revision: cursor revision {cursor_revision}, current {revision}"));
    }
    position.parse::<usize>().map_err(|_| "invalid_operation: cursor offset is invalid".into())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AssetPathArgs {
    path: String,
    expected_revision: u64,
    #[serde(default)]
    dry_run: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SvgPathArgs {
    path: String,
    parent_id: u64,
    expected_revision: u64,
    #[serde(default)]
    dry_run: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NodeArgs {
    node_id: u64,
    fields: Option<Vec<String>>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ListArgs {
    parent_id: Option<u64>,
    query: Option<String>,
    kind: Option<String>,
    style_id: Option<String>,
    variable_id: Option<String>,
    component_id: Option<u64>,
    cursor: Option<String>,
    #[serde(default)]
    offset: usize,
    #[serde(default = "default_limit")]
    limit: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VariablesArgs {
    node_id: Option<u64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CodeArgs {
    node_id: u64,
    language: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateArgs {
    parent_id: u64,
    kind: String,
    name: Option<String>,
    text: Option<String>,
    #[serde(default)]
    x: f32,
    #[serde(default)]
    y: f32,
    #[serde(default = "default_size")]
    width: f32,
    #[serde(default = "default_size")]
    height: f32,
    board: Option<desigma_engine::BoardNodeProperties>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateArgs {
    node_id: u64,
    name: Option<String>,
    text: Option<String>,
    x: Option<f32>,
    y: Option<f32>,
    width: Option<f32>,
    height: Option<f32>,
    visible: Option<bool>,
    opacity: Option<f32>,
}

/// JSON tool descriptors kept beside the argument validators.
pub fn tools() -> Vec<Value> {
    let id = json!({"type":"integer","minimum":1,"description":"Desigma numeric node ID; obtain from list_nodes/get_document."});
    let coordinate = json!({"type":"number","minimum":-1000000,"maximum":1000000});
    let size = json!({"type":"number","minimum":0,"maximum":1000000});
    let text = json!({"type":"string","maxLength":100000,"description":"Plain text; limited to 100000 UTF-8 bytes."});
    let name = json!({"type":"string","maxLength":1024,"description":"Layer name; limited to 1024 UTF-8 bytes."});
    let mut out = Vec::new();
    let mut add = |tool: &str, description: &str, properties: Value, required: Vec<&str>, read_only: bool, idempotent: bool| {
        out.push(json!({"name":format!("desigma_{tool}"),"description":description,
            "inputSchema":{"type":"object","properties":properties,"required":required,"additionalProperties":false},
            "annotations":{"readOnlyHint":read_only,"destructiveHint":!read_only,"idempotentHint":idempotent,"openWorldHint":false}}));
    };
    add("get_document", "Summarize the current in-memory document, pages, counts, and undo state.", json!({}), vec![], true, true);
    add("list_styles", "List native shared styles and metadata. Example: {}.", json!({}), vec![], true, true);
    add("list_components", "List local and imported components, variants, and property definitions. Example: {}.", json!({}), vec![], true, true);
    add("list_libraries", "List the document's published library identity and enabled libraries. Example: {}.", json!({}), vec![], true, true);
    add(
        "list_library_items",
        "List locally imported library assets, optionally filtering by library ID. Example: {\"library_id\":\"brand\",\"limit\":50}.",
        json!({"library_id":{"type":"string","maxLength":1024},"cursor":{"type":"string","maxLength":128},"offset":{"type":"integer","minimum":0},"limit":{"type":"integer","minimum":1,"maximum":500}}),
        vec![],
        true,
        true,
    );
    add(
        "go_to_main_component",
        "Get the main component ID linked to an instance. Example: {\"node_id\":42}.",
        json!({"node_id":id}),
        vec!["node_id"],
        true,
        true,
    );
    add("list_flows", "List prototype flow start frames and pages. Example: {}.", json!({}), vec![], true, true);
    add(
        "list_comments",
        "List document comment threads with page and resolved-state filters. Example: {\"page_id\":1,\"limit\":50}.",
        json!({"page_id":id,"include_resolved":{"type":"boolean"},"cursor":{"type":"string","maxLength":128},"offset":{"type":"integer","minimum":0},"limit":{"type":"integer","minimum":1,"maximum":500}}),
        vec![],
        true,
        true,
    );
    add(
        "list_nodes",
        "List node summaries in numeric ID order. Filter by parent, name, kind, style, variable, or component source. Use next_cursor for revision-stable pagination. Example: {\"kind\":\"Rectangle\",\"limit\":50}.",
        json!({"parent_id":id,"query":{"type":"string","maxLength":1024},"kind":{"type":"string","maxLength":64},"style_id":{"type":"string","maxLength":1024},"variable_id":{"type":"string","maxLength":1024},"component_id":id,"cursor":{"type":"string","maxLength":128},"offset":{"type":"integer","minimum":0,"default":0},"limit":{"type":"integer","minimum":1,"maximum":500,"default":100}}),
        vec![],
        true,
        true,
    );
    add(
        "get_node",
        "Read a native node. Set fields to select top-level properties and reduce output. Example: {\"node_id\":2,\"fields\":[\"id\",\"name\",\"kind\"]}.",
        json!({"node_id":id,"fields":{"type":"array","items":{"type":"string","maxLength":64},"minItems":1,"maxItems":32}}),
        vec!["node_id"],
        true,
        true,
    );
    add(
        "get_variables",
        "Read variable definitions and collections. With node_id, also resolve that node's direct bindings using inherited collection modes.",
        json!({"node_id":id}),
        vec![],
        true,
        true,
    );
    add(
        "generate_code",
        "Generate developer handoff code or SVG for a layer using resolved variable values. Returns source text; writes no files.",
        json!({"node_id":id,"language":{"type":"string","enum":["css","tailwind","react","swiftui","compose","svg"]}}),
        vec!["node_id", "language"],
        true,
        true,
    );
    add("create_node","Create one supported layer in a container; defaults to x/y=0, width/height=100. Board kinds accept validated board properties. Supports undo. Example: {\"expected_revision\":0,\"parent_id\":1,\"kind\":\"rectangle\"}.",
        json!({"parent_id":id,"kind":{"type":"string","enum":["frame","group","section","rectangle","ellipse","line","polygon","star","vector","text","boolean_op","sticky","shape_with_text","stamp"]},"name":name,"text":text,"x":coordinate,"y":coordinate,"width":size,"height":size,"board":{"type":"object","additionalProperties":true}}),vec!["parent_id","kind"],false,false);
    add("update_node","Update specified fields of one non-page layer. Coordinates are parent-relative. Replacing text clears text runs and list ranges. Supports undo; changes remain in memory until saved.",
        json!({"node_id":id,"name":name,"text":text,"x":coordinate,"y":coordinate,"width":size,"height":size,"visible":{"type":"boolean"},"opacity":{"type":"number","minimum":0,"maximum":1}}),vec!["node_id"],false,true);
    add(
        "delete_node",
        "Remove a non-page layer and its entire subtree. Supports undo; changes remain in memory until saved.",
        json!({"node_id":id}),
        vec!["node_id"],
        false,
        false,
    );
    add("undo", "Undo the latest successful edit in this session; returns changed=false if the stack is empty.", json!({}), vec![], false, false);
    add("redo", "Redo the most recently undone edit in this session; returns changed=false if the stack is empty.", json!({}), vec![], false, false);
    for tool in &mut out {
        if matches!(tool["name"].as_str(), Some("desigma_create_node" | "desigma_update_node" | "desigma_delete_node" | "desigma_undo" | "desigma_redo")) {
            tool["inputSchema"]["properties"]["expected_revision"] = json!({"type":"integer","minimum":0});
            tool["inputSchema"]["required"].as_array_mut().unwrap().push(json!("expected_revision"));
        }
    }
    out.extend(crate::contract::tools());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    trait Current {
        fn call_current(&mut self, name: &str, args: Value) -> Result<Value, String>;
    }
    impl Current for Engine {
        fn call_current(&mut self, name: &str, mut args: Value) -> Result<Value, String> {
            if matches!(
                name,
                "desigma_create_node"
                    | "desigma_update_node"
                    | "desigma_delete_node"
                    | "desigma_undo"
                    | "desigma_redo"
                    | "desigma_checkpoint"
                    | "desigma_restore_checkpoint"
                    | "desigma_apply_ops"
            ) {
                args["expected_revision"] = json!(self.doc.revision);
            }
            self.call(name, args)
        }
    }

    #[test]
    fn variable_inspection_and_codegen_resolve_inherited_cross_collection_aliases() {
        use desigma_doc::{Variable, VariableCollection, VariableKind, VariableMode, VariableValue};
        let mut e = Engine::default();
        let id = create(&mut e, "rectangle", "Box");
        for collection in ["alias", "size"] {
            e.doc.variable_collections.insert(
                collection.into(),
                VariableCollection {
                    id: collection.into(),
                    name: collection.into(),
                    modes: vec![VariableMode { id: "light".into(), name: "Light".into() }, VariableMode { id: "dark".into(), name: "Dark".into() }],
                    default_mode: "light".into(),
                    variables: vec![],
                    key: None,
                },
            );
        }
        for (id, collection, light, dark) in [
            ("width", "size", VariableValue::Number(100.0), VariableValue::Number(240.0)),
            ("bound", "alias", VariableValue::Alias("width".into()), VariableValue::Alias("width".into())),
        ] {
            e.doc.variables.insert(
                id.into(),
                Variable {
                    id: id.into(),
                    name: id.into(),
                    collection: collection.into(),
                    kind: VariableKind::Number,
                    values: [("light".into(), light), ("dark".into(), dark)].into_iter().collect(),
                    description: String::new(),
                    scopes: vec![],
                    hidden_from_publishing: false,
                    key: None,
                },
            );
        }
        e.doc.get_mut(NodeId(1)).unwrap().variables.modes.insert("size".into(), "dark".into());
        e.doc.get_mut(NodeId(id)).unwrap().variables.bindings.insert("width".into(), "bound".into());
        let before = e.doc.to_json().unwrap();
        let values = e.call_current("desigma_get_variables", json!({"node_id":id})).unwrap();
        assert_eq!(values["effective_bindings"][0]["value"], json!({"Number":240.0}));
        let svg = e.call_current("desigma_generate_code", json!({"node_id":id,"language":"svg"})).unwrap();
        assert!(svg["code"].as_str().unwrap().contains("width=\"240\""));
        assert_eq!(e.doc.to_json().unwrap(), before);
    }
    fn create(e: &mut Engine, kind: &str, name: &str) -> u64 {
        e.call_current("desigma_create_node", json!({"parent_id":1,"kind":kind,"name":name})).unwrap()["node"]["id"].as_u64().unwrap()
    }
    #[test]
    fn crud_round_trips_through_undo_and_redo() {
        let mut e = Engine::default();
        let id = create(&mut e, "text", "Title");
        assert_eq!(e.revision_archive.snapshot_revisions(), vec![0, 1]);
        let before = e.doc.to_json().unwrap();
        e.call_current("desigma_update_node", json!({"node_id":id,"text":"Hello","x":17,"visible":false,"opacity":0.5})).unwrap();
        assert_eq!(e.doc.get(NodeId(id)).unwrap().transform.tx, 17.0);
        e.call_current("desigma_undo", json!({})).unwrap();
        assert_eq!(e.revision_archive.snapshot_revisions(), vec![0, 1, 2, 3]);
        let mut restored: Value = serde_json::from_str(&e.doc.to_json().unwrap()).unwrap();
        let mut original: Value = serde_json::from_str(&before).unwrap();
        restored.as_object_mut().unwrap().remove("revision");
        original.as_object_mut().unwrap().remove("revision");
        assert_eq!(restored, original);
        e.call_current("desigma_redo", json!({})).unwrap();
        assert_eq!(e.revision_archive.snapshot_revisions(), vec![0, 1, 2, 3, 4]);
        assert_eq!(e.doc.get(NodeId(id)).unwrap().kind.any_text().unwrap().content, "Hello");
        e.call_current("desigma_delete_node", json!({"node_id":id})).unwrap();
        assert!(e.doc.get(NodeId(id)).is_none());
        e.call_current("desigma_undo", json!({})).unwrap();
        assert!(e.doc.get(NodeId(id)).is_some());
    }

    #[test]
    fn revision_diff_and_named_checkpoint_tools_round_trip() {
        let mut e = Engine::default();
        let id = create(&mut e, "rectangle", "Before");
        e.call_current("desigma_checkpoint", json!({"name":"baseline"})).unwrap();
        e.call_current("desigma_update_node", json!({"node_id":id,"name":"After"})).unwrap();

        let diff = e.call_current("desigma_diff_revisions", json!({"from_revision":1,"to_revision":e.doc.revision})).unwrap();
        let changed_node = diff["diff"]["changed_nodes"].as_array().unwrap().iter().find(|entry| entry["node_id"] == id).unwrap();
        assert!(changed_node["changed_fields"].as_array().unwrap().contains(&json!("name")));
        assert_eq!(e.call_current("desigma_list_checkpoints", json!({})).unwrap()["checkpoints"][0]["name"], "baseline");

        let current_revision = e.doc.revision;
        let restored = e.call_current("desigma_restore_checkpoint", json!({"name":"baseline"})).unwrap();
        assert_eq!(restored["revision"], current_revision + 1);
        assert_eq!(e.doc.get(NodeId(id)).unwrap().name, "Before");
        assert!(!e.history.can_undo());
        assert_eq!(e.call_current("desigma_list_checkpoints", json!({})).unwrap()["checkpoints"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn revision_tools_validate_stale_revisions_and_do_not_capture_dry_runs() {
        let mut e = Engine::default();
        let dry =
            e.call_current("desigma_apply_ops", json!({"dry_run":true,"ops":[{"op":"create_node","temp_id":"$new1","parent":1,"kind":"rectangle"}]})).unwrap();
        assert_eq!(dry["dry_run"], true);
        assert_eq!(e.doc.revision, 0);
        assert_eq!(e.revision_archive.snapshot_revisions(), vec![0]);
        assert!(e.call_current("desigma_diff_revisions", json!({"from_revision":0,"to_revision":1})).unwrap_err().starts_with("revision_not_found:"));
        assert!(e.call("desigma_checkpoint", json!({"name":"stale","expected_revision":1})).unwrap_err().starts_with("stale_revision:"));
        assert!(e.call_current("desigma_list_checkpoints", json!({})).unwrap()["checkpoints"].as_array().unwrap().is_empty());
    }

    #[test]
    fn revision_history_tools_are_listed_with_correct_access_annotations() {
        let tools = tools();
        for (name, readonly) in
            [("desigma_diff_revisions", true), ("desigma_checkpoint", false), ("desigma_list_checkpoints", true), ("desigma_restore_checkpoint", false)]
        {
            let tool = tools.iter().find(|tool| tool["name"] == name).unwrap();
            assert_eq!(tool["annotations"]["readOnlyHint"], readonly);
            assert_eq!(tool["inputSchema"]["additionalProperties"], false);
        }
    }
    #[test]
    fn invalid_mutations_leave_document_and_history_unchanged() {
        let mut e = Engine::default();
        let id = create(&mut e, "rectangle", "Box");
        let before = e.doc.to_json().unwrap();
        for (name, args) in [
            ("desigma_create_node", json!({"parent_id":1,"kind":"bad"})),
            ("desigma_create_node", json!({"parent_id":id,"kind":"frame"})),
            ("desigma_create_node", json!({"parent_id":1,"kind":"text","width":-1})),
            ("desigma_update_node", json!({"node_id":id,"name":"changed","text":"invalid"})),
            ("desigma_update_node", json!({"node_id":id,"x":1e100})),
            ("desigma_update_node", json!({"node_id":id,"opacity":2})),
            ("desigma_update_node", json!({"node_id":id,"unknown":true})),
            ("desigma_delete_node", json!({"node_id":1})),
            ("desigma_delete_node", json!({"node_id":0})),
        ] {
            assert!(e.call_current(name, args).is_err(), "{name}");
            assert_eq!(e.doc.to_json().unwrap(), before);
        }
        assert_eq!(e.revision_archive.snapshot_revisions(), vec![0, 1]);
        e.call_current("desigma_undo", json!({})).unwrap();
        assert!(!e.history.can_undo());
        assert!(e.doc.get(NodeId(id)).is_none());
    }
    #[test]
    fn name_filter_pagination_and_code_generation() {
        let mut e = Engine::default();
        let id = create(&mut e, "frame", "Card");
        create(&mut e, "rectangle", "card background");
        create(&mut e, "ellipse", "Icon");
        let result = e.call_current("desigma_list_nodes", json!({"parent_id":1,"query":"CARD","limit":1})).unwrap();
        assert_eq!(result["total"], 2);
        assert_eq!(result["nodes"][0]["node_id"], id);
        assert_eq!(result["next_offset"], 1);
        let cursor = result["next_cursor"].as_str().unwrap();
        let next_page = e.call_current("desigma_list_nodes", json!({"parent_id":1,"query":"CARD","limit":1,"cursor":cursor})).unwrap();
        assert_eq!(next_page["nodes"][0]["name"], "card background");
        assert!(next_page["next_offset"].is_null());
        let filtered = e.call_current("desigma_list_nodes", json!({"kind":"rectangle"})).unwrap();
        assert_eq!(filtered["total"], 1);
        let projected = e.call_current("desigma_get_node", json!({"node_id":id,"fields":["name","id"]})).unwrap();
        assert_eq!(projected["node"].as_object().unwrap().len(), 2);
        assert!(e.call_current("desigma_get_node", json!({"node_id":id,"fields":["bogus"]})).is_err());
        for language in ["css", "tailwind", "react", "swiftui", "compose", "svg"] {
            let output = e.call_current("desigma_generate_code", json!({"node_id":id,"language":language})).unwrap();
            assert!(!output["code"].as_str().unwrap().is_empty());
        }
        assert!(e.call_current("desigma_list_nodes", json!({"limit":501})).is_err());
        assert!(e.call_current("desigma_get_node", json!({"node_id":"2"})).is_err());
    }
    #[test]
    fn component_flow_and_comment_inspection_returns_revision() {
        let mut e = Engine::default();
        let id = create(&mut e, "frame", "Button");
        e.call("desigma_create_component", json!({"expected_revision":e.doc.revision,"id":id})).unwrap();
        let components = e.call_current("desigma_list_components", json!({})).unwrap();
        assert_eq!(components["revision"], e.doc.revision);
        assert!(components["local"].as_array().unwrap().iter().any(|item| item["node_id"] == id));
        assert_eq!(e.call_current("desigma_list_flows", json!({})).unwrap()["revision"], e.doc.revision);
        assert_eq!(e.call_current("desigma_list_comments", json!({})).unwrap()["revision"], e.doc.revision);
        assert_eq!(e.call_current("desigma_list_libraries", json!({})).unwrap()["revision"], e.doc.revision);
        assert_eq!(e.call_current("desigma_list_library_items", json!({})).unwrap()["total"], 0);
        assert!(e.call_current("desigma_go_to_main_component", json!({"node_id":id})).is_err());
    }

    #[test]
    fn comment_and_library_cursors_reject_stale_or_mixed_positions() {
        let mut e = Engine::default();
        for index in 0..2 {
            e.call(
                "desigma_add_comment",
                json!({"expected_revision":e.doc.revision,"thread_id":format!("thread-{index}"),"message_id":format!("message-{index}"),
                    "anchor":{"page":1,"node":null,"offset":{"x":0.0,"y":0.0}},"author":"Tester","text":"Review","created":"2026-10-15T00:00:00Z"}),
            )
            .unwrap();
        }
        let first = e.call_current("desigma_list_comments", json!({"limit":1})).unwrap();
        assert_eq!(first["total"], 2);
        let cursor = first["next_cursor"].as_str().unwrap().to_string();
        let second = e.call_current("desigma_list_comments", json!({"limit":1,"cursor":cursor})).unwrap();
        assert_eq!(second["comments"].as_array().unwrap().len(), 1);
        assert_eq!(second["next_cursor"], Value::Null);
        assert!(e.call_current("desigma_list_comments", json!({"cursor":cursor,"offset":1})).unwrap_err().starts_with("invalid_operation:"));
        assert!(e.call_current("desigma_list_library_items", json!({"cursor":cursor,"offset":1})).unwrap_err().starts_with("invalid_operation:"));
        e.call("desigma_create_node", json!({"expected_revision":e.doc.revision,"parent_id":1,"kind":"rectangle"})).unwrap();
        assert!(e.call_current("desigma_list_comments", json!({"cursor":cursor})).unwrap_err().starts_with("stale_revision:"));
        assert!(e.call_current("desigma_list_library_items", json!({"cursor":cursor})).unwrap_err().starts_with("stale_revision:"));
    }
}
