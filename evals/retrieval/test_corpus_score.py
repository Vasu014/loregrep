#!/usr/bin/env python3
"""Unit tests for the corpus definition-parity scorer.

Stdlib unittest, synthetic goldens and synthetic tool output only: these run
with no corpus checkout and no built binary.

    python3 -m unittest discover -s evals/retrieval -p 'test_*.py'
"""

import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import corpus_score as cs  # noqa: E402


def golden_sym(name, kind, file, start, end, owner=None):
    return {"name": name, "kind": kind, "file": file,
            "start_line": start, "end_line": end, "owner": owner}


def fn_item(name, path, start, end, owner=None):
    """A synthetic `search_functions` result item (absolute file_path, as loregrep emits)."""
    return {"name": name, "file_path": path, "start_line": start, "end_line": end,
            "owner": owner, "is_public": True}


def type_item(name, kind, path, start, end):
    """A synthetic `search_structs` result item."""
    return {"name": name, "kind": kind, "file_path": path,
            "start_line": start, "end_line": end}


SRC = "/tmp/fake-corpus"


def emit(func_items=(), type_items=()):
    out = [cs.normalize_emitted_function(i, SRC) for i in func_items]
    out += [cs.normalize_emitted_type(i, SRC) for i in type_items]
    return out


def norm_golden(syms):
    return [cs.normalize_golden_symbol(s) for s in syms]


def p(rel):
    return os.path.join(SRC, rel)


class TestNormalization(unittest.TestCase):
    def test_function_with_owner_is_a_method(self):
        s = cs.normalize_emitted_function(fn_item("new", p("a.rs"), 3, 5, owner="Foo"), SRC)
        self.assertEqual(s["kind"], "method")
        self.assertEqual(s["owner"], "Foo")
        self.assertEqual(s["file"], "a.rs")

    def test_function_without_owner_is_a_function(self):
        s = cs.normalize_emitted_function(fn_item("main", p("a.rs"), 1, 2), SRC)
        self.assertEqual(s["kind"], "function")

    def test_abstract_class_collapses_to_class(self):
        s = cs.normalize_emitted_type(type_item("Base", "abstract_class", p("a.ts"), 1, 9), SRC)
        self.assertEqual(s["kind"], "class")

    def test_language_of_path(self):
        self.assertEqual(cs.language_of_path("src/a.rs"), "rust")
        self.assertEqual(cs.language_of_path("pkg/a.pyi"), "python")
        self.assertEqual(cs.language_of_path("web/a.tsx"), "typescript")
        self.assertEqual(cs.language_of_path("web/a.mjs"), "javascript")
        self.assertIsNone(cs.language_of_path("README.md"))


class TestExactMatch(unittest.TestCase):
    def test_exact_match_scores_perfect(self):
        g = norm_golden([golden_sym("alpha", "function", "src/a.rs", 10, 20)])
        e = emit([fn_item("alpha", p("src/a.rs"), 10, 20)])
        card = cs.score_symbols(g, e, "rust")
        t = card["totals"]
        self.assertEqual(t["matched"], 1)
        self.assertEqual(t["recall"], 1.0)
        self.assertEqual(t["precision"], 1.0)
        self.assertEqual(t["span_exact"], 1)
        self.assertEqual(t["span_exact_rate"], 1.0)
        self.assertEqual(t["missing"], 0)
        self.assertEqual(t["false_positives_n"], 0)

    def test_other_language_symbols_are_bucketed_not_penalized(self):
        g = norm_golden([golden_sym("alpha", "function", "src/a.rs", 1, 2)])
        e = emit([fn_item("alpha", p("src/a.rs"), 1, 2),
                  fn_item("helper", p("scripts/tool.py"), 1, 2)])
        card = cs.score_symbols(g, e, "rust")
        self.assertEqual(card["totals"]["precision"], 1.0)
        self.assertEqual(card["excluded"]["other_language"]["count"], 1)


class TestSpanMismatchVsMiss(unittest.TestCase):
    def test_span_mismatch_is_a_match_not_a_miss(self):
        g = norm_golden([golden_sym("alpha", "function", "src/a.rs", 10, 20)])
        e = emit([fn_item("alpha", p("src/a.rs"), 11, 20)])  # off-by-one start
        card = cs.score_symbols(g, e, "rust")
        t = card["totals"]
        self.assertEqual(t["matched"], 1)
        self.assertEqual(t["recall"], 1.0)          # found it
        self.assertEqual(t["missing"], 0)           # NOT a miss
        self.assertEqual(t["span_exact"], 0)
        self.assertEqual(t["span_mismatch"], 1)     # reported separately
        self.assertEqual(t["span_exact_rate"], 0.0)
        b = card["per_kind"]["function"]
        self.assertEqual(b["false_negatives"], [])
        self.assertEqual(len(b["span_mismatches"]), 1)
        self.assertEqual(b["span_mismatches"][0]["golden_span"], [10, 20])
        self.assertEqual(b["span_mismatches"][0]["loregrep_span"], [11, 20])

    def test_end_line_only_drift_still_counts_as_span_mismatch(self):
        g = norm_golden([golden_sym("alpha", "function", "src/a.rs", 10, 20)])
        e = emit([fn_item("alpha", p("src/a.rs"), 10, 19)])
        card = cs.score_symbols(g, e, "rust")
        self.assertEqual(card["totals"]["span_mismatch"], 1)
        self.assertEqual(card["per_kind"]["function"]["span_start_exact"], 1)

    def test_missing_symbol_is_a_recall_miss(self):
        g = norm_golden([golden_sym("alpha", "function", "src/a.rs", 10, 20),
                         golden_sym("beta", "function", "src/a.rs", 30, 40)])
        e = emit([fn_item("alpha", p("src/a.rs"), 10, 20)])
        card = cs.score_symbols(g, e, "rust")
        t = card["totals"]
        self.assertEqual(t["missing"], 1)
        self.assertEqual(t["recall"], 0.5)
        self.assertEqual(t["precision"], 1.0)
        fns = card["per_kind"]["function"]["false_negatives"]
        self.assertEqual([f["name"] for f in fns], ["beta"])
        self.assertEqual(fns[0]["reason"], "missing")
        self.assertEqual(fns[0]["at"], "src/a.rs:30")


class TestWrongFile(unittest.TestCase):
    def test_name_found_in_wrong_file(self):
        g = norm_golden([golden_sym("alpha", "function", "src/a.rs", 10, 20)])
        e = emit([fn_item("alpha", p("src/b.rs"), 10, 20)])
        card = cs.score_symbols(g, e, "rust")
        t = card["totals"]
        self.assertEqual(t["wrong_file"], 1)
        self.assertEqual(t["missing"], 0)       # distinct from a plain miss
        self.assertEqual(t["matched"], 0)
        self.assertEqual(t["recall"], 0.0)
        self.assertEqual(t["precision"], 0.0)
        b = card["per_kind"]["function"]
        self.assertEqual(b["false_negatives"][0]["reason"], "wrong_file")
        self.assertEqual(b["false_negatives"][0]["loregrep_at"], "src/b.rs:10")
        self.assertEqual(b["false_positives"][0]["reason"], "wrong_file")
        self.assertEqual(b["false_positives"][0]["golden_at"], "src/a.rs:10")


class TestExtraSymbol(unittest.TestCase):
    def test_extra_tool_symbol_is_a_false_positive(self):
        g = norm_golden([golden_sym("alpha", "function", "src/a.rs", 10, 20)])
        e = emit([fn_item("alpha", p("src/a.rs"), 10, 20),
                  fn_item("ghost", p("src/a.rs"), 50, 60)])
        card = cs.score_symbols(g, e, "rust")
        t = card["totals"]
        self.assertEqual(t["false_positives_n"], 1)
        self.assertEqual(t["recall"], 1.0)
        self.assertEqual(t["precision"], 0.5)
        fps = card["per_kind"]["function"]["false_positives"]
        self.assertEqual([f["name"] for f in fps], ["ghost"])
        self.assertEqual(fps[0]["reason"], "unmatched")


class TestKindMismatch(unittest.TestCase):
    def test_kind_mismatch_is_its_own_bucket_not_fn_plus_fp(self):
        g = norm_golden([golden_sym("Widget", "class", "src/a.ts", 5, 9)])
        e = emit(type_items=[type_item("Widget", "interface", p("src/a.ts"), 5, 9)])
        card = cs.score_symbols(g, e, "typescript")
        t = card["totals"]
        self.assertEqual(t["matched"], 1)
        self.assertEqual(t["kind_mismatch"], 1)
        self.assertEqual(t["missing"], 0)
        self.assertEqual(t["false_positives_n"], 0)
        self.assertEqual(t["recall"], 1.0)
        self.assertEqual(t["recall_strict_kind"], 0.0)
        b = card["per_kind"]["class"]
        self.assertEqual(b["kind_mismatches"][0]["loregrep_kind"], "interface")

    def test_same_kind_preferred_when_two_candidates_share_a_name(self):
        # Two golden `run` defs in one file; the pairing must prefer the same
        # kind + nearest span rather than crossing them over.
        g = norm_golden([golden_sym("run", "function", "src/a.rs", 10, 20),
                         golden_sym("run", "method", "src/a.rs", 40, 50, owner="Job")])
        e = emit([fn_item("run", p("src/a.rs"), 40, 50, owner="Job"),
                  fn_item("run", p("src/a.rs"), 10, 20)])
        card = cs.score_symbols(g, e, "rust")
        self.assertEqual(card["totals"]["matched"], 2)
        self.assertEqual(card["totals"]["kind_mismatch"], 0)
        self.assertEqual(card["totals"]["span_mismatch"], 0)


class TestPolicyExclusions(unittest.TestCase):
    def test_excluded_path_symbols_are_bucketed_not_penalized(self):
        g = norm_golden([golden_sym("alpha", "function", "src/a.rs", 10, 20)])
        e = emit([fn_item("alpha", p("src/a.rs"), 10, 20),
                  fn_item("bench_it", p("benches/bench.rs"), 1, 4)])
        policy = {"excluded_paths": ["benches"]}
        card = cs.score_symbols(g, e, "rust", policy)
        t = card["totals"]
        self.assertEqual(t["precision"], 1.0)          # never folded in
        self.assertEqual(t["false_positives_n"], 0)
        self.assertEqual(card["excluded"]["excluded_path"]["count"], 1)
        self.assertEqual(card["excluded"]["excluded_path"]["examples"][0]["name"],
                         "bench_it")

    def test_excluded_path_applies_symmetrically_to_the_golden(self):
        g = norm_golden([golden_sym("alpha", "function", "src/a.rs", 10, 20),
                         golden_sym("bench_it", "function", "benches/bench.rs", 1, 4)])
        e = emit([fn_item("alpha", p("src/a.rs"), 10, 20)])
        card = cs.score_symbols(g, e, "rust", {"excluded_paths": ["benches"]})
        self.assertEqual(card["totals"]["recall"], 1.0)     # not a miss
        self.assertEqual(card["excluded"]["golden_excluded_path"]["count"], 1)

    def test_test_symbols_bucketed_when_policy_excludes_them(self):
        g = norm_golden([golden_sym("alpha", "function", "src/a.py", 1, 2)])
        e = emit([fn_item("alpha", p("src/a.py"), 1, 2),
                  fn_item("test_alpha", p("tests/test_a.py"), 1, 2)])
        card = cs.score_symbols(g, e, "python", {"include_test_symbols": False})
        self.assertEqual(card["totals"]["precision"], 1.0)
        self.assertEqual(card["excluded"]["test"]["count"], 1)

    def test_test_symbols_are_false_positives_when_policy_includes_them(self):
        # The oracle must have indexed tests/test_a.py for this to be a defect
        # rather than an unmeasurable file — otherwise the out-of-scope guard
        # (correctly) claims it first.
        g = norm_golden([golden_sym("alpha", "function", "src/a.py", 1, 2),
                         golden_sym("test_seen", "function", "tests/test_a.py", 8, 9)])
        e = emit([fn_item("alpha", p("src/a.py"), 1, 2),
                  fn_item("test_seen", p("tests/test_a.py"), 8, 9),
                  fn_item("test_alpha", p("tests/test_a.py"), 1, 2)])
        card = cs.score_symbols(g, e, "python", {"include_test_symbols": True})
        self.assertEqual(card["totals"]["false_positives_n"], 1)
        self.assertNotIn("test", card["excluded"])

    def test_nested_functions_bucketed_by_span_containment(self):
        g = norm_golden([golden_sym("outer", "function", "src/a.py", 1, 20)])
        e = emit([fn_item("outer", p("src/a.py"), 1, 20),
                  fn_item("inner", p("src/a.py"), 5, 9)])
        card = cs.score_symbols(g, e, "python", {"include_nested_functions": False})
        self.assertEqual(card["totals"]["precision"], 1.0)
        self.assertEqual(card["excluded"]["nested"]["count"], 1)

    def test_nested_functions_counted_when_policy_includes_them(self):
        g = norm_golden([golden_sym("outer", "function", "src/a.py", 1, 20)])
        e = emit([fn_item("outer", p("src/a.py"), 1, 20),
                  fn_item("inner", p("src/a.py"), 5, 9)])
        card = cs.score_symbols(g, e, "python", {"include_nested_functions": True})
        self.assertEqual(card["totals"]["false_positives_n"], 1)

    def test_a_matched_symbol_is_never_excluded_by_policy(self):
        # `alpha` lives in a test file but IS in the golden: policy must not
        # remove it, or recall would silently count a real hit as out of scope.
        g = norm_golden([golden_sym("alpha", "function", "tests/test_a.py", 1, 2)])
        e = emit([fn_item("alpha", p("tests/test_a.py"), 1, 2)])
        card = cs.score_symbols(g, e, "python", {"include_test_symbols": False})
        self.assertEqual(card["totals"]["matched"], 1)
        self.assertEqual(card["totals"]["recall"], 1.0)
        self.assertEqual(card["excluded_total"], 0)

    def test_generated_paths_bucketed(self):
        g = norm_golden([golden_sym("alpha", "function", "src/a.py", 1, 2)])
        e = emit([fn_item("alpha", p("src/a.py"), 1, 2),
                  fn_item("Serialize", p("src/thing_pb2.py"), 1, 2)])
        card = cs.score_symbols(g, e, "python", {"include_generated": False})
        self.assertEqual(card["totals"]["precision"], 1.0)
        self.assertEqual(card["excluded"]["generated"]["count"], 1)


class TestPerKindReporting(unittest.TestCase):
    def test_metrics_are_split_per_kind(self):
        g = norm_golden([
            golden_sym("alpha", "function", "src/a.rs", 1, 2),
            golden_sym("beta", "function", "src/a.rs", 4, 5),
            golden_sym("Thing", "struct", "src/a.rs", 10, 12),
            golden_sym("Other", "struct", "src/a.rs", 20, 22),
        ])
        e = emit([fn_item("alpha", p("src/a.rs"), 1, 2),
                  fn_item("beta", p("src/a.rs"), 4, 5)],
                 [type_item("Thing", "struct", p("src/a.rs"), 10, 12)])
        card = cs.score_symbols(g, e, "rust")
        self.assertEqual(card["per_kind"]["function"]["recall"], 1.0)
        self.assertEqual(card["per_kind"]["struct"]["recall"], 0.5)
        self.assertEqual(card["totals"]["recall"], 0.75)

    def test_example_lists_are_capped(self):
        n = cs.EXAMPLE_CAP + 10
        g = norm_golden([golden_sym("f%03d" % i, "function", "src/a.rs", i, i)
                         for i in range(1, n + 1)])
        card = cs.score_symbols(g, [], "rust")
        b = card["per_kind"]["function"]
        self.assertEqual(len(b["false_negatives"]), cs.EXAMPLE_CAP)
        self.assertTrue(b["false_negatives_truncated"])
        self.assertEqual(b["missing"], n)  # the COUNT is not capped

    def test_empty_golden_and_empty_output(self):
        card = cs.score_symbols([], [], "rust")
        self.assertEqual(card["totals"]["golden_total"], 0)
        self.assertIsNone(card["totals"]["recall"])
        self.assertEqual(card["per_kind"], {})


class TestPathHelpers(unittest.TestCase):
    def test_path_excluded_prefix_and_glob(self):
        self.assertTrue(cs.path_excluded("benches/x.rs", ["benches"]))
        self.assertTrue(cs.path_excluded("benches/x.rs", ["benches/"]))
        self.assertFalse(cs.path_excluded("benchesx/x.rs", ["benches"]))
        self.assertTrue(cs.path_excluded("a/b/gen.rs", ["a/*/gen.rs"]))
        self.assertTrue(cs.path_excluded("crates/foo/tests/x.rs", ["crates/*"]))

    def test_looks_like_test(self):
        self.assertTrue(cs.looks_like_test("tests/foo.rs"))
        self.assertTrue(cs.looks_like_test("pkg/test_thing.py"))
        self.assertTrue(cs.looks_like_test("web/a.spec.ts"))
        self.assertFalse(cs.looks_like_test("src/testing_utils_impl.rs"))

    def test_looks_like_generated(self):
        self.assertTrue(cs.looks_like_generated("node_modules/x/index.js"))
        self.assertTrue(cs.looks_like_generated("src/thing_pb2.py"))
        self.assertFalse(cs.looks_like_generated("src/builder.rs"))


class TestScorecardShape(unittest.TestCase):
    def test_commit_mismatch_refuses_without_scoring(self):
        golden = {"schema_version": 1, "repo": "demo", "language": "rust",
                  "commit": "a" * 40, "symbols": []}
        card = cs.score_golden(golden, "/nonexistent/binary", os.path.dirname(__file__),
                               allow_commit_mismatch=False)
        # This repo's HEAD is not 'aaaa...', so the scorer must refuse.
        if card.get("src_commit"):
            self.assertEqual(card.get("refused"), "commit_mismatch")
            self.assertNotIn("totals", card)

    def test_gate_field_is_explicitly_null(self):
        golden = {"schema_version": 1, "repo": "demo", "language": "rust",
                  "commit": "a" * 40, "symbols": []}
        card = cs.score_golden(golden, "/nonexistent/binary", os.path.dirname(__file__))
        self.assertEqual(card["schema"], "loregrep-eval-corpus/1")
        self.assertIsNone(card["gate"])


class TestGoldenFlagExclusion(unittest.TestCase):
    """A macro-expanding oracle reports definitions that exist in no source file.
    Those must be bucketed, not counted as recall misses."""

    def _golden(self, flags):
        return [
            {"name": "real_fn", "kind": "function", "file": "a.rs",
             "start_line": 10, "end_line": 12, "owner": None, "flags": []},
            {"name": "macro_fn", "kind": "function", "file": "a.rs",
             "start_line": 20, "end_line": 20, "owner": None, "flags": flags},
        ]

    def _emitted(self):
        return [{"name": "real_fn", "kind": "function", "file": "a.rs",
                 "start_line": 10, "end_line": 12, "owner": None, "flags": []}]

    def test_generated_golden_symbol_is_bucketed_not_a_miss(self):
        card = cs.score_symbols(
            self._golden(["generated"]), self._emitted(), "rust",
            policy={"include_generated": False})
        self.assertEqual(card["totals"]["recall"], 1.0)
        self.assertEqual(card["excluded"]["golden_generated"]["count"], 1)
        self.assertEqual(card["totals"]["missing"], 0)

    def test_generated_counts_as_a_miss_when_policy_includes_it(self):
        card = cs.score_symbols(
            self._golden(["generated"]), self._emitted(), "rust",
            policy={"include_generated": True})
        self.assertEqual(card["totals"]["missing"], 1)
        self.assertNotIn("golden_generated", card["excluded"])

    def test_test_flagged_golden_symbol_is_bucketed_when_excluded(self):
        card = cs.score_symbols(
            self._golden(["test"]), self._emitted(), "rust",
            policy={"include_test_symbols": False})
        self.assertEqual(card["totals"]["recall"], 1.0)
        self.assertEqual(card["excluded"]["golden_test"]["count"], 1)


class TestWaivers(unittest.TestCase):
    """Waivers are triage, not amnesty: counted by class, and stale ones surface."""

    def _card(self):
        golden = [{"name": "kept", "kind": "function", "file": "a.rs",
                   "start_line": 1, "end_line": 2, "owner": None, "flags": []}]
        emitted = [
            {"name": "kept", "kind": "function", "file": "a.rs",
             "start_line": 1, "end_line": 2, "owner": None},
            {"name": "waived_one", "kind": "function", "file": "a.rs",
             "start_line": 10, "end_line": 11, "owner": None},
            {"name": "real_fp", "kind": "function", "file": "a.rs",
             "start_line": 20, "end_line": 21, "owner": None},
        ]
        return cs.score_symbols(golden, emitted, "rust")

    def test_waived_fp_is_bucketed_by_class_and_leaves_others_alone(self):
        card = self._card()
        self.assertEqual(card["totals"]["false_positives_n"], 2)
        waivers = {("a.rs:10", "waived_one"):
                   {"at": "a.rs:10", "name": "waived_one", "class": "oracle-artifact-x"}}
        card = cs.apply_waivers(card, waivers)
        self.assertEqual(card["totals"]["false_positives_n"], 1)
        self.assertEqual(card["totals"]["waived"], 1)
        self.assertEqual(card["excluded"]["waived:oracle-artifact-x"]["count"], 1)
        # the un-waived false positive must survive untouched
        remaining = [fp["name"] for b in card["per_kind"].values()
                     for fp in b["false_positives"]]
        self.assertEqual(remaining, ["real_fp"])

    def test_waiver_matching_nothing_is_reported_stale(self):
        card = self._card()
        waivers = {("a.rs:999", "ghost"):
                   {"at": "a.rs:999", "name": "ghost", "class": "oracle-artifact-x"}}
        card = cs.apply_waivers(card, waivers)
        self.assertEqual([s["name"] for s in card["stale_waivers"]], ["ghost"])

    def test_no_waivers_leaves_the_card_unchanged(self):
        card = self._card()
        before = card["totals"]["false_positives_n"]
        card = cs.apply_waivers(card, {})
        self.assertEqual(card["totals"]["false_positives_n"], before)
        self.assertNotIn("stale_waivers", card)


class TestOutsideOracleScope(unittest.TestCase):
    """An indexer may cover a strict subset of the tree; files it never indexed
    are unmeasurable, not false positives."""

    def test_symbol_in_unindexed_file_is_bucketed(self):
        g = norm_golden([golden_sym("alpha", "function", "src/a.ts", 1, 2)])
        e = emit([fn_item("alpha", p("src/a.ts"), 1, 2),
                  fn_item("helper", p("src/b.test.ts"), 5, 6)])
        card = cs.score_symbols(g, e, "typescript")
        self.assertEqual(card["totals"]["false_positives_n"], 0)
        self.assertEqual(card["excluded"]["outside_oracle_scope"]["count"], 1)
        self.assertEqual(card["excluded"]["outside_oracle_scope"]["distinct_files"], 1)

    def test_guard_does_not_swallow_a_wrong_file_finding(self):
        # The whole point of wrong_file is that loregrep put a real symbol
        # somewhere the oracle did not; the guard must run after matching.
        g = norm_golden([golden_sym("alpha", "function", "src/a.ts", 1, 2)])
        e = emit([fn_item("alpha", p("src/elsewhere.ts"), 1, 2)])
        card = cs.score_symbols(g, e, "typescript")
        self.assertEqual(card["totals"]["wrong_file"], 1)
        self.assertNotIn("outside_oracle_scope", card["excluded"])


if __name__ == "__main__":
    unittest.main()
