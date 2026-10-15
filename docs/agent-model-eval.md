# Real-model editing workflow evaluation

`scripts/agent_model_eval.py` runs one catalog prompt with Codex CLI against a fresh temporary Desigma workspace. It seeds a fixture through a private stdio MCP server, asks Codex to make the requested edit through that server, reopens a saved result in a fresh server process, and evaluates native document fields. Each completed CLI invocation appends one JSON object to `target/agent-evals/results.jsonl`; event traces and stderr are kept beside it. Missing model or usage fields stay omitted when Codex JSONL does not report them.

Run an executable task from the repository root:

```sh
cargo build --locked -p desigma-mcp
python3 scripts/agent_model_eval.py \
  --task resize_auto_layout \
  --mcp target/debug/desigma-mcp \
  --repo . \
  --output-dir target/agent-evals \
  --results target/agent-evals/results.jsonl
```

Pass `--model <name>` to request a model explicitly. Fixtures and model-produced files live under the output directory. The runner starts Codex with `--approve-for-me` and sets its working directory to the attempt directory, so Codex's workspace-write sandbox is scoped to disposable files. Desigma MCP still applies its own tool approval and workspace checks.

The catalog contains executable profiles for `resize_auto_layout`, text styling, visual properties, component lifecycle, and prototype reactions. Structural checks use node data reopened from the saved native document. `scripts/test_agent_model_eval.py` tests event telemetry parsing and catalog profiles.

The first two real-model attempts are recorded in [agent-model-eval-attempts.jsonl](agent-model-eval-attempts.jsonl). Both failed before editing: the initial invocation was blocked because the Desigma `open_file` call required approval under the local `never` policy; with `--approve-for-me`, Codex failed to initialize the MCP connection. The reported token counts are the CLI's `turn.completed` usage events, including the substantial input usage on these failed attempts. Neither result is a successful task completion. Model identity was absent from the CLI events and is therefore omitted.
