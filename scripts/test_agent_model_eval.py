import json
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import agent_model_eval


class ModelEvalTests(unittest.TestCase):
    def test_codex_event_parser_counts_mcp_calls_and_real_usage(self):
        trace = "\n".join([
            json.dumps({"type": "item.started", "item": {"type": "mcp_tool_call", "name": "desigma_get_document"}}),
            json.dumps({"type": "item.completed", "item": {"type": "mcp_tool_call", "name": "desigma_get_document"}}),
            json.dumps({"type": "turn.completed", "turn_context": {"model": "test-model"}, "usage": {"input_tokens": 42, "output_tokens": 7}}),
        ])
        telemetry = agent_model_eval.event_telemetry(trace)
        self.assertEqual(telemetry["tool_calls"], 1)
        self.assertEqual(telemetry["model"], "test-model")
        self.assertEqual(telemetry["input_tokens"], 42)
        self.assertEqual(telemetry["output_tokens"], 7)

    def test_unknown_token_usage_is_omitted(self):
        trace = json.dumps({"type": "turn.completed", "model": "test-model"})
        telemetry = agent_model_eval.event_telemetry(trace)
        self.assertEqual(telemetry, {"tool_calls": 0, "model": "test-model"})

    def test_catalog_has_executable_operation_group_profiles(self):
        catalog = json.loads(Path("crates/desigma-mcp/tests/editing-workflows.json").read_text())
        executable = {task["id"]: task["evaluation"] for task in catalog["tasks"] if "evaluation" in task}
        self.assertEqual(
            set(executable),
            {"model_text_styling", "model_visual_properties", "model_component_lifecycle", "model_prototype_reaction", "resize_auto_layout"},
        )
        for evaluation in executable.values():
            self.assertTrue(evaluation["checks"])
            self.assertIn(evaluation["fixture"], {"text", "visual", "component", "prototype", "resize"})


if __name__ == "__main__":
    unittest.main()
