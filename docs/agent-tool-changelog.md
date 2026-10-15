# Agent tool catalog changelog

The live editor IPC protocol and MCP tool catalog have separate versions. Query
`desigma_get_capabilities` to read both versions from a running server. The
editor rejects an unsupported live protocol during attachment.

## Catalog 4 — 2026-10-15 (development)

- Added slide transition metadata and optional resolved-token/responsive-hint
  output to the code-generation tool.
- Added scaled JPEG output for node/region renders, AA/AAA and consistency
  diagnostics, and revision-bound comment/library cursors.
- Added MCP 2026-07-28 resource subscriptions and revision-change notifications
  for document, pages, variables and styles. Live IPC protocol remains version 1.

## Catalog 3 — 2026-10-15 (development)

- Added typed variable, shared-style, comment, vector, board, dev-handoff and
  prototype-expression actions, with temporary IDs where operations create nodes.
- Added filtered/projected node inspection, library/component/flow/comment reads,
  structural revision diffs and named session checkpoints.
- Added on-demand MCP document/page/variable/style resources, workflow prompts,
  input-schema discovery and workspace PNG/SVG/PDF export.
- Expanded document diagnostics and attached rendered-image metadata to image
  tool responses. The live IPC protocol remains version 1.

## Catalog 2 — 2026-10-15 (development)

- Typed, atomic create/delete, hierarchy, page, text, visual, component,
  variable, prototype, board, deck, asset and library operations were added.
- Legacy create/delete calls use the typed transaction path. Writes require an
  expected revision; dry runs report a preview without committing.
- Read tools now cover font inventory, revision history, layout and diagnostics,
  image listing, region rendering and node comparison.
- Live sessions require document-owner opt-in and read-only approval. Navigation
  and revision-change polling are available after attachment.

## Catalog 1 — 2026-10-14

The first shared editing engine and basic MCP editing/verification tools were
introduced. The live IPC protocol was version 1.

Catalog 2 retains live IPC protocol 1; clients should discover capabilities
rather than infer them from the application version.
