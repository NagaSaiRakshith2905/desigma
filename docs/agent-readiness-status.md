# Agent-readiness implementation status

Last updated: 2026-10-15. This tracks the task list against code in this branch; passing local tests is distinct from cross-platform or independent-model validation.

| Task group | Current implementation | Remaining work |
| --- | --- | --- |
| P0 live consent | Document-session opt-in, first-connection approval, read-only default, separate edit permission, visible pause/revoke, audit panel and README. | Windows-specific ACL/loopback verification. |
| P0 transport | Constant-time token comparison, rotation, bounded workers and queue, rate limit, panic-safe document restoration, stale endpoint cleanup. | Windows manual failure-path exercise. |
| P0 write contract | Revision-checked atomic `apply_ops`, typed create/delete, batch temporary IDs, legacy wrappers, summaries and structured errors. | Continue replacing broad native payloads with narrow validated actions. |
| P1 structure | Move, reorder, deep duplicate with ID mapping, group/ungroup, wrap, page operations and all listed basic layer kinds; board node kinds use board-only validation. | Review cross-parent transform cases beyond current tests. |
| P1 text and visuals | Text and visual patches cover native rich text, sizing, paragraph/list, stroke, effect, blend, constraints, clipping, rotation and layout-child settings; font inventory and family validation are available. | Add dedicated small tools for individual property groups and broaden font matching/suggestions. |
| P1 components, variables, styles | Component/instance lifecycle, variant combination and override operations, variable collection/mode/value/binding CRUD, style CRUD/apply and library component inspection. | Broaden property-specific style/variable use cases. |
| P1 assets | Workspace-contained raster and SVG import, image reference listing and validation. | Test additional SVG/image fixtures and live-editor workspace import policy. |
| P2 vectors, boards, prototypes | Validated vector network, boolean, flatten and stroke-outline helpers; board stickies/shapes/stamps/connectors; reactions, flows, prototype device and evaluated variable expressions. | Broader pen/anchor editing, persisted expression AST and additional prototype variable APIs. |
| P2 slides, export, handoff, comments | Slide add/delete/reorder/notes and export presets; new workspace PNG/SVG/PDF and deck PDF; dev status/annotations; comment add/reply/resolve/list. | No slide-level transition field exists in the document model; define one or map to prototype reactions. Broaden code-generation options. |
| P2 libraries and live context | Library snapshot/import/update/enable tools and read inspection; live selection/page/zoom, hovered and viewport nodes, revision polling, identified clients and audit history. | Push notifications/subscriptions and frame batching; richer library style workflows. |
| P2 diagnostics and rendering | Layout, text overflow, fonts, detached instances, bindings, bounds, overlap, naming and conservative contrast; node/region render, node diff, revision structural diff, checkpoints. | Cross-revision image diff, pinned fonts, compressed previews and wider lint coverage. |
| P3 ecosystem | Tool-schema descriptions/examples, projected node reads, filtered/cursored list, MCP resources/prompts, agent panel, packaged MCP binary, copied client configs, protocol/catalog versions and changelog. | Resource subscriptions, fully cursor-based pagination on every list, richer per-edit review/revert, optional built-in chat. |
| Evaluation and release | Rust workflow tests, real stdio tests, model-eval runner/catalog/metrics, macOS live smoke and manual checks, CI matrix definitions. | Independent model task success is unmeasured: two Codex attempts were blocked by approval and MCP startup. CI execution on Linux/Windows/macOS, Windows manual smoke, nightly regression gate and upstream PR remain to be completed. |

See [agent-validation.md](agent-validation.md) for dated check results and [agent-model-eval.md](agent-model-eval.md) for the model-runner limits.
