# Desigma MCP

A local stdio MCP server for inspecting, editing and rendering Desigma documents. It supports an isolated file session and authenticated attachment to a running editor. Both use the shared editing engine for dependent layout, text, component and connector updates.

## Build and connect

From the repository root:

```sh
cargo build --locked -p desigma-mcp
```

The executable is `target/debug/desigma-mcp` (`desigma-mcp.exe` on Windows). Use `--release` for a release build and change `debug` to `release` in the paths below. Rust and the repository's normal build prerequisites are required only for building.

Configure an MCP client that supports local stdio servers. These are generic `mcpServers` examples; use the configuration format and location required by your client. Replace both paths with absolute paths on your machine. The workspace directory must already exist.

Windows:

```json
{
  "mcpServers": {
    "desigma": {
      "command": "C:\\path\\to\\desigma\\target\\debug\\desigma-mcp.exe",
      "args": ["--workspace", "C:\\path\\to\\designs"]
    }
  }
}
```

macOS/Linux:

```json
{
  "mcpServers": {
    "desigma": {
      "command": "/path/to/desigma/target/debug/desigma-mcp",
      "args": ["--workspace", "/path/to/designs"]
    }
  }
}
```

The client launches the process and speaks MCP over stdin/stdout. The server does not listen on a network port. No account, API key, Figma connection, or running editor is needed. Configuration is not installed automatically.

## File workflow

1. Call `desigma_open_file` with a path inside the configured workspace, or `desigma_new_document` with a name.
2. Use `desigma_get_document` to discover page IDs, then `desigma_list_nodes` and `desigma_get_node` to inspect layers. IDs are positive integers; obtain them from tool results rather than Figma node IDs.
3. Inspect variables or generate handoff code. Make edits with the creation, update, and deletion tools; undo and redo operate on this session's edit history.
4. Call `desigma_save_file` with an **unused** `.desigma` filename. Its parent directory must already exist. Open the resulting file in the desktop editor to continue there.

Example tool arguments:

```text
desigma_open_file     {"path":"input.fig"}
desigma_get_document  {}
desigma_list_nodes    {"query":"button","limit":25}
desigma_get_node      {"node_id":42}
desigma_generate_code {"node_id":42,"language":"react"}
desigma_set_properties {"expected_revision":7,"id":42,"properties":{"name":"Primary action","width":160}}
desigma_save_file     {"path":"output-v2.desigma"}
```

`42` is illustrative: substitute an ID returned by the preceding queries. Relative paths resolve beneath `--workspace`. Absolute paths are accepted only within that same directory tree. Reads canonicalize paths and reject escapes; writes resolve the existing parent and create a new file without overwriting an existing file or symlink.

Edits remain in memory until saved. Restarting the server loses unsaved work. Opening another file or starting a new document resets undo history and refuses to discard pending edits unless `discard_changes: true` is explicitly supplied. Save each revision under a new filename; overwriting is not supported. Tool failures return an MCP error result with `error: {code,message}`. Codes include `stale_revision`, `missing_node`, `invalid_operation`, `permission_denied`, `operation_conflict` and `unsupported_operation`.

Imports are limited to 64 MiB per file, 100,000 layers, and 256 levels of nesting. The server validates the imported document graph before using it, rejecting malformed parent/child relationships and cycles.

## Tools

| Tool | Purpose |
| --- | --- |
| `desigma_open_file` | Import a native `.desigma`, `.fig`, SVG, or supported JSON design into the session. |
| `desigma_new_document` | Start an empty document with one page. |
| `desigma_save_file` | Write the session to a new native `.desigma` file. |
| `desigma_discover_editors` | Find local editor endpoints available for live editing. |
| `desigma_attach_editor` / `desigma_detach_editor` | Attach to or leave a running local editor session. |
| `desigma_get_live_context` | Read the attached editor's selection, page, viewport, and revision. |
| `desigma_get_document` | Read document name, pages, counts, and undo/redo availability. |
| `desigma_get_capabilities` | Read protocol/catalog versions and available document tool names. |
| `desigma_describe_schema` | Read a tool's validated JSON input schema and description. |
| `desigma_list_nodes` | Search names or filter by kind, style, variable binding, component source, and parent; paginate using a revision-stable `next_cursor`. |
| `desigma_get_node` | Read a complete native node or request selected top-level `fields`. |
| `desigma_list_styles` | Read shared styles and their metadata. |
| `desigma_get_variables` | Read collections and tokens; optionally resolve one node's direct bindings in its inherited modes. |
| `desigma_generate_code` | Return CSS, Tailwind, React, SwiftUI, Compose, or SVG source without writing a file. |
| `desigma_create_node` | Create a frame, group, section, rectangle, ellipse, line, polygon, star, vector, text, boolean, sticky, board shape, or stamp layer. Board kinds require a board document. |
| `desigma_update_node` | Change name, position, dimensions, visibility, opacity, or plain text. |
| `desigma_delete_node` | Remove a non-page layer and its descendants. |
| `desigma_apply_ops` | Apply create, delete, move, reorder, property, paint, auto-layout, component, and variable edits atomically. |
| `desigma_set_properties` / `desigma_set_fills` / `desigma_set_auto_layout` | Update supported node properties, native paints, and auto-layout. |
| `desigma_create_instance` / `desigma_set_variant` | Create or switch component instances. |
| `desigma_edit_variable` | Change a variable value for one collection mode. |
| `desigma_create_variable_collection` / `desigma_create_variable_mode` / `desigma_create_variable` | Create variable foundations; batch-created IDs can be referenced by later operations. |
| `desigma_rename_variable` / `desigma_delete_variable` / `desigma_bind_variable` / `desigma_unbind_variable` / `desigma_set_node_mode` | Manage variables, compatible property bindings, and inherited mode overrides. |
| `desigma_set_variable_expression` | Evaluate a validated prototype variable expression and store its typed result in a mode. |
| `desigma_set_text_patch` / `desigma_set_visual_patch` | Edit native text and visual properties. |
| `desigma_move_node` / `desigma_reorder_children` | Change hierarchy placement and direct-child order. |
| `desigma_duplicate_node` | Deep-copy a subtree with fresh IDs, optional offset/placement, and an old-to-new ID map. |
| `desigma_create_page` / `desigma_delete_page` / `desigma_reorder_pages` | Manage document pages; deletion retains at least one page. |
| `desigma_group_nodes` / `desigma_ungroup` | Group or ungroup sibling layers in one undoable transaction. |
| `desigma_wrap_in_frame` / `desigma_wrap_in_auto_layout` | Wrap sibling layers in a transparent frame or auto-layout frame. |
| `desigma_list_fonts` | List font faces available to the renderer. |
| `desigma_get_changes` | Poll revision changes from an attached live editor. |
| `desigma_get_history` | Read recent committed edit revisions and labels. |
| `desigma_diff_revisions` / `desigma_checkpoint` / `desigma_list_checkpoints` / `desigma_restore_checkpoint` | Inspect recent structural revision changes and manage named session checkpoints. Restore starts a new undo lineage. |
| `desigma_replace_variables` / `desigma_set_styles` | Replace validated variable collections or shared styles atomically. |
| `desigma_create_style` / `desigma_update_style` / `desigma_delete_style` / `desigma_apply_style` / `desigma_list_styles` | Manage and inspect fill, text, effect, and grid styles through undoable operations. |
| `desigma_set_vector_network` / `desigma_boolean_op` / `desigma_flatten` / `desigma_outline_stroke` | Edit vector geometry, compose live booleans, flatten geometry, or convert strokes to editable shapes. |
| `desigma_create_component` / `desigma_detach_instance` / `desigma_reset_instance` | Manage component and instance lifecycle. |
| `desigma_combine_as_variants` | Combine sibling main components into a component set in one undo step. |
| `desigma_set_component_properties` / `desigma_set_instance_property` / `desigma_reset_overrides` | Edit component definitions and instance overrides. |
| `desigma_set_reactions` / `desigma_set_flows` / `desigma_set_prototype_device` | Edit prototype interactions and device preview. |
| `desigma_set_comments` / `desigma_create_connector` / `desigma_set_connector` | Edit board comments and connectors. |
| `desigma_list_comments` / `desigma_add_comment` / `desigma_reply_comment` / `desigma_resolve_comment` | Inspect, create, reply to, resolve, and reopen comment threads. |
| `desigma_list_components` / `desigma_go_to_main_component` / `desigma_list_flows` | Inspect components, an instance's main component, and prototype flows. |
| `desigma_add_slide` / `desigma_delete_slide` / `desigma_reorder_slide` / `desigma_set_slide_notes` | Manage deck slides and presenter notes. |
| `desigma_set_export_presets` | Set bounded export presets on a layer or slide. |
| `desigma_set_dev_status` / `desigma_set_dev_annotations` | Mark handoff state and edit bounded developer annotations. |
| `desigma_export_node` / `desigma_export_deck_pdf` | Write PNG, SVG, or PDF handoff files in the offline workspace; exports create new files and do not change the document revision. |
| `desigma_import_image` / `desigma_import_svg` / `desigma_list_images` | Import workspace-contained raster/vector assets or list embedded images. |
| `desigma_list_publishable_assets` / `desigma_create_library_snapshot` | Inspect and snapshot this document's publishable library assets. |
| `desigma_list_local_components` / `desigma_list_library_components` / `desigma_list_pending_updates` | Inspect local and imported component definitions and compare library versions. |
| `desigma_list_libraries` / `desigma_list_library_items` | Inspect enabled libraries and their locally imported assets. |
| `desigma_import_library_asset` / `desigma_apply_library_updates` / `desigma_enable_library` / `desigma_disable_library` | Import library assets, apply updates, and manage enabled libraries. |
| `desigma_get_outline` / `desigma_get_layout` / `desigma_get_diagnostics` | Inspect hierarchy, computed layout, or design issues. |
| `desigma_render_node` | Render a page or layer to PNG image content. |
| `desigma_render_region` | Render a rectangular page viewport to PNG image content. |
| `desigma_render_diff` | Compare two layers and return a difference image and changed-pixel metrics. |
| `desigma_undo` / `desigma_redo` | Traverse the session's edit history. |

`desigma_list_nodes` defaults to 100 results and accepts 1–500 per call. `parent_id` filters direct children, not all descendants. Coordinates are relative to the parent. Width and height must be non-negative, geometry must be finite and within ±1,000,000, and opacity must be between 0 and 1. Replacing text clears rich-text runs and list ranges. Page roots cannot be edited or deleted by the layer tools.

MCP resources expose `desigma://document`, `desigma://pages`, `desigma://variables`, and `desigma://styles` as JSON. The prompt catalog includes `build_screen`, `apply_design_tokens`, `fix_diagnostics`, and `create_component`. Resources are read on demand; live clients can poll `desigma_get_changes` to detect editor revisions.

## Atomic editing and verification

Every document write, including legacy create/update/delete and undo/redo, requires `expected_revision`. Read it from `desigma_get_document` or compact inspection results. Successful commits, undo and redo advance the revision. Stale writes are rejected before mutation. IDs persist in native saved files; references must still be refreshed after reopening another document.

Asset path tools accept workspace-relative paths only. The server canonicalizes the requested path and rejects paths that resolve outside the configured workspace; image imports are bounded to 50 MiB and 50 megapixels. Library snapshots are passed as native snapshot data, and imported assets, updates, and library enable/disable changes commit through the same revision-checked, undoable transaction path as other edits.

Document inspection and editing results include the document `revision`, including code generation and render results, so a client can refresh its write token after any intervening change.

Typed tools accept `expected_revision` and optional `dry_run` (default false):

- `desigma_set_properties`: `id` and constrained `properties` (name, visibility, locked, opacity, x/y, width/height, corner_radii, characters).
- `desigma_set_fills`: `id`, native `fills`, optional `bindings` mapping `fills/<index>` to a color variable ID.
- `desigma_set_auto_layout`: `id`, native `auto_layout` configuration, or null to remove it.
- `desigma_create_instance`: `component`, `parent`, and `at: {x,y}`.
- `desigma_set_variant`: instance `id` and target variant `component`.
- `desigma_edit_variable`: variable `id`, `mode`, and native `value`.
- `desigma_create_node`: `temp_id` (such as `$new1`), `parent`, optional child `index`, `kind`, and optional `properties`.
- `desigma_delete_node`: `id` (numeric or a temporary ID from an earlier create in the same batch).
- `desigma_move_node`: `id`, destination `parent`, and optional child `index`; it preserves world position except where destination auto-layout recomputes placement.
- `desigma_reorder_children`: `parent` and the complete back-to-front `order` of its direct children.
- `desigma_set_text_patch`: `id` and a native text patch for style, runs, paragraph, list, direction, and resizing settings.
- `desigma_set_visual_patch`: `id` and native visual properties including stroke, effects, constraints, rotation, and frame clipping.
- `desigma_apply_ops`: 1–256 operations, each tagged with `op` (without `desigma_`). The whole batch and dependent updates commit as one undo step; failures and previews leave document/history untouched. Operations can refer to nodes created earlier using `$newN` IDs, returned as `temp_ids` in the result.

```json
{"expected_revision":7,"dry_run":true,"ops":[
  {"op":"set_properties","id":42,"properties":{"width":320}},
  {"op":"set_properties","id":43,"properties":{"name":"Updated label"}}
]}
```

The result includes `revision`, `dry_run`, changed node IDs/properties, derived-change markers and created IDs. A preview reports the original revision so the same request can be committed after inspection. Requests are limited to 1 MiB.

`desigma_get_outline`, `desigma_get_layout` and `desigma_get_diagnostics` accept optional `node_id` and `limit` (default 500, maximum 1000). Diagnostics check renderer-measured text overflow, unavailable fonts, detached component references, unresolved variables, substantial visible-sibling overlaps, and children extending beyond parent bounds. They report low text contrast only when text and its nearest ancestor background use opaque solid colors; overlap and contrast are review hints and can be intentional or affected by imagery, gradients, effects, and occlusion. Safe fixes, such as measured text-box growth or a black/white contrast correction, include a revision-checked `desigma_apply_ops` dry-run request. Diagnostics inspect document content; the existing UI chrome lint checks painted editor widgets and is not suitable for document diagnostics.

`desigma_render_node` accepts `node_id`, `scale` (default 1, maximum 4), and optional `format` (`png` by default or `jpeg`). `desigma_render_region` accepts the same `format` option. JPEG uses quality 82 and composites transparency onto white; `scale` controls output dimensions for either format. Both return MIME-typed image content and remain limited to 4096 pixels per side and 8 megapixels. Rendering resolves variables and uses the same CPU renderer and font database as editor exports, including bundled and system fonts. Runtime fonts registered only in a separate editor process are available when rendering through live attachment, rather than in the isolated session.

`desigma_list_fonts` reports bundled, system, and runtime-registered faces in sorted order. The ordering is stable for a given environment, while system and runtime font availability can differ across machines and sessions. `desigma_render_diff` uses a per-channel tolerance: a pixel is changed when any demultiplied red, green, blue, or alpha channel differs by more than the requested tolerance. Pixels outside either rendered layer are treated as transparent. For repeatable pixel comparisons, use the same renderer build and font set, and set tolerance explicitly when small rasterization differences are acceptable.

## Live editor connection

1. Open a document in the editor. Open **Agent access…** from the main menu and enable local access for this document. No endpoint is published before this opt-in. Replacing a document revokes its previous session.
2. Call `desigma_discover_editors`, then `desigma_attach_editor` with one returned `endpoint_path`.
3. The first client request opens an approval prompt in the editor. Approve it for read-only inspection, then retry the request. To allow writes, also select **Allow document edits**. The editor status indicator offers one-click pause and resume. `desigma_get_live_context` reads selection, page and viewport; each committed batch appears in editor history and can be undone there.
4. Call `desigma_detach_editor` to return to the preserved offline session. File open/save/new tools are rejected while attached.

The editor publishes a private endpoint file containing a loopback address, random authentication token and document identity only after opt-in. Tokens are sent on each request and omitted from tool results. The client rejects non-loopback addresses, non-private Unix endpoint files, oversized requests/responses and unsupported protocol versions. Connection timeout is 3 seconds and response timeout is 15 seconds. Stale discovery files can be listed; attach verifies the editor responds. Reopening/closing a document invalidates its old endpoint. Agent requests run in the UI thread’s logic callback even when the editor window is covered; incompatible UI edits, paused access and revoked access reject writes. Keep endpoint files private, including their Windows ACLs.

## Verification and evaluation

```sh
cargo test --locked -p desigma-mcp
```

`tests/evaluation.xml` contains ten independent, read-only retrieval questions based on the repository's fixed Figma REST fixture. For each question, start a fresh server with the repository root as `--workspace`, then preload `crates/desigma-fig/tests/fixtures/rest_file.json` using `desigma_open_file`. This preload is test setup; answering the questions requires only the read-only query/code tools. Do not create, update, delete, undo, redo, or save during evaluation. Match the response against the XML answer exactly.
