# Agent editing architecture and upstream proposal

This note records the Phase 0 seams found before implementation. The first shared operation is resizing an auto-layout frame, including child constraints, parent reflow, connector routing and one undo step. Implementation and verification status belong in `agent-validation.md`; this inventory describes the starting architecture.

## Reusable operations

| Behavior | Existing location | Extraction boundary |
| --- | --- | --- |
| Node properties and structural commands | `desigma-doc/src/history.rs` | `Command::Update` preserves parent/children; structural operations must remain explicit commands. |
| Live property tracking and transactions | `desigma-app/src/edit.rs` | `Edit` tracks first-touch snapshots; `Tx` combines structural and property edits. Its commit also records instance overrides, reroutes connectors and synchronizes instances. |
| Resize | `desigma-app/src/ops.rs::apply_resize` | Already accepts only document, edit tracker, node, size, transform and a reflow flag. Canvas and inspector share this entry point. |
| Auto layout, constraints, group fitting | `desigma-app/src/layout.rs` | `compute` is pure. Applying helpers depend on `Edit`, geometry utilities, text measurement and connector rerouting, but do not need an egui context. |
| Text size | `desigma-app/src/textmeasure.rs::measure` | Uses `desigma_render::text_geometry`. Caret, grapheme, selection and editing helpers can remain in the app. |
| Components | `desigma-doc/src/components.rs`; `desigma-app/src/comp_ops.rs` | Document algorithms already exist; `record_overrides` and `with_sync` are app-independent commit behavior. Selection and placement wrappers belong in the app. |
| Variable edits and reflow | `desigma-doc/src/variables_edit.rs`, `variables_apply.rs`; `desigma-app/src/variables/mod.rs` | `reflow_bound` handles indirect aliases and geometry; UI selection, panels and notifications stay in the app. |
| Connector updates | `desigma-app/src/board/connect.rs::reroute_touched`; `desigma-doc/src/board.rs` | The reroute helper is document/edit based; pointer hit testing and drag state remain UI responsibilities. |
| Rendering | `desigma-render/src/lib.rs` | `render_page` paints a CPU pixmap with an explicit viewport and needs no window. `render_node_into` is also exported. |

## App state that stays outside the engine

`ops.rs` methods for selection, clipboard, page switching, zoom, dialogs and saving access `App`, canvas interaction, operating-system services or view state. `comp_ops.rs` wrappers select instances, find a placement container, change selection or navigate to a main component. Connector drag helpers use hover/pointer and tool state. These should prepare explicit node IDs and operation parameters before invoking the shared engine.

The existing plugin runtime (`desigma-app/src/plugins/runtime.rs`) is a useful IPC precedent: it binds an ephemeral loopback TCP port, uses a random token, bounds request bodies, sends work over a bounded channel and requests an egui repaint. The UI polls requests and owns document mutation. Its cancellation flags and timeouts are patterns to preserve, rather than reusing its plugin asset endpoints for agent requests.

## Behaviors the original offline MCP omits

The original tools open/import/create/save a file, inspect documents/nodes/variables, generate code, create/update/delete basic layers and undo/redo. Their direct history commands omit auto layout, text measurement, connector rerouting, instance override recording, component synchronization and variable reflow. Sharing only `Command` therefore does not establish equivalence with the editor.

At inspection time `Command::Batch` applies children sequentially and returns early on failure without rolling back earlier commands. `Tx` snapshots touched properties but is not itself a complete isolation boundary for arbitrary untrusted operations. Validate and apply to a trial document before changing the caller's document or history. Never encode structural edits in a property update.

## Identity, rendering and fonts

Native documents serialize the `NodeId(u64)` map, parent/child references and allocator counter. Saving and reopening native JSON preserves IDs; importers assign internal IDs separately from source IDs. `try_alloc_id` guards against stale counters by checking the largest existing ID. Native identity is document-scoped: a live document-session identity must accompany revisions so the same numeric ID cannot accidentally address a replacement document.

The editor's `--screenshot` route drives the window shell and captures UI. It is not a headless document renderer. The reusable CPU renderer already works without a window, so document render tools should use it directly.

Both text measurement and CPU rendering use the renderer's text shaping. Its base font database contains bundled Inter and system fonts; explicitly registered extra fonts are replayed to each worker thread. `desigma-fonts` provides substitutions and optional font discovery/download, while registration makes them visible to shaping. Separate processes share code, not registered bytes or the system environment. Exact cross-machine pixels require a pinned font set; unavailable-family diagnostics and the chosen resolved family should be inspected when comparing output.

`desigma-app/src/lint.rs` checks rendered egui text/icon clipping in the application's own interface. It does not provide document text-overflow, missing-font or detached-instance lint checks to reuse. Document diagnostics need a separate, shared implementation based on document and renderer data.

## Upstream proposal

Propose `desigma-engine` as a GUI-independent workspace crate depending on `desigma-doc` and the CPU renderer. Move the existing edit/layout/measurement/connector/component commit behavior with minimal semantic changes, then make the app's old module entry points re-export it. Keep the resize equivalence fixture and transactional failure/undo checks alongside the engine. This reduces drift and provides a small initial upstream review boundary.

Follow with a typed, validated transaction API and document diagnostics as separate reviewable changes. Keep authenticated live IPC and agent controls in the application; the engine must not know about egui, MCP framing, transport authentication or endpoint discovery. Submit the extraction proposal before planning a long-lived downstream fork. This file is a review-ready proposal, not evidence of upstream acceptance or an externally submitted pull request.

## Questions that verification must resolve

- Does constraint/layout order remain identical for nested frames, text and resized component instances?
- Do text edits and variable aliases trigger exactly the same dependent geometry changes as UI edits?
- Are dry runs and failed batches isolated from document IDs, revisions, undo and redo stacks?
- Does every live UI edit, undo/redo and document replacement invalidate agent context?
- Can requests expire while queued without being applied later, and can the UI revoke pending writes?
- Are Windows endpoint-file permissions, shutdown cleanup and loopback behavior tested on Windows itself?
