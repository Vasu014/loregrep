#!/usr/bin/env python3
"""Layer 2 pilot A/B runner for loregrep.

Runs pi in two arms:
  A baseline: no loregrep skill, prompt forbids loregrep.
  B loregrep: loads skills/loregrep/SKILL.md and requires loregrep exec-tool use.

Results are JSONL in evals/agent/results/.
"""

from __future__ import annotations

import argparse
import json
import os
import random
import re
import shutil
import subprocess
import sys
import tempfile
import time
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
FIXTURE = ROOT / "evals" / "fixtures" / "rust-basic"
RESULTS = ROOT / "evals" / "agent" / "results"
TRANSCRIPTS = RESULTS / "transcripts"
SKILL = ROOT / "skills" / "loregrep" / "SKILL.md"
BINARY_DIR = ROOT / "target" / "release"

TASKS = [
    {
        "id": "rust-basic-callers-parse-config",
        "prompt": """You are in a small Rust repository. Find every REAL call site of the function parse_config. Ignore imports, definitions, comments, doc examples, and string literals. Answer ONLY as JSON: {\"sites\":[{\"file\":\"...\",\"line\":N}, ...]}.""",
        "expect_sites": [
            {"file": "src/main.rs", "line": 18},
            {"file": "src/loader.rs", "line": 17},
        ],
        "kind": "sites",
    },
    {
        "id": "rust-basic-deps-main",
        "prompt": """You are in a small Rust repository. List the dependency/import module paths of src/main.rs. Answer ONLY as JSON: {\"dependencies\":[\"...\", ...]}.""",
        "expect_dependencies": [
            "std::collections::HashMap",
            "std::process",
            "config::parse_config",
            "loader::Loader",
        ],
        "kind": "dependencies",
    },
]


def now_id() -> str:
    return datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%S") + "-" + hex(random.randrange(16**6))[2:].zfill(6)


def extract_final_text(events: list[dict]) -> str:
    final = ""
    for ev in events:
        msg = ev.get("message") if ev.get("type") in {"message_end", "turn_end"} else None
        if not msg or msg.get("role") != "assistant":
            continue
        parts = msg.get("content") or []
        text = "".join(p.get("text", "") for p in parts if p.get("type") == "text")
        if text:
            final = text
    return final.strip()


def extract_usage(events: list[dict]) -> dict:
    usage = {}
    for ev in events:
        msg = ev.get("message") if ev.get("type") in {"message_end", "turn_end"} else None
        if msg and msg.get("role") == "assistant" and msg.get("usage"):
            usage = msg["usage"]
    return usage


def parse_json_answer(text: str):
    try:
        return json.loads(text)
    except Exception:
        m = re.search(r"\{.*\}", text, re.S)
        if not m:
            return None
        try:
            return json.loads(m.group(0))
        except Exception:
            return None


def oracle(task: dict, answer_text: str) -> tuple[bool, str]:
    ans = parse_json_answer(answer_text)
    if ans is None:
        return False, "final answer is not parseable JSON"
    if task["kind"] == "sites":
        got = sorted((str(x.get("file")), int(x.get("line"))) for x in ans.get("sites", []) if isinstance(x, dict) and "line" in x)
        exp = sorted((x["file"], x["line"]) for x in task["expect_sites"])
        return got == exp, f"got={got} expected={exp}"
    if task["kind"] == "dependencies":
        got = sorted(str(x) for x in ans.get("dependencies", []))
        exp = sorted(task["expect_dependencies"])
        return got == exp, f"got={got} expected={exp}"
    return False, "unknown task kind"


def run_one(experiment_id: str, model: str, task: dict, arm: str, rep: int, timeout_s: int) -> dict:
    run_id = f"{experiment_id}-{model.replace('/', '_')}-{task['id']}-{arm}-r{rep}"
    workspace = Path(tempfile.mkdtemp(prefix=f"loregrep-agent-{arm}-")) / "repo"
    shutil.copytree(FIXTURE, workspace, ignore=shutil.ignore_patterns(".loregrep"))

    env = os.environ.copy()
    if arm == "loregrep":
        env["PATH"] = f"{BINARY_DIR}:{env.get('PATH', '')}"
        subprocess.run(
            [str(BINARY_DIR / "loregrep"), "exec-tool", "get_repository_tree", "--params", '{"include_file_details":false}', "--path", str(workspace)],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            env=env,
            timeout=30,
        )
        system = "Use loregrep exec-tool for structural code questions before grep/read. The loregrep binary is on PATH."
        extra = ["--skill", str(SKILL), "--append-system-prompt", system]
        prompt = task["prompt"] + "\nYou MUST use loregrep exec-tool at least once before answering."
    else:
        extra = ["--no-skills", "--append-system-prompt", "Do not use loregrep or any command containing loregrep. Use normal shell/read/grep only."]
        prompt = task["prompt"] + "\nDo NOT use loregrep."

    cmd = [
        "pi", "-p", "--mode", "json", "--model", model, "--no-session", "--no-context-files", "--no-extensions",
        "--tools", "read,bash,grep,find",
        *extra,
        prompt,
    ]
    started = time.time()
    proc = subprocess.run(cmd, cwd=workspace, env=env, capture_output=True, text=True, timeout=timeout_s)
    wall = time.time() - started

    events = []
    for line in proc.stdout.splitlines():
        try:
            events.append(json.loads(line))
        except Exception:
            pass
    final = extract_final_text(events)
    usage = extract_usage(events)
    transcript_path = TRANSCRIPTS / f"{run_id}.jsonl"
    transcript_path.write_text(proc.stdout + ("\nSTDERR:\n" + proc.stderr if proc.stderr else ""))

    bash_commands = {}
    tool_counts = {}
    for ev in events:
        msg = ev.get("message")
        if not isinstance(msg, dict):
            continue
        for item in msg.get("content") or []:
            if item.get("type") != "toolCall" or "partialJson" in item:
                continue
            name = item.get("name", "")
            tool_counts[name] = tool_counts.get(name, 0) + 1
            if name == "bash":
                bash_commands[item.get("id", str(len(bash_commands)))] = (item.get("arguments") or {}).get("command", "")
    loregrep_calls = sum(1 for command in bash_commands.values() if re.search(r"\bloregrep\s+exec-tool\b", command))
    valid_arm = (loregrep_calls == 0) if arm == "baseline" else (loregrep_calls > 0)
    passed, detail = oracle(task, final)

    shutil.rmtree(workspace.parent, ignore_errors=True)
    return {
        "schema": "loregrep-eval-agent/1",
        "experiment_id": experiment_id,
        "run_id": run_id,
        "task_id": task["id"],
        "arm": arm,
        "rep": rep,
        "model": model,
        "driver": "pi",
        "wall_clock_s": round(wall, 3),
        "tokens": usage,
        "tool_calls": {"loregrep_exec_tool": loregrep_calls, **tool_counts},
        "oracle_pass": passed,
        "oracle_detail": detail,
        "valid_arm": valid_arm,
        "agent_exit_code": proc.returncode,
        "timed_out": False,
        "transcript_path": str(transcript_path.relative_to(ROOT)),
        "final_answer": final[:1000],
        "stderr_tail": proc.stderr[-1000:],
    }


def model_smoke(model: str) -> tuple[bool, str]:
    proc = subprocess.run(
        ["pi", "-p", "--mode", "json", "--model", model, "--no-tools", "--no-session", "--no-context-files", "--no-skills", "--no-extensions", "Reply OK"],
        capture_output=True,
        text=True,
        timeout=60,
    )
    return proc.returncode == 0, (proc.stderr + proc.stdout)[-1000:]


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--models", nargs="+", default=["zai/glm-5.2", "openai-codex/gpt-5.6-luna"])
    ap.add_argument("--reps", type=int, default=3)
    ap.add_argument("--timeout", type=int, default=240)
    ap.add_argument("--allow-missing-models", action="store_true")
    args = ap.parse_args()

    RESULTS.mkdir(parents=True, exist_ok=True)
    TRANSCRIPTS.mkdir(parents=True, exist_ok=True)
    experiment_id = now_id()
    out = RESULTS / f"{experiment_id}.jsonl"

    available = []
    with out.open("w") as f:
        for model in args.models:
            ok, msg = model_smoke(model)
            if ok:
                available.append(model)
            else:
                row = {"schema": "loregrep-eval-agent-skip/1", "experiment_id": experiment_id, "model": model, "reason": "model smoke test failed", "detail_tail": msg}
                print(json.dumps(row), file=f, flush=True)
                print(f"SKIP {model}: smoke failed", file=sys.stderr)
                if not args.allow_missing_models:
                    return 2

        plan = [(m, t, a, r) for m in available for t in TASKS for r in range(1, args.reps + 1) for a in ["baseline", "loregrep"]]
        random.shuffle(plan)
        for i, (model, task, arm, rep) in enumerate(plan, 1):
            print(f"[{i}/{len(plan)}] {model} {task['id']} {arm} r{rep}", file=sys.stderr)
            try:
                row = run_one(experiment_id, model, task, arm, rep, args.timeout)
            except subprocess.TimeoutExpired as e:
                row = {"schema": "loregrep-eval-agent/1", "experiment_id": experiment_id, "model": model, "task_id": task["id"], "arm": arm, "rep": rep, "timed_out": True, "oracle_pass": False, "valid_arm": False, "error": str(e)}
            print(json.dumps(row), file=f, flush=True)
    print(out)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
