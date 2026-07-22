#!/usr/bin/env python3
"""Loregrep Layer 1 retrieval-quality runner (deterministic).

Shells out to the built `loregrep` binary over its `exec-tool` CLI surface,
compares structured results against a hand-reviewed gold set, and reports
precision/recall/F1 per case. Stdlib only; no third-party dependencies.

Usage:
    cargo build --release
    python3 evals/retrieval/run.py [--binary target/release/loregrep]
                                   [--fixture rust-basic]
                                   [--case '<glob>']
                                   [--json-out <path>]

Exit code is non-zero if any case fails unexpectedly, OR if a case listed in
known_failures.json unexpectedly PASSES (xfail contract — forces ledger cleanup).

Design notes (verified empirically against loregrep 0.4.2):
  * loregrep stores every symbol's `file_path` as `--path` joined with the
    file's path relative to the scan root, but its internal file index (used by
    get_dependencies) is keyed by the ABSOLUTE canonical path. So we always scan
    with an absolute `--path` and absolutize any `file_path` *param*, then
    relativize each result `file_path` back to the fixture root for comparison.
"""

import argparse
import fnmatch
import json
import os
import random
import string
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
EVALS_DIR = os.path.dirname(HERE)                       # evals/
REPO_ROOT = os.path.dirname(EVALS_DIR)                  # repo root
FIXTURES_DIR = os.path.join(EVALS_DIR, "fixtures")
RESULTS_DIR = os.path.join(HERE, "results")
KNOWN_FAILURES_PATH = os.path.join(HERE, "known_failures.json")

SCHEMA_CASE = "loregrep-eval-retrieval/1"
SCHEMA_SUMMARY = "loregrep-eval-retrieval-summary/1"

# Param keys whose values are file paths and must be absolutized before the call.
PATH_PARAM_KEYS = {"file_path"}


# --------------------------------------------------------------------------- #
# Helpers
# --------------------------------------------------------------------------- #

def git_sha():
    try:
        out = subprocess.run(
            ["git", "-C", REPO_ROOT, "rev-parse", "HEAD"],
            capture_output=True, text=True, timeout=10,
        )
        return out.stdout.strip() or "unknown"
    except Exception:
        return "unknown"


def dig(obj, dotted):
    """Follow a dot-path (e.g. 'data.metadata.languages') into nested dicts."""
    cur = obj
    for part in dotted.split("."):
        if not isinstance(cur, dict) or part not in cur:
            return None
        cur = cur[part]
    return cur


def relativize_path(p, fixture_abs):
    """Relativize a result path to the fixture root, POSIX.

    loregrep emits ROOT-RELATIVE paths (relative to the `--path` it was given,
    which is `fixture_abs` here), so a relative path is anchored at the scan root,
    not at this repo's root. Absolute paths are still accepted: input tolerance is
    the point, and older/embedding callers may hand back either.
    """
    if not isinstance(p, str):
        return p
    ap = p if os.path.isabs(p) else os.path.join(fixture_abs, p)
    ap = os.path.normpath(ap)
    try:
        rel = os.path.relpath(ap, fixture_abs)
    except ValueError:
        return p
    return rel.replace(os.sep, "/")


def normalize_item_paths(item, fixture_abs):
    """Return a copy of an item dict with any file_path field relativized."""
    if not isinstance(item, dict):
        return item
    out = dict(item)
    if "file_path" in out:
        out["file_path"] = relativize_path(out["file_path"], fixture_abs)
    return out


def project(item, match_on):
    """Project an item onto match_on keys, producing a canonical hashable repr.

    Lists (e.g. `generics`) are converted to tuples so they remain hashable.
    When match_on is None the item is used whole (string arrays like deps).
    """
    if match_on is None:
        return json.dumps(item, sort_keys=True) if isinstance(item, (dict, list)) else item
    proj = {}
    for key in match_on:
        v = item.get(key) if isinstance(item, dict) else None
        proj[key] = v
    return json.dumps(proj, sort_keys=True)


def absolutize_params(params, fixture_abs):
    out = dict(params)
    for k in list(out.keys()):
        if k in PATH_PARAM_KEYS and isinstance(out[k], str) and not os.path.isabs(out[k]):
            out[k] = os.path.join(fixture_abs, out[k])
    return out


def score(expected_items, actual_items, match_on):
    """Set-compare projected expected vs actual. Returns (metrics, fps, fns)."""
    exp_pairs = [(project(e, match_on), e) for e in expected_items]
    act_pairs = [(project(a, match_on), a) for a in actual_items]
    exp_keys = {k for k, _ in exp_pairs}
    act_keys = {k for k, _ in act_pairs}

    tp = len(exp_keys & act_keys)
    fp_keys = act_keys - exp_keys
    fn_keys = exp_keys - act_keys

    n_fp = len(fp_keys)
    n_fn = len(fn_keys)

    precision = tp / (tp + n_fp) if (tp + n_fp) > 0 else 1.0
    recall = tp / (tp + n_fn) if (tp + n_fn) > 0 else 1.0
    f1 = (2 * precision * recall / (precision + recall)) if (precision + recall) > 0 else 0.0

    fps = [orig for k, orig in act_pairs if k in fp_keys]
    fns = [orig for k, orig in exp_pairs if k in fn_keys]
    return (
        {"precision": round(precision, 4), "recall": round(recall, 4), "f1": round(f1, 4),
         "tp": tp, "fp": n_fp, "fn": n_fn},
        fps, fns,
    )


# --------------------------------------------------------------------------- #
# Core execution
# --------------------------------------------------------------------------- #

def run_tool(binary, tool, params, fixture_abs, timeout=60, cwd=None):
    """Invoke `loregrep exec-tool` and return (parsed_json, exit_code, stderr_tail).

    `cwd` matters more than it looks: config discovery starts at `loregrep.toml`
    RELATIVE TO THE WORKING DIRECTORY (CliConfig::default_config_paths), and a
    discovered config REPLACES the built-in include/exclude patterns. Running from
    this repo's root therefore silently applied the developer's own config to the
    scanned tree — which is how a stray `*.test.ts` exclusion once removed 59
    symbols from a corpus scorecard. Callers measuring anything reproducible must
    pin cwd to the tree under test, so the result depends only on pinned inputs.
    """
    cmd = [binary, "exec-tool", tool,
           "--params", json.dumps(params),
           "--path", fixture_abs]
    t0 = time.time()
    proc = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout,
                          cwd=cwd)
    latency_ms = int((time.time() - t0) * 1000)
    stderr_tail = "\n".join(proc.stderr.strip().splitlines()[-3:]) if proc.stderr else ""
    parsed = None
    try:
        parsed = json.loads(proc.stdout)
    except json.JSONDecodeError:
        parsed = None
    return parsed, proc.returncode, stderr_tail, latency_ms


def evaluate_case(case, parsed, fixture_abs):
    """Return (passed, metrics, fps, fns, detail) for a parsed ToolResult."""
    expect = case["expect"]
    mode = expect["mode"]
    match_on = case.get("match_on")

    if parsed is None:
        return False, {"precision": 0.0, "recall": 0.0, "f1": 0.0}, [], [], "no valid JSON on stdout"

    if mode == "error":
        success = parsed.get("success", None)
        err = parsed.get("error") or ""
        needle = expect.get("error_contains", "")
        passed = (success is False) and (needle in (err or ""))
        detail = "expected error not returned" if not passed else "error as expected"
        return passed, {"precision": 1.0 if passed else 0.0,
                        "recall": 1.0 if passed else 0.0,
                        "f1": 1.0 if passed else 0.0}, [], [], detail

    raw = dig(parsed, case["extract"])
    if raw is None:
        return False, {"precision": 0.0, "recall": 0.0, "f1": 0.0}, [], [], \
            "extract path '%s' missing" % case["extract"]

    # Normalize into a list of items.
    if isinstance(raw, list):
        actual = [normalize_item_paths(x, fixture_abs) for x in raw]
    else:
        actual = [raw]  # scalar extract -> singleton set

    expected = expect.get("items", [])
    metrics, fps, fns = score(expected, actual, match_on)

    if mode == "exact_set":
        passed = (metrics["fp"] == 0 and metrics["fn"] == 0)
    elif mode == "superset":
        min_p = expect.get("min_precision", 1.0)
        passed = (metrics["fn"] == 0 and metrics["precision"] >= min_p)
    else:
        return False, metrics, fps, fns, "unknown expect.mode: %s" % mode

    return passed, metrics, fps, fns, mode


# --------------------------------------------------------------------------- #
# Runner
# --------------------------------------------------------------------------- #

def load_known_failures():
    if not os.path.exists(KNOWN_FAILURES_PATH):
        return {}
    with open(KNOWN_FAILURES_PATH) as f:
        data = json.load(f)
    return {e["id"]: e for e in data}


def run_corpus(args, binary):
    """`--corpus <id>`: definition parity against the SCIP golden.

    A thin delegation to corpus_score, deliberately sharing this module's
    helpers rather than duplicating them — two runners that can disagree about
    what "the same result" means is a bug factory. The fixture path above and
    this one answer different questions: fixtures pin contracts and traps an
    oracle cannot express, the corpus measures population-scale recall.
    """
    import corpus_score

    corpus_dir = os.path.join(REPO_ROOT, "evals", "corpus", args.corpus)
    golden = os.path.join(corpus_dir, "golden-symbols.json")
    src = os.path.join(corpus_dir, "src")
    if not os.path.exists(golden):
        print("ERROR: golden not found: %s\n  generate it with "
              "evals/corpus/scripts/scip_to_golden.py" % golden, file=sys.stderr)
        return 2
    if not os.path.isdir(src):
        print("ERROR: corpus checkout not found: %s\n  fetch it with "
              "./evals/corpus/fetch.sh %s" % (src, args.corpus), file=sys.stderr)
        return 2
    argv = ["--golden", golden, "--src", src, "--binary", binary]
    if args.json_out:
        argv += ["--json-out", args.json_out]
    return corpus_score.main(argv)


def main():
    ap = argparse.ArgumentParser(description="Loregrep Layer 1 retrieval runner")
    ap.add_argument("--binary", default=os.environ.get("LOREGREP_BIN",
                    os.path.join(REPO_ROOT, "target/release/loregrep")))
    ap.add_argument("--fixture", default="rust-basic")
    ap.add_argument("--case", default="*", help="glob over case ids")
    ap.add_argument("--json-out", default=None)
    ap.add_argument("--no-latency", action="store_true",
                    help="(accepted for compatibility; the slice does a single cold pass)")
    ap.add_argument("--corpus", default=None, metavar="REPO_ID",
                    help="score population-scale DEFINITION PARITY against the SCIP "
                         "golden for a pinned corpus repo (see EVAL_PLAN.md 4b) "
                         "instead of running the hand-written fixture cases. "
                         "Requires evals/corpus/<id>/golden-symbols.json and a "
                         "checkout at evals/corpus/<id>/src (./evals/corpus/fetch.sh).")
    args = ap.parse_args()

    binary = os.path.abspath(args.binary)
    if not os.path.exists(binary):
        print("ERROR: binary not found: %s\n  build it with: cargo build --release" % binary,
              file=sys.stderr)
        return 2

    if args.corpus:
        return run_corpus(args, binary)

    fixture_abs = os.path.abspath(os.path.join(FIXTURES_DIR, args.fixture))
    gold_path = os.path.join(fixture_abs, "gold", "cases.json")
    if not os.path.exists(gold_path):
        print("ERROR: gold not found: %s" % gold_path, file=sys.stderr)
        return 2

    with open(gold_path) as f:
        cases = json.load(f)
    cases = [c for c in cases if fnmatch.fnmatch(c["id"], args.case)]
    if not cases:
        print("No cases match glob %r" % args.case, file=sys.stderr)
        return 2

    known_failures = load_known_failures()
    sha = git_sha()
    run_id = "%s-%s" % (time.strftime("%Y%m%dT%H%M%S"),
                        "".join(random.choices(string.ascii_lowercase + string.digits, k=6)))

    # Cold pass: clear the per-fixture index cache so the first call rescans.
    cache_dir = os.path.join(fixture_abs, ".loregrep")
    if os.path.isdir(cache_dir):
        subprocess.run(["rm", "-rf", cache_dir])

    os.makedirs(RESULTS_DIR, exist_ok=True)
    out_path = args.json_out or os.path.join(RESULTS_DIR, "%s.jsonl" % run_id)

    rows = []
    wall_start = time.time()
    per_tool = {}     # tool -> aggregate
    unexpected = 0

    for idx, case in enumerate(cases):
        cid = case["id"]
        tool = case["tool"]
        params = absolutize_params(case.get("params", {}), fixture_abs)

        parsed, exit_code, stderr_tail, latency_ms = run_tool(binary, tool, params, fixture_abs)
        passed, metrics, fps, fns, detail = evaluate_case(case, parsed, fixture_abs)

        is_known = cid in known_failures
        # xfail contract:
        #   known failure that still fails -> expected (ok)
        #   known failure that now passes  -> UNEXPECTED pass (fail the run)
        #   normal case that fails         -> UNEXPECTED failure (fail the run)
        if is_known:
            status = "xfail" if not passed else "XPASS"
            unexpected_this = passed  # xpass is unexpected
        else:
            status = "pass" if passed else "FAIL"
            unexpected_this = not passed
        if unexpected_this:
            unexpected += 1

        is_cold = (idx == 0)
        row = {
            "schema": SCHEMA_CASE,
            "run_id": run_id,
            "case_id": cid,
            "fixture": args.fixture,
            "tool": tool,
            "git_sha": sha,
            "binary": os.path.relpath(binary, REPO_ROOT),
            "passed": passed,
            "known_failure": is_known,
            "status": status,
            "precision": metrics["precision"],
            "recall": metrics["recall"],
            "f1": metrics["f1"],
            "false_positives": fps,
            "false_negatives": fns,
            "latency_ms_cold": latency_ms if is_cold else None,
            "latency_ms_warm": None if is_cold else latency_ms,
            "exit_code": exit_code,
            "stderr_tail": stderr_tail,
            "detail": detail,
        }
        rows.append(row)

        agg = per_tool.setdefault(tool, {"n": 0, "pass": 0, "p": 0.0, "r": 0.0})
        agg["n"] += 1
        agg["pass"] += 1 if (passed or is_known) else 0  # documented-or-passing
        agg["p"] += metrics["precision"]
        agg["r"] += metrics["recall"]

    wall_ms = int((time.time() - wall_start) * 1000)

    # ---------------- write JSONL ----------------
    summary_row = {
        "schema": SCHEMA_SUMMARY,
        "run_id": run_id,
        "fixture": args.fixture,
        "git_sha": sha,
        "n_cases": len(rows),
        "n_unexpected": unexpected,
        "wall_ms": wall_ms,
        "per_tool": {t: {"n": a["n"],
                         "avg_precision": round(a["p"] / a["n"], 4),
                         "avg_recall": round(a["r"] / a["n"], 4)}
                     for t, a in per_tool.items()},
    }
    with open(out_path, "w") as f:
        for row in rows:
            f.write(json.dumps(row) + "\n")
        f.write(json.dumps(summary_row) + "\n")

    # ---------------- human summary ----------------
    print("=" * 74)
    print("loregrep Layer 1 retrieval  |  fixture=%s  sha=%s" % (args.fixture, sha[:8]))
    print("=" * 74)
    hdr = "%-46s %-6s %5s %5s" % ("case", "status", "P", "R")
    print(hdr)
    print("-" * 74)
    for row in rows:
        mark = {"pass": "ok", "FAIL": "FAIL", "xfail": "xfail", "XPASS": "XPASS!"}[row["status"]]
        print("%-46s %-6s %5.2f %5.2f" % (
            row["case_id"][:46], mark, row["precision"], row["recall"]))
        if row["status"] in ("FAIL", "XPASS"):
            if row["false_negatives"]:
                print("      missing (FN): %s" % json.dumps(row["false_negatives"]))
            if row["false_positives"]:
                print("      spurious (FP): %s" % json.dumps(row["false_positives"]))
            if row["detail"]:
                print("      detail: %s" % row["detail"])
    print("-" * 74)
    print("per-tool avg P/R:")
    for t, a in sorted(per_tool.items()):
        print("  %-24s n=%d  P=%.2f  R=%.2f" % (
            t, a["n"], a["p"] / a["n"], a["r"] / a["n"]))
    print("-" * 74)
    n_pass = sum(1 for r in rows if r["status"] == "pass")
    n_xfail = sum(1 for r in rows if r["status"] == "xfail")
    n_fail = sum(1 for r in rows if r["status"] == "FAIL")
    n_xpass = sum(1 for r in rows if r["status"] == "XPASS")
    print("totals: pass=%d  xfail(known)=%d  FAIL=%d  XPASS=%d  wall=%dms" % (
        n_pass, n_xfail, n_fail, n_xpass, wall_ms))
    print("results -> %s" % os.path.relpath(out_path, REPO_ROOT))
    if unexpected:
        print("RESULT: FAILED (%d unexpected outcome(s))" % unexpected)
    else:
        print("RESULT: OK (all cases pass or are documented known-failures)")
    print("=" * 74)

    return 1 if unexpected else 0


if __name__ == "__main__":
    sys.exit(main())
