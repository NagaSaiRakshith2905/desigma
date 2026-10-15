#!/usr/bin/env python3
"""Summarize externally recorded agent workflow attempts; does not run a model.

Input: JSONL objects with task, success, tool_calls, input_tokens, output_tokens,
and optionally model and duration_ms. Token counts must come from provider usage.
Unknown usage should be omitted, never estimated as zero.
"""
import argparse
import json
import statistics
from collections import defaultdict
from pathlib import Path


def summarize(path):
    groups = defaultdict(list)
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not line.strip():
            continue
        row = json.loads(line)
        if not isinstance(row, dict) or not isinstance(row.get("task"), str) or not row["task"] or type(row.get("success")) is not bool:
            raise ValueError(f"line {number}: task and boolean success are required")
        for field in ("tool_calls", "input_tokens", "output_tokens", "duration_ms"):
            if field in row and (type(row[field]) is not int or row[field] < 0):
                raise ValueError(f"line {number}: {field} must be a non-negative integer")
        groups[(row["task"], row.get("model", "unspecified"))].append(row)
    result = []
    for (task, model), rows in sorted(groups.items()):
        item = {"task": task, "model": model, "attempts": len(rows), "success_rate": sum(r["success"] for r in rows) / len(rows)}
        for field in ("tool_calls", "input_tokens", "output_tokens", "duration_ms"):
            values = [r[field] for r in rows if field in r]
            item[field] = {"samples": len(values), "mean": statistics.mean(values) if values else None}
        result.append(item)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("results", type=Path)
    args = parser.parse_args()
    try:
        print(json.dumps(summarize(args.results), indent=2))
    except (OSError, ValueError, TypeError) as error:
        parser.error(str(error))


if __name__ == "__main__":
    main()
