# Agent editing validation

The implementation follows the Phase 0–5 plan. Architecture and the proposed upstream boundary are recorded in [agent-architecture.md](agent-architecture.md). The crate tests provide deterministic engineering checks; the agent workflow catalog describes independent model trials. Neither a test catalog nor a metrics aggregator demonstrates a measured model success rate.

## Milestone coverage and completion evidence

| Milestone | Implementation and checks | Remaining environment evidence |
| --- | --- | --- |
| M1: shared vertical slice | `desigma-engine` exports shared edit/layout/resize/measurement/component/connector behavior. `agent_workflows` compares agent resize against the UI entry point, verifies child layout, exact CPU rendered pixels, one undo and redo. | Inspect a running desktop editor on each supported platform. |
| M2: agent contract | Engine typed operations, expected revision, isolated dry run, structured errors and summaries. Tests cover stale context after undo and failed-batch/dry-run preservation of redo. | Evaluate tool usability with independent model trials. |
| M3: verification | MCP layout, diagnostics and CPU rendering tools. Render parity is checked in the engine workflow tests without a window. | Pin fonts and agree image tolerances before cross-machine screenshot comparisons. |
| M4: live editor | Authenticated loopback bridge dispatches through UI polling; agent controls pause/revoke. App tests cover authentication, document identity, live transaction/undo, active UI gesture, expiry, cancellation and disconnect. | Actual Windows ACL and socket behavior requires the Windows CI run; manual editor interaction remains separate from unit tests. |
| M5: evaluations | Rust regression workflows, machine-readable editing task catalog and JSONL metrics aggregator. Existing CI runs workspace tests on Linux, Windows and macOS. | No model benchmark results or cross-platform CI execution are claimed from a single local checkout. |

## Repeatable checks

```sh
cargo test --locked -p desigma-engine --test agent_workflows
cargo test --locked -p desigma-engine
cargo test --locked -p desigma-mcp
cargo test --locked -p desigma-app agent::tests
cargo test --workspace --locked
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
```

The workflow suite checks native save/reopen node identity, UI/agent geometry and render equivalence, failed transactions and dry runs preserving document/history, monotonic revisions across undo, component instance creation and variant changes with an override, and variable-bound fills followed by variable edits and undo. The app IPC tests use independent queues and ephemeral ports to avoid changing a user's discovery files or racing global process environment variables. Expiry is tested with an already-expired queued request rather than sleeping for the production timeout.

`.github/workflows/ci.yml` already builds and tests all workspace targets on `ubuntu-latest`, `windows-latest` and `macos-14`. The engine workspace member and app test modules therefore participate automatically. CI reports, rather than this document, establish whether those checks have passed on another OS.

## Agent task trials and measurement

Existing `crates/desigma-mcp/tests/evaluation.xml` remains a read-only retrieval benchmark. Do not mix its fresh-session setup or exact answers with editing workflows. The independent `editing-workflows.json` catalog adds resize, variant switching, fill binding, atomic multi-node edits, overflow repair, stale revisions and live editor control tasks. Each includes structural acceptance assertions; render assertions supplement structure where meaningful.

For each task, seed a fresh disposable session with explicit fixtures. In live mode use a dedicated evaluation document. Obtain IDs and revisions from tools, count every inspection/write/error/retry call, record the provider/model, commit, operating system, transport and fonts, then score success against the assertions. Repeat tasks with the same conditions to compare implementations or models.

Write one result per line, for example:

```json
{"task":"resize_auto_layout","model":"provider/model","success":true,"tool_calls":8,"input_tokens":1840,"output_tokens":410,"duration_ms":9300}
```

The numbers above illustrate the record format and are not benchmark measurements. Omit unknown token usage rather than estimating it as zero. Aggregate completed trials with:

```sh
python3 scripts/agent_eval_metrics.py results.jsonl
```

Output groups by task and model and reports attempts, success rate, and a mean plus observed sample count for tool calls, input tokens, output tokens and elapsed milliseconds. Missing metrics remain null when there are no observations. Keep raw records with experiment metadata so summaries remain auditable.

## Release and upstream review

Before calling the live-editor milestone verified, run the desktop app, attach an MCP client, inspect selection/viewport, perform the resize, see it appear, undo using the editor, and verify pause/revoke. Repeat on supported operating systems and test shutdown/replacement/stale endpoint behavior. Prepare the shared-engine extraction as the initial upstream proposal described in the architecture note. External acceptance and a submitted pull request are separate from having a local proposal ready.

## Local verification evidence (2026-10-15)

On this macOS checkout, `cargo test --workspace --locked` passed **1,454 tests**
across 74 suites, with 11 ignored tests and no failures. The added coverage includes
deep duplication, variant combination, vector operations, style and variable tools,
prototype expressions, export handoff, MCP resources and prompts, schema examples,
AA/AAA lint, scaled JPEG rendering, cursor paging, and live transport behavior.
`cargo fmt --all -- --check`, `git diff --check`,
the model-runner unit tests, and workspace Clippy with warnings denied also passed.

The macOS live smoke completed 19 tool calls against a disposable editor fixture,
rendered a 300-by-80-pixel PNG, exercised undo, and confirmed app quit cleaned up
endpoint files. The manual checklist covered keyboard undo, pause and revoke,
document replacement, quit cleanup, and stale endpoint handling on macOS.

The real-model evaluation runner was attempted twice. One trial was blocked by
client approval and one failed during MCP startup; both are recorded in
`agent-model-eval-attempts.jsonl`. Neither trial produced a scored model success
rate. Linux and Windows CI execution, Windows ACL and loopback test results,
cross-OS live smoke, and upstream acceptance remain open release evidence.
The Windows CI suite now includes endpoint ACL inspection, the `icacls` failure
path, and an authenticated loopback exchange; their outcome requires a Windows
runner. The narrow
[shared-engine extraction PR](https://github.com/deepfounder-ai/desigma/pull/2)
was submitted as a draft after 776 focused tests passed on macOS.

## Earlier local verification evidence (2026-10-14)

On this macOS checkout, `cargo test --workspace --locked` passed **1,354 tests**
across 69 suites (11 existing ignored tests). Workspace Clippy with warnings denied
and rustfmt also passed. The tests include seven engine editing workflows,
14 MCP checks including a real stdio client and image response, and four editor
bridge checks including the editor undo method.

A real stdio MCP process attached to a separate desktop editor fixture and completed
preview, resize, layout inspection, diagnostics, PNG rendering, and undo cleanup.
The frame grew from 200 to 300 pixels and its second auto-layout child moved from
x=180 to x=280. The repeatable smoke took 19 document tool calls including cleanup;
this is an engineering smoke check, not an independent model success-rate benchmark.
No provider token usage was available. The first desktop attempt exposed requests
stalling behind an occluded window; moving dispatch from UI painting into eframe's
`App::logic` fixed the repeated hidden-window roundtrip.

Run the same check against an explicitly chosen empty, disposable editor document:

In the editor, open **Agent access**, enable local access, approve the connection
request, and enable document edits before the smoke writes. The bridge publishes
no endpoint until the document owner opts in. If the first attach is rejected
while approval is pending, approve the prompt and rerun the command.

```sh
cargo build --locked -p desigma-app -p desigma-mcp
python3 scripts/agent_live_smoke.py \
  --endpoint /path/to/agent-endpoints/pid-session.json \
  --expected-document-name 'Untitled' \
  --output /tmp/desigma-live-evaluation
python3 scripts/agent_eval_metrics.py /tmp/desigma-live-evaluation/metrics.jsonl
```

The endpoint argument is the `endpoint_path` returned by discovery, not an MCP
response JSON file. The smoke writes `render.png`, `state.json` (no authentication
token), and `metrics.jsonl` to the output directory. It undoes its five committed
changes on success unless `--keep-fixture` is provided for visual inspection.

Actual Windows/Linux execution, independent model trials with provider token
usage, and upstream submission/acceptance remain external release evidence. Native
keyboard-driven undo and the pause/revoke controls should still be checked manually
on each supported desktop; their document behavior is covered by regression tests.
