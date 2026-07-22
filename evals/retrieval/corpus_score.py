#!/usr/bin/env python3
"""Loregrep Layer 1 corpus parity scorer — DEFINITIONS ONLY (eval plan task L1-S3).

Scores loregrep's symbol *inventory* for one pinned corpus repo against a
SCIP-derived golden (evals/corpus/golden.schema.json). See evals/EVAL_PLAN.md
section 4b.2: definitions are the strict, gated axis. Edge (call/import) parity is
deliberately NOT implemented here — it is deferred to P3-7 (4b.6), because P3-3/P3-4
change caller semantics and an edge golden built now would be triaged twice.

Usage (standalone):
    cargo build --release
    python3 evals/retrieval/corpus_score.py \
        --golden evals/corpus/<repo>/golden-symbols.json \
        --src    evals/corpus/<repo>/src \
        [--binary target/release/loregrep] [--json-out <path>]

Usage (as a module, e.g. from run.py's future --corpus mode):
    from corpus_score import collect_symbols, score_symbols, score_golden

EXIT CODE IS ALWAYS 0, DELIBERATELY. This runs locally until the number
stabilizes; per EVAL_PLAN 4b.2 definitions are the *gated* axis, but the gate
thresholds are chosen AFTER the first real corpus number, never before. Wiring a
threshold in now would either be vacuous or would encode a guess as a contract.
The scorecard carries "gate": null to make that explicit to consumers.

Shared helpers (`run_tool`, `normalize_item_paths`, `relativize_path`, `git_sha`)
are imported from run.py rather than reimplemented — one definition of "the same
result" (4b.4). run.py does no work at import time (module level is constants
only), so a plain import is safe; it is still wrapped so a future regression there
produces a clear message instead of a stack trace.
"""

import argparse
import fnmatch
import json
import os
import posixpath
import random
import string
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
EVALS_DIR = os.path.dirname(HERE)
REPO_ROOT = os.path.dirname(EVALS_DIR)
RESULTS_DIR = os.path.join(HERE, "results")

if HERE not in sys.path:
    sys.path.insert(0, HERE)
try:
    import run as _run  # the existing Layer 1 runner; constants-only at import time
except Exception as exc:  # pragma: no cover - defensive
    raise SystemExit(
        "corpus_score: cannot import evals/retrieval/run.py (%s).\n"
        "  The corpus scorer reuses its helpers on purpose; fix run.py rather than "
        "forking the helpers." % exc
    )

run_tool = _run.run_tool
normalize_item_paths = _run.normalize_item_paths
relativize_path = _run.relativize_path
git_sha = _run.git_sha

SCHEMA_SCORECARD = "loregrep-eval-corpus/1"

# Cap on how many examples we serialize per bucket, so a scorecard stays
# human-triageable rather than becoming a second copy of the golden.
EXAMPLE_CAP = 50

# Enumeration knobs. `limit` defaults to 20 in the tool schema, which would
# silently truncate a real repo into a ~0.01 recall number.
ENUMERATE_PATTERN = ".*"
ENUMERATE_LIMIT = 1000000

# Golden `language` -> file extensions. We bucket emitted symbols by extension
# ourselves rather than passing loregrep's own `language` filter param: the
# scorer must not let the system under test decide which of its answers are in
# scope (4b.5 independence).
LANGUAGE_EXTENSIONS = {
    "rust": (".rs",),
    "python": (".py", ".pyi"),
    "typescript": (".ts", ".tsx"),
    "javascript": (".js", ".jsx", ".mjs", ".cjs"),
}

# loregrep TypeKind (serde snake_case) -> golden neutral kind vocabulary.
# Only `abstract_class` needs collapsing; the golden vocabulary has no such kind.
TYPE_KIND_MAP = {"abstract_class": "class"}

TEST_DIR_COMPONENTS = {"test", "tests", "__tests__", "testing", "spec", "specs"}
TEST_FILE_GLOBS = (
    "test_*.py", "*_test.py", "tests.py", "conftest.py",
    "*_test.rs", "*_tests.rs",
    "*.test.ts", "*.test.tsx", "*.test.js", "*.test.jsx",
    "*.spec.ts", "*.spec.tsx", "*.spec.js", "*.spec.jsx",
)
GENERATED_DIR_COMPONENTS = {
    "node_modules", "target", "dist", "build", "vendor", ".venv", "venv",
    "site-packages", "generated", "__generated__", "__pycache__",
}
GENERATED_FILE_GLOBS = ("*_pb2.py", "*_pb2_grpc.py", "*.pb.go", "*.generated.ts",
                        "*.generated.js", "*.g.dart")


# --------------------------------------------------------------------------- #
# Normalization: golden + emitted -> one neutral symbol shape
# --------------------------------------------------------------------------- #

def _sym(name, kind, file, start_line, end_line, owner=None, source=None, flags=None):
    return {
        "name": name,
        "kind": kind,
        "file": file,
        "start_line": start_line,
        "end_line": end_line,
        "owner": owner or None,
        "source": source,
        # Golden-side only: the oracle's per-symbol attributes, which policy may
        # put out of scope (notably `generated`). Must survive normalization —
        # dropping it here silently disables every flag-based policy rule.
        "flags": list(flags or []),
    }


def normalize_golden_symbol(s):
    return _sym(s.get("name"), s.get("kind"), s.get("file"),
                s.get("start_line"), s.get("end_line"),
                s.get("owner"), source="golden", flags=s.get("flags"))


def normalize_emitted_function(item, src_abs):
    """A `search_functions` result -> neutral symbol.

    loregrep has no separate `method` kind for callables: a method is a function
    carrying a non-null `owner` (the impl block / class). The golden vocabulary
    splits them, so we split them the same way here.
    """
    it = normalize_item_paths(item, src_abs)
    owner = it.get("owner") or None
    return _sym(it.get("name"), "method" if owner else "function",
                it.get("file_path"), it.get("start_line"), it.get("end_line"),
                owner, source="loregrep")


def normalize_emitted_type(item, src_abs):
    """A `search_structs` result -> neutral symbol (structs/enums/traits/classes/...)."""
    it = normalize_item_paths(item, src_abs)
    kind = it.get("kind") or "struct"
    kind = TYPE_KIND_MAP.get(kind, kind)
    return _sym(it.get("name"), kind, it.get("file_path"),
                it.get("start_line"), it.get("end_line"), None, source="loregrep")


def language_of_path(path):
    ext = posixpath.splitext(path or "")[1].lower()
    for lang, exts in LANGUAGE_EXTENSIONS.items():
        if ext in exts:
            return lang
    return None


# --------------------------------------------------------------------------- #
# Policy
# --------------------------------------------------------------------------- #

def path_excluded(path, excluded_paths):
    """True if `path` is under / matches any excluded_paths entry."""
    for e in excluded_paths or []:
        e = e.rstrip("/")
        if not e:
            continue
        if any(ch in e for ch in "*?["):
            if fnmatch.fnmatch(path, e) or fnmatch.fnmatch(path, e + "/*"):
                return True
        elif path == e or path.startswith(e + "/"):
            return True
    return False


def looks_like_test(path):
    parts = path.split("/")
    if any(p.lower() in TEST_DIR_COMPONENTS for p in parts[:-1]):
        return True
    base = parts[-1]
    return any(fnmatch.fnmatch(base, g) for g in TEST_FILE_GLOBS)


def looks_like_generated(path):
    parts = path.split("/")
    if any(p in GENERATED_DIR_COMPONENTS for p in parts[:-1]):
        return True
    base = parts[-1]
    return any(fnmatch.fnmatch(base, g) for g in GENERATED_FILE_GLOBS)


def nested_symbol_ids(symbols):
    """ids of callables strictly enclosed by another callable in the same file.

    loregrep does not flag nesting, so we derive it from spans: a function whose
    [start,end] sits strictly inside another function's span in the same file is
    a nested/inner function. Only consulted when the golden's policy says nested
    functions were excluded from the oracle side.
    """
    out = set()
    by_file = {}
    for s in symbols:
        if s["kind"] in ("function", "method"):
            by_file.setdefault(s["file"], []).append(s)
    for _f, group in by_file.items():
        for a in group:
            for b in group:
                if a is b:
                    continue
                if _span_ok(a) and _span_ok(b) \
                        and b["start_line"] <= a["start_line"] \
                        and a["end_line"] <= b["end_line"] \
                        and (b["start_line"], b["end_line"]) != (a["start_line"], a["end_line"]):
                    out.add(id(a))
                    break
    return out


def _span_ok(s):
    return isinstance(s.get("start_line"), int) and isinstance(s.get("end_line"), int)


def classify_exclusion(sym, policy, nested_ids):
    """Return an exclusion bucket name, or None if the symbol is in scope.

    NEVER folded into precision: an excluded symbol is removed from the
    precision denominator AND reported in its own counted bucket, because
    "loregrep emits things SCIP does not" is a policy fact, not a defect, and
    silently dropping it would hide a real regression behind a policy rule.
    """
    p = policy or {}
    if path_excluded(sym["file"], p.get("excluded_paths")):
        return "excluded_path"
    if p.get("include_generated") is False and looks_like_generated(sym["file"]):
        return "generated"
    if p.get("include_test_symbols") is False and looks_like_test(sym["file"]):
        return "test"
    if p.get("include_nested_functions") is False and id(sym) in nested_ids:
        return "nested"
    return None


# --------------------------------------------------------------------------- #
# Matching
# --------------------------------------------------------------------------- #

def _pair_cost(g, e):
    """Lower is better. Kind agreement, then owner agreement, then span distance."""
    kind_pen = 0 if g["kind"] == e["kind"] else 1
    owner_pen = 0
    if g.get("owner") and e.get("owner"):
        owner_pen = 0 if g["owner"] == e["owner"] else 1
    delta = abs((g.get("start_line") or 0) - (e.get("start_line") or 0))
    return (kind_pen, owner_pen, delta)


def _greedy_pair(golden_group, emitted_group):
    """Pair within one candidate group by ascending cost. Returns (pairs, gl, el)."""
    cands = []
    for gi, g in enumerate(golden_group):
        for ei, e in enumerate(emitted_group):
            cands.append((_pair_cost(g, e), gi, ei))
    cands.sort(key=lambda c: (c[0], c[1], c[2]))
    used_g, used_e, pairs = set(), set(), []
    for _cost, gi, ei in cands:
        if gi in used_g or ei in used_e:
            continue
        used_g.add(gi)
        used_e.add(ei)
        pairs.append((golden_group[gi], emitted_group[ei]))
    gl = [g for i, g in enumerate(golden_group) if i not in used_g]
    el = [e for i, e in enumerate(emitted_group) if i not in used_e]
    return pairs, gl, el


def match_symbols(golden, emitted):
    """Match on (file, name), span proximity as tiebreak; then (name) across files.

    Returns (pairs, wrong_file_pairs, missing, extra).
      pairs            -- same file, same name (kind may still disagree)
      wrong_file_pairs -- same name, DIFFERENT file: a real defect, but a
                          different one from "not found at all", so it is never
                          merged into the missing bucket.
    """
    g_by_key, e_by_key = {}, {}
    for g in golden:
        g_by_key.setdefault((g["file"], g["name"]), []).append(g)
    for e in emitted:
        e_by_key.setdefault((e["file"], e["name"]), []).append(e)

    pairs, missing, extra = [], [], []
    for key in sorted(set(g_by_key) | set(e_by_key)):
        gs, es = g_by_key.get(key, []), e_by_key.get(key, [])
        p, gl, el = _greedy_pair(gs, es)
        pairs.extend(p)
        missing.extend(gl)
        extra.extend(el)

    # Second pass: same bare name, different file.
    g_by_name, e_by_name = {}, {}
    for g in missing:
        g_by_name.setdefault(g["name"], []).append(g)
    for e in extra:
        e_by_name.setdefault(e["name"], []).append(e)
    wrong_file, still_missing, still_extra = [], [], []
    for name in sorted(set(g_by_name) | set(e_by_name)):
        p, gl, el = _greedy_pair(g_by_name.get(name, []), e_by_name.get(name, []))
        wrong_file.extend(p)
        still_missing.extend(gl)
        still_extra.extend(el)

    return pairs, wrong_file, still_missing, still_extra


# --------------------------------------------------------------------------- #
# Scoring
# --------------------------------------------------------------------------- #

def _ex(sym, **extra):
    """A compact, human-triageable example: file:line plus identity."""
    out = {
        "name": sym.get("name"),
        "kind": sym.get("kind"),
        "at": "%s:%s" % (sym.get("file"), sym.get("start_line")),
        "end_line": sym.get("end_line"),
    }
    if sym.get("owner"):
        out["owner"] = sym["owner"]
    out.update(extra)
    return out


def _cap(items):
    return items[:EXAMPLE_CAP], len(items) > EXAMPLE_CAP


def _ratio(num, den):
    return round(num / den, 4) if den else None


def score_symbols(golden_symbols, emitted_symbols, language, policy=None):
    """Pure scorer: neutral golden symbols vs neutral emitted symbols.

    Both inputs are lists in the `_sym` shape. No subprocess, no filesystem —
    this is the function the unit tests drive.
    """
    policy = policy or {}
    excluded_paths = policy.get("excluded_paths")

    # Symmetric policy application on the golden side: a golden entry that policy
    # puts out of scope is out of scope, not a recall miss. Two sources:
    #   - path exclusions (vendored trees, examples, ...)
    #   - the golden's own per-symbol `flags`. `generated` is the important one:
    #     an oracle that expands macros reports definitions that exist in no
    #     source file, and a syntax-level parser structurally cannot see them.
    #     Counting those as misses would permanently cap recall at a number that
    #     says nothing about parser quality. They are counted in their own
    #     bucket instead, so the cost of the limitation stays visible.
    flag_excluded_kinds = []
    if policy.get("include_generated") is False:
        flag_excluded_kinds.append("generated")
    if policy.get("include_test_symbols") is False:
        flag_excluded_kinds.append("test")

    golden_in, golden_excluded = [], []
    golden_flag_excluded = {}
    for g in golden_symbols:
        if path_excluded(g["file"], excluded_paths):
            golden_excluded.append(g)
            continue
        flagged = next((f for f in flag_excluded_kinds if f in (g.get("flags") or [])), None)
        if flagged:
            golden_flag_excluded.setdefault("golden_" + flagged, []).append(g)
            continue
        golden_in.append(g)

    # Emitted side: keep only this golden's language, by extension.
    emitted_lang, emitted_other_lang = [], []
    for e in emitted_symbols:
        (emitted_lang if language_of_path(e["file"]) == language
         else emitted_other_lang).append(e)

    # Files the oracle never indexed at all (see the post-match guard below).
    oracle_files = {g["file"] for g in golden_symbols}

    pairs, wrong_file, missing, extra = match_symbols(golden_in, emitted_lang)

    # Policy exclusions apply ONLY to unmatched emitted symbols. Anything that
    # matched a golden entry is in scope by construction.
    nested_ids = nested_symbol_ids(emitted_lang) \
        if policy.get("include_nested_functions") is False else set()
    excluded = {}
    false_positives = []
    emitted_out_of_scope = []
    for e in extra:
        # Parity is only meaningful where BOTH sides can speak. An indexer builds
        # from tsconfig/package projects and may cover a strict subset of the
        # tree (scip-typescript indexes no *.test.ts in hono), so an UNMATCHED
        # symbol in a file with zero golden entries is unmeasurable, not a false
        # positive. Applied after matching on purpose: doing it earlier would
        # swallow wrong-file findings, where the whole point is that loregrep put
        # a real symbol somewhere the oracle did not.
        bucket = classify_exclusion(e, policy, nested_ids)
        if bucket is None and e["file"] not in oracle_files:
            emitted_out_of_scope.append(e)
            continue
        if bucket:
            excluded.setdefault(bucket, []).append(e)
        else:
            false_positives.append(e)

    # ---- bucket everything by kind -------------------------------------- #
    kinds = set()
    for g in golden_in:
        kinds.add(g["kind"])
    for e in false_positives:
        kinds.add(e["kind"])
    per_kind = {}
    for k in sorted(kinds):
        per_kind[k] = {
            "kind": k, "golden_total": 0, "emitted_total": 0, "matched": 0,
            "kind_mismatch": 0, "span_exact": 0, "span_start_exact": 0,
            "span_mismatch": 0, "wrong_file": 0, "missing": 0,
            "false_positives_n": 0,
            "_fn": [], "_fp": [], "_span": [], "_kindmm": [],
        }

    for g, e in pairs:
        b = per_kind[g["kind"]]
        b["golden_total"] += 1
        b["emitted_total"] += 1
        b["matched"] += 1
        start_exact = g.get("start_line") == e.get("start_line")
        end_exact = g.get("end_line") == e.get("end_line")
        if start_exact:
            b["span_start_exact"] += 1
        if start_exact and end_exact:
            b["span_exact"] += 1
        else:
            b["span_mismatch"] += 1
            b["_span"].append(_ex(g, golden_span=[g.get("start_line"), g.get("end_line")],
                                  loregrep_span=[e.get("start_line"), e.get("end_line")]))
        if g["kind"] != e["kind"]:
            b["kind_mismatch"] += 1
            b["_kindmm"].append(_ex(g, loregrep_kind=e["kind"]))

    for g, e in wrong_file:
        b = per_kind[g["kind"]]
        b["golden_total"] += 1
        b["emitted_total"] += 1
        b["wrong_file"] += 1
        b["_fn"].append(_ex(g, reason="wrong_file",
                            loregrep_at="%s:%s" % (e["file"], e["start_line"])))
        b["_fp"].append(_ex(e, reason="wrong_file",
                            golden_at="%s:%s" % (g["file"], g["start_line"])))

    for g in missing:
        b = per_kind[g["kind"]]
        b["golden_total"] += 1
        b["missing"] += 1
        b["_fn"].append(_ex(g, reason="missing"))

    for e in false_positives:
        b = per_kind[e["kind"]]
        b["emitted_total"] += 1
        b["false_positives_n"] += 1
        b["_fp"].append(_ex(e, reason="unmatched"))

    out_kinds = {}
    for k, b in per_kind.items():
        fns, fn_trunc = _cap(sorted(b.pop("_fn"), key=lambda x: x["at"]))
        fps, fp_trunc = _cap(sorted(b.pop("_fp"), key=lambda x: x["at"]))
        spans, span_trunc = _cap(sorted(b.pop("_span"), key=lambda x: x["at"]))
        kmm, kmm_trunc = _cap(sorted(b.pop("_kindmm"), key=lambda x: x["at"]))
        b["recall"] = _ratio(b["matched"], b["golden_total"])
        b["recall_strict_kind"] = _ratio(b["matched"] - b["kind_mismatch"],
                                         b["golden_total"])
        b["precision"] = _ratio(b["matched"], b["emitted_total"])
        b["span_exact_rate"] = _ratio(b["span_exact"], b["matched"])
        b["false_negatives"] = fns
        b["false_negatives_truncated"] = fn_trunc
        b["false_positives"] = fps
        b["false_positives_truncated"] = fp_trunc
        b["span_mismatches"] = spans
        b["span_mismatches_truncated"] = span_trunc
        b["kind_mismatches"] = kmm
        b["kind_mismatches_truncated"] = kmm_trunc
        out_kinds[k] = b

    tot = {"golden_total": 0, "emitted_total": 0, "matched": 0, "kind_mismatch": 0,
           "span_exact": 0, "span_mismatch": 0, "wrong_file": 0, "missing": 0,
           "false_positives_n": 0}
    for b in out_kinds.values():
        for k in tot:
            tot[k] += b[k]
    tot["recall"] = _ratio(tot["matched"], tot["golden_total"])
    tot["recall_strict_kind"] = _ratio(tot["matched"] - tot["kind_mismatch"],
                                       tot["golden_total"])
    tot["precision"] = _ratio(tot["matched"], tot["emitted_total"])
    tot["span_exact_rate"] = _ratio(tot["span_exact"], tot["matched"])

    excluded_out = {}
    for bucket, items in sorted(excluded.items()):
        ex, trunc = _cap([_ex(s) for s in sorted(items, key=lambda s: (s["file"],
                                                                      s["start_line"] or 0))])
        excluded_out[bucket] = {"count": len(items), "examples": ex, "truncated": trunc}
    if golden_excluded:
        ex, trunc = _cap([_ex(s) for s in golden_excluded])
        excluded_out["golden_excluded_path"] = {"count": len(golden_excluded),
                                                "examples": ex, "truncated": trunc}
    for bucket, items in sorted(golden_flag_excluded.items()):
        ex, trunc = _cap([_ex(s) for s in sorted(items, key=lambda s: (s["file"],
                                                                      s["start_line"] or 0))])
        excluded_out[bucket] = {"count": len(items), "examples": ex, "truncated": trunc}
    if emitted_out_of_scope:
        ex, trunc = _cap([_ex(s) for s in emitted_out_of_scope])
        n_files = len({s["file"] for s in emitted_out_of_scope})
        excluded_out["outside_oracle_scope"] = {
            "count": len(emitted_out_of_scope), "examples": ex, "truncated": trunc,
            "distinct_files": n_files}
    if emitted_other_lang:
        ex, trunc = _cap([_ex(s) for s in emitted_other_lang])
        excluded_out["other_language"] = {"count": len(emitted_other_lang),
                                          "examples": ex, "truncated": trunc}

    return {
        "language": language,
        "totals": tot,
        "per_kind": out_kinds,
        # Every symbol we removed from the precision denominator, counted.
        "excluded": excluded_out,
        "excluded_total": sum(v["count"] for v in excluded_out.values()),
    }


# --------------------------------------------------------------------------- #
# Driving the system under test
# --------------------------------------------------------------------------- #

def collect_symbols(binary, src_abs, timeout=600):
    """Enumerate loregrep's whole definition inventory for a source tree.

    Strategy: two `exec-tool` calls, `search_functions` and `search_structs`,
    each with pattern ".*" and an effectively-unbounded `limit`.

    Why this and not per-file `analyze_file` / `get_repository_tree`:
      * `.*` really does enumerate. `RepoMap::matches_pattern` falls through to a
        regex branch whenever the pattern contains a regex metacharacter, and
        `.*` matches every name. Verified empirically against this repo's own
        `src/`: `search_functions .*` returned 761 functions and 86 types, byte
        for byte the same totals as summing every per-file skeleton out of
        `get_repository_tree --include_file_details`. So the two agree on
        coverage, and this path is 2 subprocesses instead of N.
      * The `get_repository_tree` skeletons are strictly poorer: each function
        carries only `line_number` — no `end_line`, no `owner` — so span parity
        and the function/method split could not be scored from them at all.
      * `limit` defaults to 20 in the tool schema; not overriding it would
        truncate any real repo into a meaningless recall number.
    """
    calls = []
    symbols = []
    for tool, normalizer in (("search_functions", normalize_emitted_function),
                             ("search_structs", normalize_emitted_type)):
        params = {"pattern": ENUMERATE_PATTERN, "limit": ENUMERATE_LIMIT}
        # cwd = the pinned checkout, NOT this repo. loregrep discovers
        # `loregrep.toml` relative to the working directory and a discovered
        # config REPLACES the built-in include/exclude patterns, so scoring from
        # the repo root leaked the developer's own config into the measurement
        # (it once silently dropped every *.test.ts file from a corpus, 59
        # symbols, with nothing in the scorecard to show for it). Pinning cwd to
        # the tree under test makes the number depend only on pinned inputs.
        parsed, rc, stderr_tail, latency_ms = run_tool(binary, tool, params,
                                                       src_abs, timeout=timeout,
                                                       cwd=src_abs)
        items = _run.dig(parsed, "data.results") if parsed else None
        ok = isinstance(items, list)
        if ok:
            symbols.extend(normalizer(it, src_abs) for it in items)
        calls.append({"tool": tool, "params": params, "exit_code": rc,
                      "latency_ms": latency_ms, "n_results": len(items) if ok else 0,
                      "ok": ok, "stderr_tail": stderr_tail})
    return symbols, calls


def read_src_commit(src_abs):
    try:
        out = subprocess.run(["git", "-C", src_abs, "rev-parse", "HEAD"],
                             capture_output=True, text=True, timeout=10)
        return out.stdout.strip() or None
    except Exception:
        return None


def load_waivers(golden_path):
    """Per-symbol waivers living beside the golden (`waivers.json`).

    A waiver records that loregrep emitted a REAL definition the oracle does not
    represent — an overload sibling, a pyright-pruned branch, a name the indexer
    models as a local. It is triage, not amnesty: waived items stay counted by
    class, and a waiver that no longer matches any false positive is reported as
    STALE, the same ratchet `known_failures.json` applies to xfails. Without that,
    a genuine regression could hide inside a stack of legitimate quirks.
    """
    path = os.path.join(os.path.dirname(os.path.abspath(golden_path)), "waivers.json")
    if not os.path.exists(path):
        return {}, path
    with open(path) as fh:
        doc = json.load(fh)
    index = {}
    for entry in doc.get("waivers", []):
        index[(entry["at"], entry["name"])] = entry
    return index, path


def apply_waivers(card, waivers):
    """Move waived false positives into counted per-class buckets."""
    if not waivers:
        return card
    matched = set()
    by_class = {}
    for kind, bucket in card.get("per_kind", {}).items():
        kept = []
        for fp in bucket.get("false_positives", []):
            key = (fp["at"], fp["name"])
            hit = waivers.get(key)
            if hit:
                matched.add(key)
                by_class.setdefault(hit["class"], []).append(fp)
                bucket["false_positives_n"] -= 1
            else:
                kept.append(fp)
        bucket["false_positives"] = kept
    for cls, items in sorted(by_class.items()):
        card["excluded"]["waived:" + cls] = {
            "count": len(items),
            "examples": items[:50],
            "truncated": len(items) > 50,
        }
    card["excluded_total"] = sum(v["count"] for v in card["excluded"].values())
    card["totals"]["false_positives_n"] = max(
        0, card["totals"].get("false_positives_n", 0) - len(matched))
    card["totals"]["waived"] = len(matched)
    stale = [dict(k=k, entry=v) for k, v in waivers.items() if k not in matched]
    card["stale_waivers"] = [
        {"at": k[0], "name": k[1], "class": v["class"]}
        for k, v in sorted(waivers.items()) if k not in matched
    ]
    return card


def score_golden(golden, binary, src_abs, allow_commit_mismatch=False,
                 timeout=600):
    """Load-free entry point: golden dict in, full scorecard out."""
    language = golden["language"]
    src_commit = read_src_commit(src_abs)
    commit_match = None
    if src_commit and golden.get("commit"):
        commit_match = (src_commit == golden["commit"])

    card = {
        "schema": SCHEMA_SCORECARD,
        "run_id": "%s-%s" % (time.strftime("%Y%m%dT%H%M%S"),
                             "".join(random.choices(string.ascii_lowercase + string.digits,
                                                    k=6))),
        "generated_at": time.strftime("%Y-%m-%dT%H:%M:%S"),
        "git_sha": git_sha(),
        "repo": golden.get("repo"),
        "commit": golden.get("commit"),
        "src_commit": src_commit,
        "commit_match": commit_match,
        "language": language,
        "generator": golden.get("generator"),
        "policy": golden.get("policy") or {},
        "binary": binary,
        "src": src_abs,
        # Gate thresholds are chosen after the first real corpus number, not
        # before (see module docstring). Consumers must treat null as ungated.
        "gate": None,
    }

    if commit_match is False and not allow_commit_mismatch:
        card["refused"] = "commit_mismatch"
        card["detail"] = ("golden was generated at %s but %s is at %s; "
                          "regenerate the golden or pass --allow-commit-mismatch"
                          % (golden.get("commit"), src_abs, src_commit))
        return card

    # A scored run must start from an empty index cache and must leave the
    # pinned checkout exactly as it found it. Both now hold by construction:
    # every `loregrep` call this harness makes points at `_run.isolated_cache_root()`,
    # a temp directory owned by this process and deleted at exit, so there is
    # nothing to clear before the run and nothing to clean up after it.
    #
    # This used to `rm -rf <src>/.loregrep` on both sides. That hardcoded where
    # loregrep keeps its cache, and would have degraded into a silent no-op the
    # moment that moved — scoring a stale index while still printing a
    # scorecard. Isolation cannot rot that way.
    _run.isolated_cache_root()

    t0 = time.time()
    emitted, calls = collect_symbols(binary, src_abs, timeout=timeout)
    card["tool_calls"] = calls
    card["wall_ms"] = int((time.time() - t0) * 1000)

    if not all(c["ok"] for c in calls):
        card["refused"] = "tool_error"
        card["detail"] = "; ".join("%s: rc=%s %s" % (c["tool"], c["exit_code"],
                                                     c["stderr_tail"])
                                   for c in calls if not c["ok"])
        return card

    golden_syms = [normalize_golden_symbol(s) for s in golden.get("symbols", [])]
    card.update(score_symbols(golden_syms, emitted, language,
                              golden.get("policy")))
    return card


# --------------------------------------------------------------------------- #
# Human summary (same visual style as run.py)
# --------------------------------------------------------------------------- #

def print_summary(card, out_path=None):
    W = 74
    print("=" * W)
    print("loregrep Layer 1 corpus parity (definitions)  |  repo=%s lang=%s sha=%s"
          % (card.get("repo"), card.get("language"), (card.get("git_sha") or "")[:8]))
    print("=" * W)
    if card.get("refused"):
        print("REFUSED: %s" % card["refused"])
        print("  %s" % card.get("detail", ""))
        print("=" * W)
        return

    print("%-14s %6s %6s %5s %5s %6s %6s %6s" % (
        "kind", "gold", "emit", "R", "P", "span%", "spanX", "miss"))
    print("-" * W)
    for k in sorted(card["per_kind"]):
        b = card["per_kind"][k]
        print("%-14s %6d %6d %5s %5s %6s %6d %6d" % (
            k[:14], b["golden_total"], b["emitted_total"],
            _fmt(b["recall"]), _fmt(b["precision"]), _fmt(b["span_exact_rate"]),
            b["span_mismatch"], b["missing"] + b["wrong_file"]))
    t = card["totals"]
    print("-" * W)
    print("%-14s %6d %6d %5s %5s %6s %6d %6d" % (
        "TOTAL", t["golden_total"], t["emitted_total"], _fmt(t["recall"]),
        _fmt(t["precision"]), _fmt(t["span_exact_rate"]), t["span_mismatch"],
        t["missing"] + t["wrong_file"]))
    print("-" * W)
    print("defect split: missing=%d  wrong_file=%d  span_mismatch=%d  kind_mismatch=%d  FP=%d"
          % (t["missing"], t["wrong_file"], t["span_mismatch"], t["kind_mismatch"],
             t["false_positives_n"]))
    if t.get("waived"):
        print("waived (individually triaged, see waivers.json): %d" % t["waived"])
    # A waiver that matches nothing is either fixed upstream or was wrong. Same
    # ratchet as an unexpectedly-passing known_failure: it must be visible.
    if card.get("stale_waivers"):
        print("STALE WAIVERS (no longer match any finding - re-triage or delete):")
        for sw in card["stale_waivers"][:20]:
            print("  %-52s %s [%s]" % (sw["at"], sw["name"], sw["class"]))
    if card["excluded"]:
        print("excluded by policy (NOT counted against precision):")
        for bucket, v in sorted(card["excluded"].items()):
            print("  %-24s %d" % (bucket, v["count"]))
    else:
        print("excluded by policy: none")
    for k in sorted(card["per_kind"]):
        b = card["per_kind"][k]
        for fn in b["false_negatives"][:5]:
            print("  FN [%s] %s %s (%s)" % (k, fn["name"], fn["at"], fn["reason"]))
        for fp in b["false_positives"][:5]:
            print("  FP [%s] %s %s (%s)" % (k, fp["name"], fp["at"], fp["reason"]))
    print("-" * W)
    if out_path:
        try:
            rel = os.path.relpath(out_path, REPO_ROOT)
        except ValueError:
            rel = out_path
        print("scorecard -> %s" % rel)
    print("RESULT: reported (ungated — thresholds are set after the first real number)")
    print("=" * W)


def _fmt(v):
    return "  -  " if v is None else "%5.2f" % v


# --------------------------------------------------------------------------- #
# CLI
# --------------------------------------------------------------------------- #

def main(argv=None):
    ap = argparse.ArgumentParser(
        description="Loregrep corpus definition-parity scorer (definitions only)")
    ap.add_argument("--golden", required=True, help="path to golden-symbols.json")
    ap.add_argument("--src", required=True, help="path to the pinned source checkout")
    ap.add_argument("--binary", default=os.environ.get(
        "LOREGREP_BIN", os.path.join(REPO_ROOT, "target/release/loregrep")))
    ap.add_argument("--json-out", default=None)
    ap.add_argument("--allow-commit-mismatch", action="store_true",
                    help="score even if the checkout is not at the golden's commit")
    # --keep-cache is gone: it meant "do not delete <src>/.loregrep", and the
    # index cache no longer lives in the scanned tree. Every run now uses a
    # private temp cache directory (see score_golden), so there is nothing left
    # for the flag to opt out of.
    ap.add_argument("--timeout", type=int, default=600)
    args = ap.parse_args(argv)

    binary = os.path.abspath(args.binary)
    if not os.path.exists(binary):
        print("ERROR: binary not found: %s\n  build it with: cargo build --release"
              % binary, file=sys.stderr)
        return 0  # see module docstring: never gate on exit code yet
    src_abs = os.path.abspath(args.src)
    if not os.path.isdir(src_abs):
        print("ERROR: src not found: %s" % src_abs, file=sys.stderr)
        return 0
    if not os.path.exists(args.golden):
        print("ERROR: golden not found: %s" % args.golden, file=sys.stderr)
        return 0

    with open(args.golden) as f:
        golden = json.load(f)

    card = score_golden(golden, binary, src_abs,
                        allow_commit_mismatch=args.allow_commit_mismatch,
                        timeout=args.timeout)

    # Provenance: any config file the scanned tree itself carries is part of the
    # pinned fixture; anything else would be a machine-dependent input and must
    # be visible in the scorecard rather than silently applied.
    discovered = [n for n in ("loregrep.toml", ".loregrep.toml")
                  if os.path.exists(os.path.join(src_abs, n))]
    card["config_in_effect"] = discovered or "built-in defaults"

    waivers, waivers_path = load_waivers(args.golden)
    card["waivers_file"] = waivers_path if waivers else None
    card = apply_waivers(card, waivers)

    os.makedirs(RESULTS_DIR, exist_ok=True)
    out_path = args.json_out or os.path.join(
        RESULTS_DIR, "corpus-%s-%s.json" % (card.get("repo") or "unknown",
                                            card["run_id"]))
    with open(out_path, "w") as f:
        json.dump(card, f, indent=2, sort_keys=True)
        f.write("\n")

    print_summary(card, out_path)
    # ALWAYS 0. Gates are chosen after the first real number (EVAL_PLAN 4b.2).
    return 0


if __name__ == "__main__":
    sys.exit(main())
