use serde_json::{json, Value};
fn object(properties: Value, required: &[&str]) -> Value {
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}
pub fn tools() -> Vec<Value> {
    let id = json!({"type":"integer","minimum":1});
    let number = json!({"type":"number","minimum":-1000000,"maximum":1000000});
    let size = json!({"type":"number","minimum":0,"maximum":1000000});
    let properties = object(
        json!({"name":{"type":"string","maxLength":1024},"visible":{"type":"boolean"},"locked":{"type":"boolean"},"opacity":{"type":"number","minimum":0,"maximum":1},"x":number,"y":number,"width":size,"height":size,"corner_radii":{"type":"array","items":size,"minItems":4,"maxItems":4},"characters":{"type":"string","maxLength":100000}}),
        &[],
    );
    let layout = object(
        json!({"direction":{"enum":["Horizontal","Vertical"]},"wrap":{"type":"boolean"},"item_spacing":number,"counter_spacing":number,"padding":{"type":"array","items":size,"minItems":4,"maxItems":4},"primary_align":{"enum":["Start","Center","End","SpaceBetween","Baseline"]},"counter_align":{"enum":["Start","Center","End","SpaceBetween","Baseline"]},"primary_sizing":{"enum":["Fixed","Hug","Fill"]},"counter_sizing":{"enum":["Fixed","Hug","Fill"]},"stroke_included":{"type":"boolean"},"reverse_z":{"type":"boolean"}}),
        &[
            "direction",
            "wrap",
            "item_spacing",
            "counter_spacing",
            "padding",
            "primary_align",
            "counter_align",
            "primary_sizing",
            "counter_sizing",
            "stroke_included",
            "reverse_z",
        ],
    );
    let unit = json!({"type":"number","minimum":0,"maximum":1});
    let color = object(json!({"r":unit,"g":unit,"b":unit,"a":unit}), &["r", "g", "b", "a"]);
    let board_properties = object(
        json!({
            "color":color,
            "author":{"type":"string","maxLength":256},
            "show_author":{"type":"boolean"},
            "shape":{"enum":["Square","RoundedRect","Ellipse","Diamond","Triangle","TriangleDown","Parallelogram","Star","Hexagon","Pentagon"]},
            "stamp_kind":{"enum":["thumbs_up","heart","star","question","plus_one"]}
        }),
        &[],
    );
    let point = object(json!({"x":number,"y":number}), &["x", "y"]);
    let node_ref = json!({"oneOf":[id,{"type":"string","pattern":"^\\$new[0-9]+$"}]});
    let gradient = object(
        json!({"handles":{"type":"array","items":point,"minItems":3,"maxItems":3},"stops":{"type":"array","maxItems":256,"items":object(json!({"pos":unit,"color":color}),&["pos","color"])}}),
        &["handles", "stops"],
    );
    let mut kinds = vec![object(json!({"Solid":color}), &["Solid"])];
    for name in ["LinearGradient", "RadialGradient", "AngularGradient", "DiamondGradient"] {
        let mut props = serde_json::Map::new();
        props.insert(name.into(), gradient.clone());
        kinds.push(object(Value::Object(props), &[name]));
    }
    kinds.push(object(json!({"Image":object(json!({"image_ref":{"type":"string","maxLength":1024},"scale_mode":{"enum":["Fill","Fit","Crop","Tile"]}}),&["image_ref","scale_mode"])}),&["Image"]));
    let paint = object(
        json!({"kind":{"oneOf":kinds},"opacity":unit,"visible":{"type":"boolean"},"blend":{"enum":["Normal","PassThrough","Darken","Multiply","ColorBurn","LinearBurn","Lighten","Screen","ColorDodge","LinearDodge","Overlay","SoftLight","HardLight","Difference","Exclusion","Hue","Saturation","Color","Luminosity"]}}),
        &["kind", "opacity", "visible", "blend"],
    );
    let variable_value = json!({"oneOf":[object(json!({"Color":color}),&["Color"]),object(json!({"Number":number}),&["Number"]),object(json!({"String":{"type":"string","maxLength":100000}}),&["String"]),object(json!({"Boolean":{"type":"boolean"}}),&["Boolean"]),object(json!({"Alias":{"type":"string","maxLength":1024}}),&["Alias"])]});
    let native = json!({"type":"object","additionalProperties":{}});
    let mut specs = vec![
        (
            "replace_variables",
            "Replace the validated variable store atomically, rebaking removed bindings and reflowing affected nodes.",
            object(json!({"variables":native,"collections":native}), &["variables", "collections"]),
        ),
        ("set_styles", "Replace shared styles and their metadata in one undo step.", object(json!({"styles":native,"style_meta":native}), &["styles"])),
        (
            "create_style",
            "Create a validated shared fill, text, effect or grid style.",
            object(json!({"key":{"type":"string","minLength":1,"maxLength":256},"style":native,"meta":native}), &["key", "style", "meta"]),
        ),
        (
            "update_style",
            "Update a shared style and refresh all compatible nodes that reference it in this undo step.",
            object(json!({"key":{"type":"string","minLength":1,"maxLength":256},"style":native,"meta":native}), &["key", "style"]),
        ),
        (
            "delete_style",
            "Delete a shared style and clear its references while preserving the current appearance.",
            object(json!({"key":{"type":"string","minLength":1,"maxLength":256}}), &["key"]),
        ),
        (
            "apply_style",
            "Apply a shared style to compatible nodes. Fill styles can target fill or stroke.",
            object(
                json!({"ids":{"type":"array","items":node_ref,"minItems":1,"maxItems":10000},"key":{"type":"string","minLength":1,"maxLength":256},"property":{"enum":["Fill","Stroke","Text","Effect","Grid"]}}),
                &["ids", "key"],
            ),
        ),
        ("create_component", "Convert a supported layer to a component.", object(json!({"id":node_ref}), &["id"])),
        (
            "duplicate_node",
            "Deep-copy a node and descendants, preserving component instance links. Returns a new temp handle and old-to-new ID mapping.",
            object(
                json!({"temp_id":{"type":"string","pattern":"^\\$new[0-9]+$"},"id":node_ref,"parent":node_ref,"index":{"type":"integer","minimum":0},"offset":object(json!({"x":number,"y":number}),&["x","y"])}),
                &["temp_id", "id"],
            ),
        ),
        ("detach_instance", "Detach an instance from its component.", object(json!({"id":node_ref}), &["id"])),
        ("reset_instance", "Reset an instance to its main component definition.", object(json!({"id":node_ref}), &["id"])),
        (
            "reset_overrides",
            "Clear selected component override fields on a node.",
            object(json!({"id":node_ref,"fields":{"type":"array","items":native}}), &["id", "fields"]),
        ),
        (
            "set_component_properties",
            "Set component property definitions.",
            object(json!({"id":node_ref,"props":{"type":"array","items":native}}), &["id", "props"]),
        ),
        (
            "set_instance_property",
            "Set or clear a component property value on an instance.",
            object(json!({"id":node_ref,"name":{"type":"string","maxLength":256},"value":native}), &["id", "name"]),
        ),
        (
            "set_reactions",
            "Replace a node's prototype reactions after validating destinations, expressions and transitions.",
            object(json!({"id":node_ref,"reactions":{"type":"array","items":native,"maxItems":128}}), &["id", "reactions"]),
        ),
        (
            "set_flows",
            "Replace prototype flows after validating page and start-frame references.",
            object(json!({"flows":{"type":"array","items":native,"maxItems":128}}), &["flows"]),
        ),
        ("set_prototype_device", "Set the bounded prototype device preview settings.", object(json!({"device":native}), &["device"])),
        (
            "set_variable_expression",
            "Evaluate a bounded prototype expression and store its result in a variable mode. Variable display names in expressions must be unique.",
            object(
                json!({"variable_id":{"type":"string","minLength":1,"maxLength":1024},"mode_id":{"type":"string","minLength":1,"maxLength":1024},"expression":{"type":"string","minLength":1,"maxLength":1024}}),
                &["variable_id", "mode_id", "expression"],
            ),
        ),
        (
            "add_comment",
            "Add a validated thread and opening message to a page or node anchor. IDs and RFC3339 timestamp are supplied by the caller.",
            object(
                json!({"thread_id":{"type":"string","minLength":1,"maxLength":128},"message_id":{"type":"string","minLength":1,"maxLength":128},"anchor":native,"author":{"type":"string","minLength":1,"maxLength":256},"text":{"type":"string","minLength":1,"maxLength":32768},"created":{"type":"string","minLength":20,"maxLength":64}}),
                &["thread_id", "message_id", "anchor", "author", "text", "created"],
            ),
        ),
        (
            "reply_comment",
            "Reply to an unresolved thread; reopen resolved threads first. Thread/message IDs and RFC3339 timestamp are caller supplied.",
            object(
                json!({"thread_id":{"type":"string","minLength":1,"maxLength":128},"message_id":{"type":"string","minLength":1,"maxLength":128},"author":{"type":"string","minLength":1,"maxLength":256},"text":{"type":"string","minLength":1,"maxLength":32768},"created":{"type":"string","minLength":20,"maxLength":64}}),
                &["thread_id", "message_id", "author", "text", "created"],
            ),
        ),
        (
            "resolve_comment",
            "Resolve or reopen a comment thread.",
            object(json!({"thread_id":{"type":"string","minLength":1,"maxLength":128},"resolved":{"type":"boolean"}}), &["thread_id", "resolved"]),
        ),
        (
            "set_dev_status",
            "Mark a supported frame, section or component Ready for dev, mark a tracked node Completed, or clear its status.",
            object(
                json!({"id":node_ref,"status":{"anyOf":[{"enum":["ReadyForDev","Completed","Changed"]},{"type":"null"}]},"updated_at":{"type":"string","minLength":20,"maxLength":64},"snapshot_version":{"type":"integer","minimum":0}}),
                &["id"],
            ),
        ),
        (
            "set_dev_annotations",
            "Replace a node's bounded Dev-mode handoff annotations while preserving its other Dev metadata.",
            object(json!({"id":node_ref,"annotations":{"type":"array","maxItems":256,"items":native}}), &["id", "annotations"]),
        ),
        (
            "set_comments",
            "Replace comment threads after validating anchors, sizes and timestamps.",
            object(json!({"comments":{"type":"array","items":native,"maxItems":10000}}), &["comments"]),
        ),
        (
            "create_connector",
            "Create a validated connector in a board document.",
            object(json!({"parent":node_ref,"connector":native,"temp_id":{"type":"string","pattern":"^\\$new[0-9]+$"}}), &["parent", "connector", "temp_id"]),
        ),
        (
            "set_connector",
            "Update an existing board connector's endpoints, routing or label.",
            object(json!({"id":node_ref,"connector":native}), &["id", "connector"]),
        ),
        (
            "add_slide",
            "Add a slide to a deck page.",
            object(
                json!({"page":node_ref,"after":node_ref,"name":{"type":"string","maxLength":256},"temp_id":{"type":"string","pattern":"^\\$new[0-9]+$"}}),
                &["page", "temp_id"],
            ),
        ),
        ("delete_slide", "Delete a slide while retaining at least one slide on its page.", object(json!({"id":node_ref}), &["id"])),
        (
            "reorder_slide",
            "Reorder a slide among the slides on its page.",
            object(json!({"id":node_ref,"index":{"type":"integer","minimum":0}}), &["id", "index"]),
        ),
        ("set_slide_notes", "Set presenter notes on a slide.", object(json!({"id":node_ref,"notes":{"type":"string","maxLength":100000}}), &["id", "notes"])),
        (
            "set_slide_transition",
            "Set or clear a slide transition. Example: {\"id\":2,\"transition\":{\"kind\":\"Dissolve\",\"easing\":\"EaseInOut\",\"duration_ms\":300}}.",
            object(json!({"id":node_ref,"transition":{"anyOf":[native,{"type":"null"}]}}), &["id", "transition"]),
        ),
        (
            "set_export_presets",
            "Replace bounded export settings for a layer or slide.",
            object(json!({"id":node_ref,"presets":{"type":"array","items":native,"maxItems":32}}), &["id", "presets"]),
        ),
        (
            "create_page",
            "Create a page in the document. temp_id can be used in later operations in the same batch.",
            object(
                json!({"temp_id":{"type":"string","pattern":"^\\$new[0-9]+$"},"name":{"type":"string","minLength":1,"maxLength":1024},"index":{"type":"integer","minimum":0}}),
                &["temp_id", "name"],
            ),
        ),
        (
            "delete_page",
            "Delete a page and its page-scoped flows, comments and guides; documents must retain one page.",
            object(json!({"id":node_ref}), &["id"]),
        ),
        (
            "reorder_pages",
            "Set the exact document page order.",
            object(json!({"order":{"type":"array","items":node_ref,"minItems":1,"maxItems":1000}}), &["order"]),
        ),
        (
            "group_nodes",
            "Group two or more sibling layers under a new group while preserving their world positions.",
            object(
                json!({"temp_id":{"type":"string","pattern":"^\\$new[0-9]+$"},"ids":{"type":"array","items":node_ref,"minItems":2,"maxItems":10000},"name":{"type":"string","maxLength":1024}}),
                &["temp_id", "ids"],
            ),
        ),
        ("ungroup", "Ungroup a group while preserving child order and positions.", object(json!({"id":node_ref}), &["id"])),
        (
            "wrap_in_frame",
            "Wrap sibling layers in a transparent frame while preserving their positions.",
            object(
                json!({"temp_id":{"type":"string","pattern":"^\\$new[0-9]+$"},"ids":{"type":"array","items":node_ref,"minItems":1,"maxItems":10000},"name":{"type":"string","maxLength":1024}}),
                &["temp_id", "ids"],
            ),
        ),
        (
            "wrap_in_auto_layout",
            "Wrap sibling layers in a frame with the supplied auto-layout configuration.",
            object(
                json!({"temp_id":{"type":"string","pattern":"^\\$new[0-9]+$"},"ids":{"type":"array","items":node_ref,"minItems":1,"maxItems":10000},"name":{"type":"string","maxLength":1024},"auto_layout":layout}),
                &["temp_id", "ids", "auto_layout"],
            ),
        ),
        (
            "set_text_patch",
            "Set native text style, runs, paragraph, list and resizing data with validated ranges.",
            object(
                json!({"id":id,"patch":object(json!({"style":{},"runs":{},"auto_resize":{},"max_lines":{},"paragraph":{},"lists":{},"direction":{}}),&[])}),
                &["id", "patch"],
            ),
        ),
        (
            "set_visual_patch",
            "Set native stroke, effects, blend, corner, mask, constraints, child layout, clipping or rotation properties.",
            object(
                json!({"id":id,"patch":object(json!({"stroke":{},"effects":{},"blend":{},"corner_smoothing":{},"is_mask":{},"constraints":{},"layout_child":{},"clips_content":{},"rotation":{}}),&[])}),
                &["id", "patch"],
            ),
        ),
        (
            "set_vector_network",
            "Replace an editable vector's native network after validating geometry and bounds.",
            object(
                json!({"id":node_ref,"network":object(json!({"vertices":{"type":"array","maxItems":100000,"items":native},"segments":{"type":"array","maxItems":100000,"items":native},"regions":{"type":"array","maxItems":100000,"items":native}}), &["vertices","segments","regions"])}),
                &["id", "network"],
            ),
        ),
        (
            "boolean_op",
            "Create a live boolean operation group around sibling vector-capable layers. temp_id can be referenced by later operations in the batch.",
            object(
                json!({"temp_id":{"type":"string","pattern":"^\\$new[0-9]+$"},"ids":{"type":"array","items":node_ref,"minItems":2,"maxItems":10000},"boolean_op":{"enum":["Union","Subtract","Intersect","Exclude"]}}),
                &["temp_id", "ids", "boolean_op"],
            ),
        ),
        (
            "flatten",
            "Flatten sibling geometry into one editable vector. temp_id can be referenced by later operations in the batch.",
            object(
                json!({"temp_id":{"type":"string","pattern":"^\\$new[0-9]+$"},"ids":{"type":"array","items":node_ref,"minItems":1,"maxItems":10000}}),
                &["temp_id", "ids"],
            ),
        ),
        (
            "outline_stroke",
            "Convert a layer's visible stroke to filled vector geometry. temp_id can be referenced by later operations in the batch.",
            object(json!({"temp_id":{"type":"string","pattern":"^\\$new[0-9]+$"},"id":node_ref}), &["temp_id", "id"]),
        ),
        (
            "move_node",
            "Move a layer under another container; indices are zero-based and IDs may reference earlier creates.",
            object(json!({"id":node_ref,"parent":node_ref,"index":{"type":"integer","minimum":0}}), &["id", "parent"]),
        ),
        (
            "reorder_children",
            "Set the exact back-to-front order of every direct child of a container.",
            object(json!({"parent":node_ref,"order":{"type":"array","items":node_ref,"maxItems":100000}}), &["parent", "order"]),
        ),
        (
            "set_properties",
            "Set supported properties with dependent layout, text and component updates.",
            object(json!({"id":id,"properties":properties}), &["id", "properties"]),
        ),
        (
            "set_fills",
            "Replace native paints and optional fills/<index> color-variable bindings. Obtain native paint structure from get_node.",
            object(
                json!({"id":id,"fills":{"type":"array","maxItems":64,"items":paint},"bindings":{"type":"object","additionalProperties":{"type":"string"}}}),
                &["id", "fills"],
            ),
        ),
        (
            "set_auto_layout",
            "Set a native auto-layout configuration, or null to remove auto layout.",
            object(json!({"id":id,"auto_layout":{"anyOf":[layout,{"type":"null"}]}}), &["id", "auto_layout"]),
        ),
        (
            "create_instance",
            "Create an instance of a local component at parent-relative coordinates.",
            object(json!({"component":id,"parent":id,"at":object(json!({"x":number,"y":number}),&["x","y"])}), &["component", "parent", "at"]),
        ),
        ("set_variant", "Switch an instance to another component variant.", object(json!({"id":id,"component":id}), &["id", "component"])),
        (
            "combine_as_variants",
            "Combine 2 or more sibling components into a new component set. Returns the generated set through temp_id for later operations in the same batch.",
            object(
                json!({"temp_id":{"type":"string","pattern":"^\\$new[0-9]+$"},"ids":{"type":"array","items":node_ref,"minItems":2,"maxItems":10000}}),
                &["temp_id", "ids"],
            ),
        ),
        (
            "edit_variable",
            "Set a variable mode value using native VariableValue (e.g. {Number:12}, {Color:{r,g,b,a}}, {Alias:\"id\"}).",
            object(json!({"id":{"type":"string"},"mode":{"type":"string"},"value":variable_value}), &["id", "mode", "value"]),
        ),
        (
            "create_variable_collection",
            "Create a collection and its default mode. Returns generated handles in temp_variable_ids; the default mode handle is <temp_id>_mode.",
            object(
                json!({"temp_id":{"type":"string","pattern":"^\\$new[0-9]+$"},"name":{"type":"string","minLength":1,"maxLength":256}}),
                &["temp_id", "name"],
            ),
        ),
        (
            "create_variable_mode",
            "Create a mode in a collection. collection_id accepts an existing ID or an earlier $newN handle.",
            object(
                json!({"temp_id":{"type":"string","pattern":"^\\$new[0-9]+$"},"collection_id":{"type":"string","minLength":1,"maxLength":1024},"name":{"type":"string","minLength":1,"maxLength":256}}),
                &["temp_id", "collection_id", "name"],
            ),
        ),
        (
            "create_variable",
            "Create a typed variable with default values in every collection mode. IDs can reference earlier $newN handles.",
            object(
                json!({"temp_id":{"type":"string","pattern":"^\\$new[0-9]+$"},"collection_id":{"type":"string","minLength":1,"maxLength":1024},"name":{"type":"string","minLength":1,"maxLength":256},"kind":{"enum":["Color","Number","String","Boolean"]}}),
                &["temp_id", "collection_id", "name", "kind"],
            ),
        ),
        (
            "rename_variable",
            "Rename a variable; duplicate names are disambiguated by the variable store.",
            object(json!({"id":{"type":"string","minLength":1,"maxLength":1024},"name":{"type":"string","minLength":1,"maxLength":256}}), &["id", "name"]),
        ),
        (
            "delete_variable",
            "Delete a variable, bake currently resolved bindings and detach aliases safely.",
            object(json!({"id":{"type":"string","minLength":1,"maxLength":1024}}), &["id"]),
        ),
        (
            "bind_variable",
            "Bind a supported node property to a type-compatible variable. Existing or earlier-created IDs are accepted.",
            object(
                json!({"id":node_ref,"property":{"type":"string","minLength":1,"maxLength":128},"variable_id":{"type":"string","minLength":1,"maxLength":1024}}),
                &["id", "property", "variable_id"],
            ),
        ),
        (
            "unbind_variable",
            "Remove a property binding while retaining its currently resolved appearance.",
            object(json!({"id":node_ref,"property":{"type":"string","minLength":1,"maxLength":128}}), &["id", "property"]),
        ),
        (
            "set_node_mode",
            "Set or clear an explicit collection mode on a node and reflow affected descendant bindings. IDs accept earlier $newN handles.",
            object(
                json!({"id":node_ref,"collection_id":{"type":"string","minLength":1,"maxLength":1024},"mode_id":{"anyOf":[{"type":"string","minLength":1,"maxLength":1024},{"type":"null"}]}}),
                &["id", "collection_id"],
            ),
        ),
        (
            "import_image",
            "Import a workspace-relative image file. Paths are canonicalized and must remain inside the session workspace.",
            object(json!({"path":{"type":"string","minLength":1,"maxLength":4096}}), &["path"]),
        ),
        (
            "import_svg",
            "Import a workspace-relative SVG as editable vector geometry under parent_id. Paths must remain inside the session workspace.",
            object(json!({"path":{"type":"string","minLength":1,"maxLength":4096},"parent_id":id}), &["path", "parent_id"]),
        ),
        (
            "import_library_asset",
            "Import a published component, style or variable asset into this document.",
            object(json!({"snapshot":native,"key":{"type":"string","maxLength":1024},"placement":{"anyOf":[native,{"type":"null"}]}}), &["snapshot", "key"]),
        ),
        (
            "apply_library_updates",
            "Apply selected pending library updates atomically.",
            object(json!({"snapshot":native,"keys":{"type":"array","items":{"type":"string","maxLength":1024},"maxItems":1000}}), &["snapshot", "keys"]),
        ),
        ("enable_library", "Enable a published library snapshot for this document.", object(json!({"snapshot":native}), &["snapshot"])),
        (
            "disable_library",
            "Disable a library by its library ID.",
            object(json!({"library_id":{"type":"string","minLength":1,"maxLength":1024}}), &["library_id"]),
        ),
    ];
    let mut ops = Vec::new();
    for (name, schema) in [
        (
            "create_node",
            object(
                json!({"temp_id":{"type":"string","pattern":"^\\$new[0-9]+$"},"parent":node_ref,"index":{"type":"integer","minimum":0},"kind":{"enum":["frame","group","section","rectangle","ellipse","line","polygon","star","vector","text","boolean_op","sticky","shape_with_text","stamp"]},"properties":properties,"board":board_properties}),
                &["temp_id", "parent", "kind"],
            ),
        ),
        ("delete_node", object(json!({"id":node_ref}), &["id"])),
    ] {
        let mut op = schema;
        op["properties"]["op"] = json!({"const":name});
        op["required"].as_array_mut().unwrap().push(json!("op"));
        ops.push(op);
    }
    for (name, _, schema) in &specs {
        if matches!(*name, "import_image" | "import_svg") {
            continue;
        }
        let mut op = schema.clone();
        op["properties"]["op"] = json!({"const":name});
        op["required"].as_array_mut().unwrap().push(json!("op"));
        ops.push(op);
    }
    specs.push((
        "apply_ops",
        "Apply 1..256 typed operations atomically as one undo step. A failed operation leaves the document and history unchanged.",
        object(json!({"ops":{"type":"array","minItems":1,"maxItems":256,"items":{"oneOf":ops}}}), &["ops"]),
    ));
    let mut out = Vec::new();
    for (name, description, mut schema) in specs {
        schema["properties"]["expected_revision"] = json!({"type":"integer","minimum":0,"description":"Revision read from get_document; stale requests fail."});
        schema["properties"]["dry_run"] =
            json!({"type":"boolean","default":false,"description":"Calculate and validate the full result without changing the document or history."});
        schema["required"].as_array_mut().unwrap().push(json!("expected_revision"));
        out.push(json!({"name":format!("desigma_{name}"),"description":description,"inputSchema":schema,"annotations":{"readOnlyHint":false,"destructiveHint":true,"idempotentHint":false,"openWorldHint":false}}));
    }
    for (name, description) in [
        ("get_outline", "Read a compact preorder layer outline. node_id scopes to a subtree; limit bounds output."),
        ("get_layout", "Read computed local/world bounds, transforms and auto layout from the current document."),
        (
            "get_diagnostics",
            "Inspect renderer-measured text overflow, unavailable fonts, detached instances and unresolved variables. limit bounds checked nodes.",
        ),
    ] {
        out.push(json!({"name":format!("desigma_{name}"),"description":description,"inputSchema":object(json!({"node_id":id,"limit":{"type":"integer","minimum":1,"maximum":1000,"default":500}}),&[]),"annotations":{"readOnlyHint":true,"openWorldHint":false}}));
    }
    for (name, description, schema) in [
        ("list_images", "List embedded raster image assets and their dimensions.", object(json!({}), &[])),
        ("list_publishable_assets", "List components, styles and variables exposed by this document as a library.", object(json!({}), &[])),
        (
            "create_library_snapshot",
            "Create an immutable published snapshot from selected document assets.",
            object(
                json!({"library_id":{"type":"string","minLength":1,"maxLength":1024},"name":{"type":"string","minLength":1,"maxLength":1024},"version":{"type":"integer","minimum":1}}),
                &["library_id", "name", "version"],
            ),
        ),
        ("list_local_components", "List local component definitions.", object(json!({}), &[])),
        ("list_library_components", "List imported library components.", object(json!({}), &[])),
        ("list_pending_updates", "Compare an imported library with a newer snapshot.", object(json!({"snapshot":native}), &["snapshot"])),
    ] {
        out.push(
            json!({"name":format!("desigma_{name}"),"description":description,"inputSchema":schema,"annotations":{"readOnlyHint":true,"openWorldHint":false}}),
        );
    }
    out.push(json!({"name":"desigma_render_node","description":"Render a page or layer using the editor CPU renderer and font resolution. Returns PNG by default; format jpeg selects a smaller lossy image composited onto white. Limited to 4096 per side and 8 megapixels.","inputSchema":object(json!({"node_id":id,"scale":{"type":"number","exclusiveMinimum":0,"maximum":4,"default":1},"format":{"type":"string","enum":["png","jpeg"],"default":"png"}}),&["node_id"]),"annotations":{"readOnlyHint":true,"openWorldHint":false}}));
    out.push(json!({"name":"desigma_list_fonts","description":"List font families and face styles available to the editor text renderer, including bundled, system, and runtime-registered fonts. Results are sorted for stable reuse within an environment; system and runtime fonts can vary between environments or sessions.","inputSchema":object(json!({}),&[]),"annotations":{"readOnlyHint":true,"openWorldHint":false}}));
    out.push(json!({"name":"desigma_get_capabilities","description":"Read the live IPC protocol and tool catalog versions plus available document tool names.","inputSchema":object(json!({}),&[]),"annotations":{"readOnlyHint":true,"openWorldHint":false}}));
    out.push(json!({"name":"desigma_get_history","description":"Read recent committed edit history with revisions and optional labels.","inputSchema":object(json!({"limit":{"type":"integer","minimum":1,"maximum":500,"default":100}}),&[]),"annotations":{"readOnlyHint":true,"openWorldHint":false}}));
    out.push(json!({"name":"desigma_diff_revisions","description":"Compare two captured document revisions, reporting added, removed, and changed nodes and document fields. Only revisions observed in this session are available.","inputSchema":object(json!({"from_revision":{"type":"integer","minimum":0},"to_revision":{"type":"integer","minimum":0}}),&["from_revision","to_revision"]),"annotations":{"readOnlyHint":true,"openWorldHint":false}}));
    out.push(json!({"name":"desigma_checkpoint","description":"Create a named in-memory session checkpoint at the current revision. Checkpoints are not embedded in saved design files.","inputSchema":object(json!({"name":{"type":"string","minLength":1,"maxLength":128},"expected_revision":{"type":"integer","minimum":0}}),&["name","expected_revision"]),"annotations":{"readOnlyHint":false,"destructiveHint":false,"idempotentHint":false,"openWorldHint":false}}));
    out.push(json!({"name":"desigma_list_checkpoints","description":"List named in-memory session checkpoints and the revisions they capture.","inputSchema":object(json!({}),&[]),"annotations":{"readOnlyHint":true,"openWorldHint":false}}));
    out.push(json!({"name":"desigma_restore_checkpoint","description":"Restore a named session checkpoint. This replaces the current document, advances its revision, reserves IDs to avoid reuse, and starts a fresh undo lineage.","inputSchema":object(json!({"name":{"type":"string","minLength":1,"maxLength":128},"expected_revision":{"type":"integer","minimum":0}}),&["name","expected_revision"]),"annotations":{"readOnlyHint":false,"destructiveHint":true,"idempotentHint":false,"openWorldHint":false}}));
    out.push(json!({"name":"desigma_render_region","description":"Render a rectangular viewport of a page. Returns PNG by default; format jpeg selects a smaller lossy image composited onto white. Limited to 4096 per side and 8 megapixels.","inputSchema":object(json!({"page_id":id,"x":number,"y":number,"width":size,"height":size,"scale":{"type":"number","exclusiveMinimum":0,"maximum":4,"default":1},"format":{"type":"string","enum":["png","jpeg"],"default":"png"}}),&["page_id","x","y","width","height"]),"annotations":{"readOnlyHint":true,"openWorldHint":false}}));
    out.push(json!({"name":"desigma_render_diff","description":"Render and compare two layers, returning a PNG difference image and changed-pixel metrics. A pixel changes when any demultiplied RGBA channel differs by more than tolerance; pixels outside either image are transparent.","inputSchema":object(json!({"node_a":id,"node_b":id,"scale":{"type":"number","exclusiveMinimum":0,"maximum":4,"default":1},"tolerance":{"type":"integer","minimum":0,"maximum":255,"default":0}}),&["node_a","node_b"]),"annotations":{"readOnlyHint":true,"openWorldHint":false}}));
    out
}

/// Validate the public schema before serde parsing, including nested native data.
/// Only the schema keywords used by this catalog are interpreted.
pub fn validate(value: &Value, schema: &Value) -> Result<(), String> {
    if let Some(branches) = schema.get("oneOf").or_else(|| schema.get("anyOf")).and_then(Value::as_array) {
        let count = branches.iter().filter(|s| validate(value, s).is_ok()).count();
        if count == 0 || (schema.get("oneOf").is_some() && count != 1) {
            return Err("value does not match a supported schema".into());
        }
    }
    if schema.get("const").is_some_and(|v| v != value) {
        return Err("invalid operation tag".into());
    }
    if schema.get("enum").and_then(Value::as_array).is_some_and(|vs| !vs.contains(value)) {
        return Err("value is not a supported enum variant".into());
    }
    if let Some(kind) = schema.get("type").and_then(Value::as_str) {
        let valid = match kind {
            "object" => value.is_object(),
            "array" => value.is_array(),
            "integer" => value.is_u64() || value.is_i64(),
            "number" => value.is_number(),
            "string" => value.is_string(),
            "boolean" => value.is_boolean(),
            "null" => value.is_null(),
            _ => false,
        };
        if !valid {
            return Err(format!("expected {kind}"));
        }
    }
    if let Some(fields) = value.as_object() {
        if let Some(required) = schema.get("required").and_then(Value::as_array) {
            for key in required.iter().filter_map(Value::as_str) {
                if !fields.contains_key(key) {
                    return Err(format!("missing required field {key}"));
                }
            }
        }
        for (key, v) in fields {
            if let Some(s) = schema.get("properties").and_then(|p| p.get(key)) {
                validate(v, s).map_err(|e| format!("{key}: {e}"))?;
            } else if schema.get("additionalProperties") == Some(&Value::Bool(false)) {
                return Err(format!("unsupported property {key}"));
            } else if let Some(s) = schema.get("additionalProperties").filter(|s| s.is_object()) {
                validate(v, s)?;
            }
        }
    }
    if let Some(array) = value.as_array() {
        for (key, valid) in [("minItems", true), ("maxItems", false)] {
            if let Some(n) = schema.get(key).and_then(Value::as_u64) {
                if (valid && array.len() < n as usize) || (!valid && array.len() > n as usize) {
                    return Err(format!("array violates {key} {n}"));
                }
            }
        }
        if let Some(item) = schema.get("items") {
            for v in array {
                validate(v, item)?;
            }
        }
    }
    if let Some(text) = value.as_str() {
        if let Some(n) = schema.get("maxLength").and_then(Value::as_u64) {
            if text.len() > n as usize {
                return Err(format!("text exceeds {n} UTF-8 bytes"));
            }
        }
    }
    if let Some(number) = value.as_f64() {
        for (key, valid) in [("minimum", true), ("maximum", false)] {
            if let Some(n) = schema.get(key).and_then(Value::as_f64) {
                if (valid && number < n) || (!valid && number > n) {
                    return Err(format!("number violates {key} {n}"));
                }
            }
        }
        if schema.get("exclusiveMinimum").and_then(Value::as_f64).is_some_and(|n| number <= n) {
            return Err("number violates exclusive minimum".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vector_operation_schemas_are_registered_and_typed() {
        let catalog = tools();
        let apply = catalog.iter().find(|tool| tool["name"] == "desigma_apply_ops").unwrap();
        let branches = apply["inputSchema"]["properties"]["ops"]["items"]["oneOf"].as_array().unwrap();
        for (name, payload) in [
            ("set_vector_network", json!({"op":"set_vector_network","id":7,"network":{"vertices":[],"segments":[],"regions":[]}})),
            ("boolean_op", json!({"op":"boolean_op","temp_id":"$new1","ids":[7,8],"boolean_op":"Union"})),
            ("flatten", json!({"op":"flatten","temp_id":"$new1","ids":[7]})),
            ("outline_stroke", json!({"op":"outline_stroke","temp_id":"$new1","id":7})),
        ] {
            let schema = branches.iter().find(|branch| branch["properties"]["op"]["const"] == name).unwrap();
            validate(&payload, schema).unwrap_or_else(|e| panic!("{name}: {e}"));
        }
        let boolean = branches.iter().find(|branch| branch["properties"]["op"]["const"] == "boolean_op").unwrap();
        assert!(validate(&json!({"op":"boolean_op","temp_id":"$new1","ids":[7,8],"boolean_op":"Xor"}), boolean).is_err());
    }

    #[test]
    fn prototype_variable_expression_schema_is_registered() {
        let catalog = tools();
        let apply = catalog.iter().find(|tool| tool["name"] == "desigma_apply_ops").unwrap();
        let branches = apply["inputSchema"]["properties"]["ops"]["items"]["oneOf"].as_array().unwrap();
        let schema = branches.iter().find(|branch| branch["properties"]["op"]["const"] == "set_variable_expression").unwrap();
        validate(&json!({"op":"set_variable_expression","variable_id":"v1","mode_id":"m1","expression":"$Count * 2"}), schema).unwrap();
        assert!(validate(&json!({"op":"set_variable_expression","variable_id":"v1","mode_id":"m1"}), schema).is_err());
    }

    #[test]
    fn combine_as_variants_schema_is_registered_and_typed() {
        let catalog = tools();
        let apply = catalog.iter().find(|tool| tool["name"] == "desigma_apply_ops").unwrap();
        let branches = apply["inputSchema"]["properties"]["ops"]["items"]["oneOf"].as_array().unwrap();
        let schema = branches.iter().find(|branch| branch["properties"]["op"]["const"] == "combine_as_variants").unwrap();
        validate(&json!({"op":"combine_as_variants","temp_id":"$new1","ids":[7,8]}), schema).unwrap();
        assert!(validate(&json!({"op":"combine_as_variants","temp_id":"$new1","ids":[7]}), schema).is_err());
    }
}
