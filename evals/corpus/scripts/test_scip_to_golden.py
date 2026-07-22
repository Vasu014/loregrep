#!/usr/bin/env python3
"""Tests for scip_to_golden.py.

    python3 evals/corpus/scripts/test_scip_to_golden.py

Stdlib unittest only, and self-contained: both SCIP fixtures below are written by
hand so the suite never depends on a multi-megabyte sample index.

`fixture_index()` mirrors the shapes rust-analyzer 1.97.1 / scip v0.9.0 actually
emit (both range encodings, `impl#[T][Trait]` descriptors, backtick-escaped
generic impl targets, `local N` symbols, per-target `crate/` pseudo-symbols).

`python_fixture_index()` mirrors scip-python 0.6.6 (no `kind` and no
`display_name` on SymbolInformation, `<module.path>/__init__:` meta
pseudo-symbols, `f().(param)` descriptors, and `enclosing_range`s that open on a
decorator line rather than on the `def`).
"""

import json
import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import scip_to_golden as s2g  # noqa: E402

PKG = "rust-analyzer cargo demo 0.1.0 "
COMMIT = "0" * 40


def occ(symbol, rng, enclosing=None, definition=True):
    o = {"symbol": symbol, "range": list(rng), "symbol_roles": 1 if definition else 0}
    if enclosing is not None:
        o["enclosing_range"] = list(enclosing)
    return o


def sym(symbol, kind=None, display_name=None, signature=None):
    s = {"symbol": symbol}
    if kind is not None:
        s["kind"] = kind
    if display_name is not None:
        s["display_name"] = display_name
    if signature is not None:
        s["signature_documentation"] = {"language": "rust", "text": signature}
    return s


def fixture_index():
    """A two-document synthetic index. Line numbers are 0-indexed as in SCIP."""
    lib = {
        "language": "rust",
        "relative_path": "src/lib.rs",
        "occurrences": [
            # crate root pseudo-symbol: whole file, no display name -> dropped
            occ(PKG + "crate/", [0, 0, 120, 0], [0, 0, 120, 0]),
            # free function
            occ(PKG + "helper().", [4, 7, 13], [3, 0, 8, 1]),
            # struct
            occ(PKG + "Widget#", [10, 11, 17], [9, 0, 13, 1]),
            # struct field -> dropped (no neutral kind)
            occ(PKG + "Widget#size.", [11, 4, 8]),
            # enum + variant (variant dropped)
            occ(PKG + "Colour#", [15, 9, 15], [15, 0, 18, 1]),
            occ(PKG + "Colour#Red#", [16, 4, 7]),
            # trait declaration + its member (visibility inherited -> unknown)
            occ(PKG + "Draw#", [20, 10, 14], [20, 0, 22, 1]),
            occ(PKG + "Draw#draw().", [21, 7, 11], [21, 4, 21]),
            # inherent impl method, async
            occ(PKG + "impl#[Widget]render().", [26, 13, 19], [25, 4, 29, 5]),
            # trait impl method -> owner is the implementing type, not the trait
            occ(PKG + "impl#[Widget][Draw]draw().", [33, 7, 11], [33, 4, 35, 5]),
            # impl on a generic type: backtick-escaped descriptor
            occ(PKG + "impl#[`Holder<T>`]get().", [40, 11, 14], [40, 4, 42, 5]),
            # type alias, single-line range encoding for the enclosing range too
            occ(PKG + "Res#", [45, 9, 12], [45, 0, 40]),
            # constant -> dropped
            occ(PKG + "MAX.", [47, 10, 13], [47, 0, 24]),
            # local -> dropped
            occ("local 0", [5, 8, 11], [5, 8, 11]),
            # a reference, not a definition -> ignored
            occ(PKG + "helper().", [50, 4, 10], definition=False),
            # external package definition (macro import) -> dropped
            occ("rust-analyzer cargo serde 1.0.0 de/Deserialize#", [55, 0, 9], [55, 0, 9]),
            # two distinct symbols sharing one range: macro expansion -> dropped
            occ(PKG + "GenA#", [60, 0, 4], [60, 0, 4]),
            occ(PKG + "GenB#", [60, 0, 4], [60, 0, 4]),
        ],
        "symbols": [
            sym(PKG + "crate/", kind=29),
            sym(PKG + "helper().", kind=17, display_name="helper", signature="fn helper() -> u32"),
            sym(PKG + "Widget#", kind=49, display_name="Widget", signature="pub struct Widget"),
            sym(PKG + "Widget#size.", kind=15, display_name="size", signature="size: u32"),
            sym(PKG + "Colour#", kind=11, display_name="Colour", signature="pub(crate) enum Colour"),
            sym(PKG + "Colour#Red#", kind=12, display_name="Red", signature="Red"),
            sym(PKG + "Draw#", kind=53, display_name="Draw", signature="pub trait Draw"),
            sym(PKG + "Draw#draw().", kind=70, display_name="draw", signature="fn draw(&self)"),
            sym(
                PKG + "impl#[Widget]render().",
                kind=26,
                display_name="render",
                signature="pub async fn render(&self) -> String",
            ),
            sym(PKG + "impl#[Widget][Draw]draw().", kind=26, display_name="draw", signature="fn draw(&self)"),
            sym(PKG + "impl#[`Holder<T>`]get().", kind=26, display_name="get", signature="pub fn get(&self) -> &T"),
            sym(PKG + "Res#", kind=55, display_name="Res", signature="pub type Res = Result<(), Error>"),
            sym(PKG + "MAX.", kind=8, display_name="MAX", signature="const MAX: usize"),
            sym("local 0", kind=61, display_name="x", signature="let x: u32"),
            sym("rust-analyzer cargo serde 1.0.0 de/Deserialize#", kind=53, display_name="Deserialize"),
            sym(PKG + "GenA#", kind=49, display_name="GenA", signature="struct GenA"),
            sym(PKG + "GenB#", kind=49, display_name="GenB", signature="struct GenB"),
        ],
    }
    util = {
        "language": "rust",
        "relative_path": "src/util/mod.rs",
        "occurrences": [
            # file module, whole-file range with an exclusive end column of 0
            occ(PKG + "util/", [0, 0, 30, 0], [0, 0, 30, 0]),
            occ(PKG + "util/slugify().", [2, 3, 10], [1, 0, 5, 1]),
            # conventional test module and a function inside it
            occ(PKG + "util/tests/", [10, 4, 9], [9, 0, 20, 1]),
            occ(PKG + "util/tests/it_works().", [13, 7, 15], [12, 4, 15, 5]),
            # symbol with no SymbolInformation entry: name must fall back to the
            # trailing descriptor identifier
            occ(PKG + "util/orphan().", [22, 3, 9], [22, 0, 24, 1]),
        ],
        "symbols": [
            sym(PKG + "util/", kind=29, display_name="util", signature="pub mod util"),
            sym(PKG + "util/slugify().", kind=17, display_name="slugify", signature="pub fn slugify(s: &str) -> String"),
            sym(PKG + "util/tests/", kind=29, display_name="tests", signature="mod tests"),
            sym(PKG + "util/tests/it_works().", kind=17, display_name="it_works", signature="fn it_works()"),
        ],
    }
    return {
        "metadata": {
            "tool_info": {"name": "rust-analyzer", "version": "1.97.1"},
            "project_root": "file:///tmp/demo",
        },
        "documents": [lib, util],
    }


class Args(object):
    def __init__(self, **kw):
        self.repo = "demo"
        self.commit = COMMIT
        self.language = "rust"
        self.exclude = None
        self.package = None
        self.repo_root = None
        self.indexer = None
        self.indexer_version = None
        self.scip_cli_version = "v0.9.0"
        # Tests that assert on module symbols opt out of the default exclusion.
        self.excluded_kinds = s2g.EXCLUDED_KINDS
        self.__dict__.update(kw)


def convert_fixture(**kw):
    golden, stats = s2g.build_golden(fixture_index(), Args(**kw))
    return golden, stats


def by_name(golden, name, file=None):
    hits = [s for s in golden["symbols"] if s["name"] == name and (file is None or s["file"] == file)]
    return hits


# --------------------------------------------------------------------------


class TestRanges(unittest.TestCase):
    def test_single_line_encoding(self):
        self.assertEqual(s2g.range_to_lines([4, 7, 13]), (5, 5))

    def test_multi_line_encoding(self):
        self.assertEqual(s2g.range_to_lines([3, 0, 8, 1]), (4, 9))

    def test_exclusive_end_column_zero_backs_up_a_line(self):
        # [0,0,30,0] covers lines 1..30 inclusive, not 1..31.
        self.assertEqual(s2g.range_to_lines([0, 0, 30, 0]), (1, 30))

    def test_single_line_multiline_encoding_is_not_backed_up(self):
        self.assertEqual(s2g.range_to_lines([7, 0, 7, 0]), (8, 8))

    def test_bad_arity_rejected(self):
        self.assertRaises(s2g.ConversionError, s2g.range_to_lines, [1, 2])


class TestDescriptors(unittest.TestCase):
    def test_module_path_and_function(self):
        d = s2g.parse_descriptors("storage/graph/resolve_rust/resolve_rust_import().")
        self.assertEqual([(x.name, x.kind) for x in d],
                         [("storage", "namespace"), ("graph", "namespace"),
                          ("resolve_rust", "namespace"), ("resolve_rust_import", "method")])

    def test_impl_descriptor(self):
        d = s2g.parse_descriptors("loregrep/impl#[LoreGrepBuilder]with_rust_analyzer().")
        self.assertEqual(s2g.descriptor_owner(d), "LoreGrepBuilder")

    def test_trait_impl_owner_is_the_type_not_the_trait(self):
        d = s2g.parse_descriptors("impl#[Widget][Draw]draw().")
        self.assertEqual(s2g.descriptor_owner(d), "Widget")
        self.assertTrue(s2g.visibility_is_inherited(d))

    def test_trait_declaration_member(self):
        d = s2g.parse_descriptors("storage/persistence/PersistentRepoMap#save_to_disk().")
        self.assertEqual(s2g.descriptor_owner(d), "PersistentRepoMap")
        self.assertTrue(s2g.visibility_is_inherited(d))

    def test_inherent_impl_visibility_is_written_not_inherited(self):
        d = s2g.parse_descriptors("impl#[Widget]render().")
        self.assertFalse(s2g.visibility_is_inherited(d))

    def test_backticked_generic_impl_target(self):
        d = s2g.parse_descriptors("graph/impl#[`FileSet<'a>`]index_of().")
        self.assertEqual(s2g.descriptor_owner(d), "FileSet")

    def test_free_function_has_no_owner(self):
        self.assertIsNone(s2g.descriptor_owner(s2g.parse_descriptors("util/slugify().")))

    def test_enum_variant(self):
        d = s2g.parse_descriptors("core/errors/LoreGrepError#NotScanned#")
        self.assertEqual([(x.name, x.kind) for x in d][-2:], [("LoreGrepError", "type"), ("NotScanned", "type")])

    def test_crate_root_detected(self):
        p = s2g.ParsedSymbol(PKG + "crate/")
        self.assertTrue(s2g.is_crate_root(p))
        self.assertFalse(s2g.is_crate_root(s2g.ParsedSymbol(PKG + "util/")))


class TestSignatureParsing(unittest.TestCase):
    def test_keyword_after_modifiers(self):
        cases = {
            "pub async fn go()": "fn",
            "pub(crate) unsafe fn go()": "fn",
            "const fn go()": "fn",
            "pub const MAX: usize": "const",
            'pub extern "C" fn go()': "fn",
            "pub struct S": "struct",
            "pub(super) enum E": "enum",
            "type Res = ()": "type",
            "mod tests": "mod",
        }
        for sig, want in cases.items():
            self.assertEqual(s2g.rust_signature_keyword(sig), want, sig)

    def test_visibility(self):
        d = s2g.ParsedSymbol(PKG + "helper().")
        self.assertEqual(s2g.rust_visibility("pub fn helper()", d, "function"), "public")
        self.assertEqual(s2g.rust_visibility("fn helper()", d, "function"), "private")
        # restricted visibility is not modelled by the schema: abstain
        self.assertEqual(s2g.rust_visibility("pub(crate) fn helper()", d, "function"), "unknown")
        self.assertEqual(s2g.rust_visibility("", d, "function"), "unknown")


class TestConversion(unittest.TestCase):
    def setUp(self):
        self.golden, self.stats = convert_fixture()
        self.syms = self.golden["symbols"]

    def test_only_definitions_are_emitted(self):
        # `helper` is defined once and referenced once.
        self.assertEqual(len(by_name(self.golden, "helper")), 1)

    def test_names_are_bare_identifiers(self):
        for s in self.syms:
            self.assertNotIn(" ", s["name"], s)
            self.assertNotIn("#", s["name"], s)
            self.assertNotIn("/", s["name"], s)

    def test_name_falls_back_to_trailing_descriptor(self):
        self.assertEqual(len(by_name(self.golden, "orphan")), 1)

    def test_free_function(self):
        f = by_name(self.golden, "helper")[0]
        self.assertEqual((f["kind"], f["owner"], f["start_line"], f["end_line"]), ("function", None, 5, 9))
        self.assertEqual(f["visibility"], "private")

    def test_method_owner_and_async_flag(self):
        m = by_name(self.golden, "render")[0]
        self.assertEqual((m["kind"], m["owner"], m["visibility"]), ("method", "Widget", "public"))
        self.assertEqual(m["flags"], ["async"])
        self.assertEqual((m["start_line"], m["end_line"]), (27, 30))

    def test_trait_impl_method_visibility_is_unknown(self):
        m = [x for x in by_name(self.golden, "draw") if x["owner"] == "Widget"][0]
        self.assertEqual(m["visibility"], "unknown")

    def test_trait_declaration_member_visibility_is_unknown(self):
        m = [x for x in by_name(self.golden, "draw") if x["owner"] == "Draw"][0]
        self.assertEqual(m["visibility"], "unknown")
        self.assertEqual((m["start_line"], m["end_line"]), (22, 22))

    def test_generic_impl_owner_is_normalized(self):
        self.assertEqual(by_name(self.golden, "get")[0]["owner"], "Holder")

    def test_kinds(self):
        self.golden, self.stats = convert_fixture(excluded_kinds=())
        want = {
            "Widget": "struct",
            "Colour": "enum",
            "Draw": "trait",
            "Res": "type_alias",
            "util": "namespace",
        }
        for name, kind in want.items():
            self.assertEqual(by_name(self.golden, name)[0]["kind"], kind, name)

    def test_restricted_visibility_abstains(self):
        self.assertEqual(by_name(self.golden, "Colour")[0]["visibility"], "unknown")

    def test_file_module_span_covers_the_whole_file(self):
        self.golden, self.stats = convert_fixture(excluded_kinds=())
        m = by_name(self.golden, "util")[0]
        self.assertEqual((m["start_line"], m["end_line"]), (1, 30))

    def test_test_flag_propagates_into_a_test_module(self):
        self.golden, self.stats = convert_fixture(excluded_kinds=())
        self.assertEqual(by_name(self.golden, "tests")[0]["flags"], ["test"])
        self.assertEqual(by_name(self.golden, "it_works")[0]["flags"], ["test"])
        self.assertNotIn("test", by_name(self.golden, "slugify")[0]["flags"])

    def test_dropped_categories(self):
        dropped = {s["name"] for s in self.syms}
        for gone in ("size", "Red", "MAX", "x", "Deserialize", "GenA", "GenB", "crate"):
            self.assertNotIn(gone, dropped, gone)
        self.assertEqual(self.stats.dropped["macro_generated"], 2)
        self.assertEqual(self.stats.dropped["external_package"], 1)
        self.assertEqual(self.stats.dropped["crate_root"], 1)
        self.assertEqual(self.stats.dropped["local"], 1)

    def test_external_package_never_enters_the_golden(self):
        self.assertEqual(set(self.stats.packages_kept), {"demo"})

    def test_paths_are_repo_relative_posix(self):
        for s in self.syms:
            self.assertFalse(s["file"].startswith("./"))
            self.assertFalse(s["file"].startswith("/"))

    def test_deterministic_sort(self):
        keys = [(s["file"], s["start_line"], s["name"]) for s in self.syms]
        self.assertEqual(keys, sorted(keys))
        again, _ = convert_fixture()
        self.assertEqual(json.dumps(again["symbols"]), json.dumps(self.syms))

    def test_excluded_paths_are_honoured(self):
        golden, _ = convert_fixture(exclude=["src/util"])
        self.assertEqual([s for s in golden["symbols"] if s["file"].startswith("src/util")], [])
        self.assertEqual(golden["policy"]["excluded_paths"], ["src/util"])

    def test_explicit_package_allowlist(self):
        golden, _ = convert_fixture(package=["nobody"])
        self.assertEqual(golden["symbols"], [])

    def test_generator_provenance(self):
        gen = self.golden["generator"]
        self.assertEqual(gen["indexer"], "rust-analyzer")
        self.assertEqual(gen["indexer_version"], "1.97.1")
        self.assertEqual(gen["scip_cli_version"], "v0.9.0")
        self.assertEqual(gen["converter_version"], s2g.CONVERTER_VERSION)

    def test_unimplemented_language_is_refused(self):
        # rust, python and typescript are implemented; anything else must refuse
        # rather than reinterpret one language's descriptor grammar as another's.
        self.assertRaises(s2g.ConversionError, convert_fixture, language="javascript")
        self.assertRaises(s2g.ConversionError, convert_fixture, language="go")


class TestCfgTestDetection(unittest.TestCase):
    def test_cfg_test_module_with_an_unconventional_name(self):
        with tempfile.TemporaryDirectory() as root:
            os.makedirs(os.path.join(root, "src"))
            with open(os.path.join(root, "src", "lib.rs"), "w") as fh:
                fh.write("fn a() {}\n\n#[cfg(test)]\nmod p1_tests {\n    fn helper() {}\n}\n")
            index = {
                "metadata": {"tool_info": {"name": "rust-analyzer", "version": "1.97.1"}},
                "documents": [{
                    "relative_path": "src/lib.rs",
                    "occurrences": [
                        occ(PKG + "p1_tests/", [3, 4, 12], [2, 0, 5, 1]),
                        occ(PKG + "p1_tests/helper().", [4, 7, 13], [4, 4, 17]),
                    ],
                    "symbols": [
                        sym(PKG + "p1_tests/", kind=29, display_name="p1_tests", signature="mod p1_tests"),
                        sym(PKG + "p1_tests/helper().", kind=17, display_name="helper", signature="fn helper()"),
                    ],
                }],
            }
            # Opt out of the default namespace exclusion so the module symbol
            # itself is observable here; this test is about cfg(test) detection.
            golden, _ = s2g.build_golden(index, Args(repo_root=root, excluded_kinds=()))
            self.assertEqual({s["name"]: s["flags"] for s in golden["symbols"]},
                             {"p1_tests": ["test"], "helper": ["test"]})
            # Without --repo-root the unconventional name is simply not detected.
            plain, _ = s2g.build_golden(index, Args(excluded_kinds=()))
            self.assertEqual({s["name"]: s["flags"] for s in plain["symbols"]},
                             {"p1_tests": [], "helper": []})


# --------------------------------------------------------------------------
# Python (scip-python)
# --------------------------------------------------------------------------
#
# Same principle as the Rust fixture above: hand-written, so the suite never
# depends on the 2 MB real flask index. Every shape below was read off
# `scip print --json` of scip-python 0.6.6 on flask 3.1.3:
#   * NO `kind` and NO `display_name` on SymbolInformation — only
#     {symbol, documentation, relationships}
#   * `range` 3 ints (the identifier is always on one line),
#     `enclosing_range` 4 ints, opening at the first DECORATOR line
#   * one `<module.path>/__init__:` meta pseudo-symbol per file, range [0,0,0]
#   * parameters as `f().(name)` descriptors, attributes as `T#a.`
#   * externals as ordinary symbols under a different package

PY = "scip-python python demo 1.2.3 "


def pysym(symbol, doc=None):
    s = {"symbol": symbol}
    if doc is not None:
        s["documentation"] = [doc]
    return s


def fence(text):
    return "```python\n%s\n```" % text


def python_fixture_index():
    mod = {
        "relative_path": "src/demo/mod.py",
        "occurrences": [
            # the module pseudo-symbol: no enclosing_range, range [0,0,0]
            occ(PY + "`demo.mod`/__init__:", [0, 0, 0]),
            # module-level binding -> no neutral kind
            occ(PY + "`demo.mod`/VERSION.", [2, 0, 7]),
            # module-level function, multi-line rendered signature
            occ(PY + "`demo.mod`/_helper().", [5, 4, 11], [5, 0, 7, 20]),
            occ(PY + "`demo.mod`/_helper().(value)", [5, 12, 17]),
            # class + attribute + methods
            occ(PY + "`demo.mod`/Widget#", [10, 6, 12], [10, 0, 30, 1]),
            occ(PY + "`demo.mod`/Widget#size.", [11, 4, 8]),
            occ(PY + "`demo.mod`/Widget#__init__().", [13, 8, 16], [13, 4, 16, 12]),
            # decorated: enclosing_range opens on the @property line, the name
            # occurrence is on the `def` line. The golden must use the latter.
            occ(PY + "`demo.mod`/Widget#_render().", [19, 8, 16], [18, 4, 21, 20]),
            occ(PY + "`demo.mod`/Widget#fetch().", [23, 14, 19], [22, 4, 25, 20]),
            # nested function / nested class / method of a nested class
            occ(PY + "`demo.mod`/outer().", [33, 4, 9], [33, 0, 45, 1]),
            occ(PY + "`demo.mod`/outer().inner().", [35, 8, 13], [35, 4, 37, 1]),
            occ(PY + "`demo.mod`/outer().Inner#", [39, 10, 15], [39, 4, 42, 1]),
            occ(PY + "`demo.mod`/outer().Inner#run().", [40, 12, 15], [40, 8, 41, 1]),
            # a `type` descriptor whose rendered doc claims a `def`: kept as a
            # class (the descriptor is primary) but counted as a disagreement
            occ(PY + "`demo.mod`/Puzzle#", [47, 6, 12], [47, 0, 48, 1]),
            # a class with no SymbolInformation at all: descriptor still decides
            occ(PY + "`demo.mod`/Bare#", [50, 6, 10], [50, 0, 51, 1]),
            occ("local 3", [52, 4, 5]),
            # externals: ordinary symbols under a different package
            occ("scip-python python Werkzeug 3.1.8 `werkzeug.wsgi`/ClosingIterator#",
                [55, 0, 15], [55, 0, 56, 1]),
            occ("scip-python python python-stdlib 3.11 os/__init__:", [57, 0, 0]),
            # a reference, and one spelled with the unstable package casing
            occ("scip-python python Demo 1.2.3 `demo.mod`/_helper().", [60, 4, 11],
                definition=False),
        ],
        "symbols": [
            pysym(PY + "`demo.mod`/__init__:", "(module) demo.mod"),
            pysym(PY + "`demo.mod`/VERSION.", fence("(variable) VERSION: str")),
            pysym(PY + "`demo.mod`/_helper().",
                  fence("def _helper(\n  value: int\n) -> int:")),
            pysym(PY + "`demo.mod`/_helper().(value)", "value: the thing"),
            pysym(PY + "`demo.mod`/Widget#", fence("class Widget(Base):")),
            pysym(PY + "`demo.mod`/Widget#size.", fence("(variable) size: int")),
            pysym(PY + "`demo.mod`/Widget#__init__().", fence("def __init__(\n  self\n) -> None:")),
            pysym(PY + "`demo.mod`/Widget#_render().", fence("@property\ndef _render(\n  self\n) -> str:")),
            pysym(PY + "`demo.mod`/Widget#fetch().", fence("@staticmethod\nasync def fetch() -> str:")),
            pysym(PY + "`demo.mod`/outer().", fence("def outer() -> None:")),
            pysym(PY + "`demo.mod`/outer().inner().", fence("def inner() -> None:")),
            pysym(PY + "`demo.mod`/outer().Inner#", fence("class Inner:")),
            pysym(PY + "`demo.mod`/outer().Inner#run().", fence("def run(\n  self\n) -> None:")),
            pysym(PY + "`demo.mod`/Puzzle#", fence("def puzzle() -> None:")),
            pysym("local 3", fence("(variable) tmp: int")),
            pysym("scip-python python Werkzeug 3.1.8 `werkzeug.wsgi`/ClosingIterator#",
                  fence("class ClosingIterator:")),
            pysym("scip-python python python-stdlib 3.11 os/__init__:", "(module) os"),
        ],
    }
    tests = {
        "relative_path": "tests/test_mod.py",
        "occurrences": [
            occ(PY + "`tests.test_mod`/__init__:", [0, 0, 0]),
            occ(PY + "`tests.test_mod`/test_thing().", [3, 4, 14], [3, 0, 5, 1]),
            occ(PY + "`tests.test_mod`/Helper#", [8, 6, 12], [8, 0, 12, 1]),
            occ(PY + "`tests.test_mod`/Helper#check().", [9, 8, 13], [9, 4, 11, 1]),
        ],
        "symbols": [
            pysym(PY + "`tests.test_mod`/__init__:", "(module) tests.test_mod"),
            pysym(PY + "`tests.test_mod`/test_thing().", fence("def test_thing() -> None:")),
            pysym(PY + "`tests.test_mod`/Helper#", fence("class Helper:")),
            pysym(PY + "`tests.test_mod`/Helper#check().", fence("def check(\n  self\n) -> None:")),
        ],
    }
    docs = {
        "relative_path": "docs/conf.py",
        "occurrences": [
            occ(PY + "`docs.conf`/setup().", [3, 4, 9], [3, 0, 4, 1]),
        ],
        "symbols": [pysym(PY + "`docs.conf`/setup().", fence("def setup() -> None:"))],
    }
    return {
        "metadata": {"tool_info": {"name": "scip-python", "version": "0.6.6"},
                     "project_root": "file:///tmp/demo"},
        "documents": [mod, tests, docs],
    }


def convert_python_fixture(**kw):
    kw.setdefault("language", "python")
    return s2g.build_golden(python_fixture_index(), Args(**kw))


class TestPythonDescriptors(unittest.TestCase):
    def test_module_pseudo_symbol(self):
        p = s2g.ParsedSymbol(PY + "`demo.mod`/__init__:")
        self.assertTrue(s2g.python_is_module(p))
        self.assertEqual(s2g.python_module_path(p), "demo.mod")
        # the identifier a reader means is the last dotted segment, not __init__
        self.assertEqual(s2g.python_name(p), "mod")
        self.assertFalse(s2g.python_is_module(s2g.ParsedSymbol(PY + "`demo.mod`/outer().")))

    def test_backticked_dotted_module_path_is_one_descriptor(self):
        d = s2g.parse_descriptors("`flask.json.tag`/TagDict#loads().")
        self.assertEqual([(x.name, x.kind) for x in d],
                         [("flask.json.tag", "namespace"), ("TagDict", "type"),
                          ("loads", "method")])

    def test_owner_is_the_immediately_enclosing_class(self):
        self.assertEqual(
            s2g.python_owner(s2g.ParsedSymbol(PY + "`demo.mod`/Widget#__init__().")), "Widget")
        # a nested class's method belongs to the nested class
        self.assertEqual(
            s2g.python_owner(s2g.ParsedSymbol(PY + "`demo.mod`/outer().Inner#run().")), "Inner")
        # a closure inside a method is NOT a method of the enclosing class
        self.assertIsNone(
            s2g.python_owner(s2g.ParsedSymbol(PY + "`demo.mod`/Widget#go().poll().")))
        self.assertIsNone(
            s2g.python_owner(s2g.ParsedSymbol(PY + "`demo.mod`/outer().")))

    def test_nested_detection(self):
        self.assertTrue(s2g.python_is_nested(s2g.ParsedSymbol(PY + "`demo.mod`/outer().inner().")))
        self.assertTrue(s2g.python_is_nested(s2g.ParsedSymbol(PY + "`demo.mod`/outer().Inner#")))
        self.assertFalse(s2g.python_is_nested(s2g.ParsedSymbol(PY + "`demo.mod`/Widget#__init__().")))

    def test_parameter_descriptor(self):
        p = s2g.ParsedSymbol(PY + "`demo.mod`/_helper().(value)")
        self.assertEqual(p.descriptors[-1].kind, "parameter")


class TestPythonDocSignal(unittest.TestCase):
    def test_decorators_are_skipped(self):
        head = s2g.python_doc_head({"documentation": [fence("@property\ndef _render(\n  self\n) -> str:")]})
        self.assertEqual(head, "def _render(")
        self.assertEqual(s2g.python_doc_kind(head), "function")

    def test_async(self):
        head = s2g.python_doc_head({"documentation": [fence("@staticmethod\nasync def fetch() -> str:")]})
        self.assertEqual(s2g.python_doc_kind(head), "function")
        self.assertTrue(head.startswith("async def "))

    def test_class_and_module_and_bindings(self):
        self.assertEqual(s2g.python_doc_kind(s2g.python_doc_head(
            {"documentation": [fence("class Widget(Base):")]})), "class")
        self.assertEqual(s2g.python_doc_kind(s2g.python_doc_head(
            {"documentation": ["(module) demo.mod"]})), "namespace")
        for binding in ("(variable) size: int", "(type alias) X: Type[int]",
                        'undefined = t.TypeVar("T")', "werkzeug.datastructures.ImmutableDict"):
            self.assertIsNone(s2g.python_doc_kind(s2g.python_doc_head(
                {"documentation": [fence(binding)]})), binding)

    def test_absent_documentation_says_nothing(self):
        self.assertEqual(s2g.python_doc_head({}), "")
        self.assertIsNone(s2g.python_doc_kind(""))

    def test_visibility_convention(self):
        self.assertEqual(s2g.python_visibility("send_static_file"), "public")
        self.assertEqual(s2g.python_visibility("_helper"), "private")
        self.assertEqual(s2g.python_visibility("__slots"), "private")
        # dunders are protocol members, not private
        self.assertEqual(s2g.python_visibility("__init__"), "public")


class TestPythonConversion(unittest.TestCase):
    def setUp(self):
        self.golden, self.stats = convert_python_fixture()
        self.syms = self.golden["symbols"]

    def test_module_level_function(self):
        f = by_name(self.golden, "_helper")[0]
        self.assertEqual((f["kind"], f["owner"], f["start_line"], f["end_line"]),
                         ("function", None, 6, 8))
        self.assertEqual(f["visibility"], "private")
        self.assertEqual(f["flags"], [])

    def test_class(self):
        c = by_name(self.golden, "Widget")[0]
        self.assertEqual((c["kind"], c["owner"], c["start_line"], c["end_line"]),
                         ("class", None, 11, 31))

    def test_method_has_owner(self):
        m = by_name(self.golden, "__init__")[0]
        self.assertEqual((m["kind"], m["owner"], m["start_line"], m["end_line"]),
                         ("method", "Widget", 14, 17))
        self.assertEqual(m["visibility"], "public")

    def test_decorated_declaration_line_is_the_def_not_the_decorator(self):
        # enclosing_range opens at line 19 (`@property`); the golden must say 20.
        m = by_name(self.golden, "_render")[0]
        self.assertEqual((m["start_line"], m["end_line"]), (20, 22))
        self.assertEqual(m["owner"], "Widget")
        self.assertEqual(m["visibility"], "private")

    def test_async_flag_and_decorated_async_declaration_line(self):
        m = by_name(self.golden, "fetch")[0]
        self.assertEqual(m["flags"], ["async"])
        self.assertEqual((m["start_line"], m["end_line"]), (24, 26))

    def test_nested_function_is_included_and_flagged(self):
        f = by_name(self.golden, "inner")[0]
        self.assertEqual((f["kind"], f["owner"]), ("function", None))
        self.assertEqual(f["flags"], ["nested"])
        self.assertEqual((f["start_line"], f["end_line"]), (36, 38))
        self.assertTrue(self.golden["policy"]["include_nested_functions"])

    def test_nested_class_and_its_method(self):
        c = by_name(self.golden, "Inner")[0]
        self.assertEqual((c["kind"], c["owner"], c["flags"]), ("class", None, ["nested"]))
        m = by_name(self.golden, "run")[0]
        self.assertEqual((m["kind"], m["owner"], m["flags"]), ("method", "Inner", ["nested"]))

    def test_modules_are_excluded_but_classifiable(self):
        self.assertEqual([s for s in self.syms if s["kind"] == "namespace"], [])
        self.assertEqual(self.golden["policy"]["excluded_kinds"], ["namespace"])
        opt_in, _ = convert_python_fixture(excluded_kinds=())
        m = by_name(opt_in, "mod")[0]
        self.assertEqual((m["kind"], m["start_line"], m["end_line"]), ("namespace", 1, 1))

    def test_bindings_parameters_and_locals_are_dropped(self):
        names = {s["name"] for s in self.syms}
        for gone in ("VERSION", "size", "value", "ClosingIterator", "os"):
            self.assertNotIn(gone, names, gone)
        self.assertEqual(self.stats.dropped["local"], 1)
        self.assertEqual(self.stats.dropped["kind:parameter"], 1)
        self.assertEqual(self.stats.dropped["kind:term"], 2)
        self.assertEqual(self.stats.dropped["external_package"], 2)

    def test_only_definitions_are_emitted(self):
        # `_helper` is defined once and referenced once (under a different casing).
        self.assertEqual(len(by_name(self.golden, "_helper")), 1)

    def test_first_party_package_inference_ignores_casing_and_stdlib(self):
        # python-stdlib owns a module definition in this index and must still be
        # rejected; Werkzeug owns a class definition but no module.
        self.assertEqual(s2g.infer_local_packages_python(python_fixture_index()), {"demo"})
        golden, _ = convert_python_fixture(package=["DEMO"])
        self.assertTrue(golden["symbols"])
        golden, _ = convert_python_fixture(package=["nobody"])
        self.assertEqual(golden["symbols"], [])

    def test_descriptor_wins_over_documentation_and_disagreement_is_counted(self):
        p = by_name(self.golden, "Puzzle")[0]
        self.assertEqual(p["kind"], "class")
        self.assertEqual(self.stats.disagreements["descriptor:type vs doc:function"], 1)

    def test_class_without_documentation_still_classified(self):
        self.assertEqual(by_name(self.golden, "Bare")[0]["kind"], "class")

    def test_test_flag_from_path(self):
        self.assertEqual(by_name(self.golden, "test_thing")[0]["flags"], ["test"])
        self.assertEqual(by_name(self.golden, "Helper")[0]["flags"], ["test"])
        self.assertEqual(by_name(self.golden, "check")[0]["flags"], ["test"])
        self.assertNotIn("test", by_name(self.golden, "outer")[0]["flags"])
        self.assertTrue(self.golden["policy"]["include_test_symbols"])

    def test_excluded_paths_count_definitions_not_files(self):
        golden, stats = convert_python_fixture(exclude=["docs"])
        self.assertEqual([s for s in golden["symbols"] if s["file"].startswith("docs/")], [])
        self.assertEqual(golden["policy"]["excluded_paths"], ["docs"])
        self.assertEqual(stats.dropped["excluded_path"], 1)
        self.assertEqual(stats.dropped["excluded_path_file"], 1)

    def test_generated_is_never_flagged(self):
        # Python has no macro expansion: every definition corresponds to source
        # text a syntax-level parser can see. See POLICY.md 10.5.
        self.assertEqual([s for s in self.syms if "generated" in s["flags"]], [])

    def test_deterministic_sort_and_paths(self):
        keys = [(s["file"], s["start_line"], s["name"]) for s in self.syms]
        self.assertEqual(keys, sorted(keys))
        again, _ = convert_python_fixture()
        self.assertEqual(json.dumps(again["symbols"]), json.dumps(self.syms))
        for s in self.syms:
            self.assertFalse(s["file"].startswith("./"))
            self.assertFalse(s["file"].startswith("/"))

    def test_generator_provenance(self):
        gen = self.golden["generator"]
        self.assertEqual((gen["indexer"], gen["indexer_version"]), ("scip-python", "0.6.6"))
        self.assertEqual(self.golden["language"], "python")

    def test_schema_conformance(self):
        with open(SCHEMA_PATH) as fh:
            schema = json.load(fh)
        self.assertEqual(validate(self.golden, schema), [])


# --------------------------------------------------------------------------
# TypeScript (scip-typescript 0.4.0)
# --------------------------------------------------------------------------
#
# The fixture mirrors what scip-typescript actually emits, as measured on hono
# v4.12.31 (POLICY.md §11):
#   * no `kind` and no `display_name` on SymbolInformation; only
#     `documentation[0]`, a ```ts fenced block rendering the declaration
#   * a per-FILE module pseudo-symbol `<dir>/`<file.ts>`/`, alongside real
#     `namespace X {}` symbols that look identical apart from the trailing name
#   * `<constructor>` / `<get>x` / `<set>x` angle-bracketed member names
#   * `typeLiteral0:` / `name0:` SCIP meta descriptors for anonymous scopes
#   * module-level arrow functions as plain `x.` term descriptors that carry an
#     `enclosing_range`, while a plain `const` or a re-export alias carries none

TS = "scip-typescript npm demo 1.0.0 "


def tsym(symbol, doc=None):
    s = {"symbol": symbol}
    if doc is not None:
        s["documentation"] = [doc]
    return s


def tsfence(text):
    return "```ts\n%s\n```" % text


def typescript_fixture_index():
    mod = {
        "relative_path": "src/mod.ts",
        "occurrences": [
            # the per-file module pseudo-symbol: NOT a `namespace` declaration
            occ(TS + "src/`mod.ts`/", [0, 0, 0], [0, 0, 62, 0]),
            occ(TS + "src/`mod.ts`/Pattern#", [2, 12, 19], [2, 0, 40]),
            occ(TS + "src/`mod.ts`/Router#", [4, 17, 23], [4, 0, 9, 1]),
            occ(TS + "src/`mod.ts`/Router#[T]", [4, 24, 25]),
            occ(TS + "src/`mod.ts`/Router#add().", [5, 2, 5], [5, 2, 30]),
            occ(TS + "src/`mod.ts`/Router#name.", [6, 2, 6]),
            occ(TS + "src/`mod.ts`/Algo#", [11, 12, 16], [11, 0, 14, 1]),
            occ(TS + "src/`mod.ts`/Algo#HS256.", [12, 2, 7]),
            occ(TS + "src/`mod.ts`/Widget#", [16, 13, 19], [16, 0, 30, 1]),
            occ(TS + "src/`mod.ts`/Widget#`<constructor>`().", [17, 2, 13], [17, 2, 19, 3]),
            occ(TS + "src/`mod.ts`/Widget#`<constructor>`().(size)", [17, 14, 18]),
            occ(TS + "src/`mod.ts`/Widget#render().", [21, 2, 8], [21, 2, 24, 3]),
            # accessors carry no enclosing_range at all (28/28 in hono)
            occ(TS + "src/`mod.ts`/Widget#`<get>size`().", [26, 6, 10]),
            occ(TS + "src/`mod.ts`/Widget#count.", [27, 2, 7]),
            # anonymous type-literal scope + its member
            occ(TS + "src/`mod.ts`/Opts#typeLiteral0:res.", [33, 2, 5]),
            occ(TS + "src/`mod.ts`/name0:", [34, 4, 8]),
            # module-level arrow: inline function type AND an enclosing_range
            occ(TS + "src/`mod.ts`/splitPath.", [36, 13, 22], [36, 25, 40, 1]),
            # module-level arrow annotated with a named alias: the rendered type
            # is silent, the enclosing_range is not
            occ(TS + "src/`mod.ts`/notFoundHandler.", [42, 6, 21], [42, 40, 44, 1]),
            # `export const verify = Other.verify`: a function TYPE, but a
            # re-export binding rather than a definition -> no enclosing_range
            occ(TS + "src/`mod.ts`/verify.", [46, 13, 19]),
            occ(TS + "src/`mod.ts`/METHODS.", [48, 6, 13]),
            occ(TS + "src/`mod.ts`/parseBody().", [50, 15, 24], [50, 0, 55, 1]),
            # a real `export namespace JSX {}` and a type declared inside it
            occ(TS + "src/`mod.ts`/JSX/", [57, 17, 20]),
            occ(TS + "src/`mod.ts`/JSX/Element#", [58, 14, 21], [58, 2, 40]),
            # `declare module '../..' {}`: the "name" is a module specifier
            occ(TS + "src/`mod.ts`/`'../..'`/", [60, 15, 22]),
            # a type-ish descriptor with no documentation at all: unclassifiable
            occ(TS + "src/`mod.ts`/Mystery#", [61, 5, 12], [61, 0, 61, 20]),
            # descriptor says callable, rendered head says type-ish
            occ(TS + "src/`mod.ts`/weird().", [63, 9, 14], [63, 0, 64, 1]),
            occ("local 5", [65, 2, 3]),
            # an npm dependency's symbol: an ordinary symbol, different package
            occ("scip-typescript npm zod 3.0.0 src/`z.ts`/ZodType#", [66, 0, 7],
                [66, 0, 67, 1]),
            # a reference, not a definition
            occ(TS + "src/`mod.ts`/splitPath.", [70, 4, 13], definition=False),
        ],
        "symbols": [
            tsym(TS + "src/`mod.ts`/", tsfence('module "mod.ts"')),
            tsym(TS + "src/`mod.ts`/Pattern#", tsfence("type Pattern")),
            tsym(TS + "src/`mod.ts`/Router#", tsfence("interface Router<T>")),
            tsym(TS + "src/`mod.ts`/Router#[T]", tsfence("T: T")),
            tsym(TS + "src/`mod.ts`/Router#add().",
                 tsfence("(method) add(path: string): void")),
            tsym(TS + "src/`mod.ts`/Router#name.", tsfence("(property) name: string")),
            tsym(TS + "src/`mod.ts`/Algo#", tsfence("enum Algo")),
            tsym(TS + "src/`mod.ts`/Algo#HS256.", tsfence("(enum member) HS256 = HS256")),
            tsym(TS + "src/`mod.ts`/Widget#", tsfence("class Widget")),
            tsym(TS + "src/`mod.ts`/Widget#`<constructor>`().",
                 tsfence("constructor(size: number): Widget")),
            tsym(TS + "src/`mod.ts`/Widget#`<constructor>`().(size)",
                 tsfence("(parameter) size: number")),
            tsym(TS + "src/`mod.ts`/Widget#render().", tsfence("(method) render(): string")),
            tsym(TS + "src/`mod.ts`/Widget#`<get>size`().", tsfence("get size: number")),
            tsym(TS + "src/`mod.ts`/Widget#count.", tsfence("(property) count: number")),
            tsym(TS + "src/`mod.ts`/Opts#typeLiteral0:res.", tsfence("(property) res: string")),
            tsym(TS + "src/`mod.ts`/name0:", tsfence("(property) name: string")),
            tsym(TS + "src/`mod.ts`/splitPath.",
                 tsfence("var splitPath: (path: string) => string[]")),
            tsym(TS + "src/`mod.ts`/notFoundHandler.",
                 tsfence("var notFoundHandler: NotFoundHandler")),
            tsym(TS + "src/`mod.ts`/verify.",
                 tsfence("var verify: (token: string) => boolean")),
            tsym(TS + "src/`mod.ts`/METHODS.",
                 tsfence('var METHODS: readonly ["get", "post"]')),
            tsym(TS + "src/`mod.ts`/parseBody().",
                 tsfence("function parseBody<T extends Body>(r: Request): Promise<T>")),
            tsym(TS + "src/`mod.ts`/JSX/", tsfence("JSX: any")),
            tsym(TS + "src/`mod.ts`/JSX/Element#", tsfence("type Element")),
            tsym(TS + "src/`mod.ts`/`'../..'`/",
                 tsfence("'../..': typeof import(\"/src/index\")")),
            tsym(TS + "src/`mod.ts`/weird().", tsfence("class Weird")),
            tsym("scip-typescript npm zod 3.0.0 src/`z.ts`/ZodType#", tsfence("class ZodType")),
        ],
    }
    tests = {
        "relative_path": "src/mod.test.ts",
        "occurrences": [
            occ(TS + "src/`mod.test.ts`/", [0, 0, 0], [0, 0, 20, 0]),
            occ(TS + "src/`mod.test.ts`/setup().", [3, 9, 14], [3, 0, 5, 1]),
        ],
        "symbols": [
            tsym(TS + "src/`mod.test.ts`/", tsfence('module "mod.test.ts"')),
            tsym(TS + "src/`mod.test.ts`/setup().", tsfence("function setup(): void")),
        ],
    }
    ambient = {
        "relative_path": "src/globals.d.ts",
        "occurrences": [
            occ(TS + "src/`globals.d.ts`/", [0, 0, 0], [0, 0, 9, 0]),
            occ(TS + "src/`globals.d.ts`/Deno/", [0, 18, 22]),
        ],
        "symbols": [
            tsym(TS + "src/`globals.d.ts`/", tsfence('module "globals.d.ts"')),
            tsym(TS + "src/`globals.d.ts`/Deno/", tsfence("Deno: typeof Deno")),
        ],
    }
    bench = {
        "relative_path": "benchmarks/run.ts",
        "occurrences": [
            occ(TS + "benchmarks/`run.ts`/", [0, 0, 0], [0, 0, 9, 0]),
            occ(TS + "benchmarks/`run.ts`/main().", [1, 9, 13], [1, 0, 3, 1]),
        ],
        "symbols": [
            tsym(TS + "benchmarks/`run.ts`/", tsfence('module "run.ts"')),
            tsym(TS + "benchmarks/`run.ts`/main().", tsfence("function main(): void")),
        ],
    }
    return {
        "metadata": {"tool_info": {"name": "scip-typescript", "version": "0.4.0"},
                     "project_root": "file:///tmp/demo"},
        "documents": [mod, tests, ambient, bench],
    }


def convert_ts_fixture(**kw):
    kw.setdefault("language", "typescript")
    # None -> the per-language default (empty for TypeScript), which is what the
    # CLI does; the Args default mirrors the Rust/Python path.
    kw.setdefault("excluded_kinds", None)
    return s2g.build_golden(typescript_fixture_index(), Args(**kw))


class TestTypescriptHelpers(unittest.TestCase):
    def test_doc_head_unwraps_the_ts_fence(self):
        self.assertEqual(s2g.ts_doc_head(tsym("x", tsfence("class Widget"))), "class Widget")
        self.assertEqual(s2g.ts_doc_head(tsym("x", "(module) plain")), "(module) plain")
        self.assertEqual(s2g.ts_doc_head(tsym("x")), "")

    def test_doc_head_kinds(self):
        for head, expected in (
            ("type Pattern", "type_alias"),
            ("interface Router<T>", "interface"),
            ("class Widget", "class"),
            ("abstract class FetchEventLike", "class"),
            ("enum Algo", "enum"),
            ("const enum Algo", "enum"),
            ("function parseBody<T>(r: Request): Promise<T>", "function"),
            ("(method) render(): string", "method"),
            ("constructor(size: number): Widget", "method"),
            ("get size: number", "method"),
            ("set size: number", "method"),
        ):
            self.assertEqual(s2g.ts_doc_kind(head)[0], expected, head)
        for head in ('module "mod.ts"', "var METHODS: string[]",
                     "(property) name: string", "(enum member) HS256 = HS256",
                     "(parameter) size: number"):
            self.assertIsNone(s2g.ts_doc_kind(head)[0], head)
        # says nothing at all -> no class either, so nothing can disagree
        self.assertEqual(s2g.ts_doc_kind(""), (None, None))

    def test_file_module_is_told_apart_from_a_real_namespace(self):
        self.assertTrue(s2g.ts_is_file_module(
            s2g.ParsedSymbol(TS + "src/`mod.ts`/"), "src/mod.ts"))
        self.assertFalse(s2g.ts_is_file_module(
            s2g.ParsedSymbol(TS + "src/`mod.ts`/JSX/"), "src/mod.ts"))
        self.assertFalse(s2g.ts_is_file_module(
            s2g.ParsedSymbol(TS + "src/`mod.ts`/parseBody()."), "src/mod.ts"))

    def test_member_names_are_normalized(self):
        self.assertEqual(s2g.ts_normalize_name("<constructor>"), "constructor")
        self.assertEqual(s2g.ts_normalize_name("<get>url"), "url")
        self.assertEqual(s2g.ts_normalize_name("<set>url"), "url")
        self.assertEqual(s2g.ts_normalize_name("render"), "render")

    def test_rendered_function_type_detection(self):
        yes = ["var f: (path: string) => string[]",
               "var f: <Name extends string>(n: Name) => string",
               "var f: (...paths: string[]) => string"]
        no = ["var f: NotFoundHandler",
              "var f: { [key: string]: Pattern; }",
              "var f: readonly [\"get\"]",
              "var f: unique symbol",
              "var f"]
        for h in yes:
            self.assertTrue(s2g.ts_rendered_type_is_callable(h), h)
        for h in no:
            self.assertFalse(s2g.ts_rendered_type_is_callable(h), h)

    def test_module_level_term_excludes_members(self):
        self.assertTrue(s2g.ts_is_module_level_term(
            s2g.ParsedSymbol(TS + "src/`mod.ts`/splitPath.")))
        self.assertFalse(s2g.ts_is_module_level_term(
            s2g.ParsedSymbol(TS + "src/`mod.ts`/Widget#count.")))
        self.assertFalse(s2g.ts_is_module_level_term(
            s2g.ParsedSymbol(TS + "src/`mod.ts`/Opts#typeLiteral0:res.")))


class TestTypescriptConversion(unittest.TestCase):
    def setUp(self):
        self.golden, self.stats = convert_ts_fixture()

    def one(self, name, file="src/mod.ts"):
        hits = by_name(self.golden, name, file)
        self.assertEqual(len(hits), 1, "%s in %s -> %r" % (name, file, hits))
        return hits[0]

    def test_the_four_type_kinds_are_distinguished(self):
        # This is the whole reason TypeScript is in the corpus: it is the only
        # language that uses class / interface / type_alias / enum at once, and
        # the `#` descriptor cannot tell them apart -- only the rendered head can.
        self.assertEqual(self.one("Widget")["kind"], "class")
        self.assertEqual(self.one("Router")["kind"], "interface")
        self.assertEqual(self.one("Pattern")["kind"], "type_alias")
        self.assertEqual(self.one("Algo")["kind"], "enum")

    def test_type_without_documentation_is_dropped_not_guessed(self):
        self.assertEqual(by_name(self.golden, "Mystery"), [])
        self.assertEqual(self.stats.dropped["type_unclassified"], 1)

    def test_class_method_owner_and_span(self):
        m = self.one("render")
        self.assertEqual((m["kind"], m["owner"]), ("method", "Widget"))
        self.assertEqual((m["start_line"], m["end_line"]), (22, 25))

    def test_interface_members_are_methods_of_the_interface(self):
        m = self.one("add")
        self.assertEqual((m["kind"], m["owner"]), ("method", "Router"))

    def test_constructor_and_accessor_names(self):
        c = self.one("constructor")
        self.assertEqual((c["kind"], c["owner"]), ("method", "Widget"))
        g = self.one("size")
        self.assertEqual((g["kind"], g["owner"]), ("method", "Widget"))
        # accessors carry no enclosing_range, so the span collapses to the
        # declaration line -- a known oracle artifact, see POLICY.md §11.9
        self.assertEqual((g["start_line"], g["end_line"]), (27, 27))

    def test_top_level_arrow_is_a_function(self):
        f = self.one("splitPath")
        self.assertEqual((f["kind"], f["owner"]), ("function", None))
        # declaration line from the name occurrence; end from enclosing_range
        self.assertEqual((f["start_line"], f["end_line"]), (37, 41))

    def test_arrow_annotated_with_an_alias_is_still_a_function(self):
        f = self.one("notFoundHandler")
        self.assertEqual(f["kind"], "function")
        self.assertEqual(self.stats.disagreements[
            "term:enclosing_range=True vs rendered_fn_type=False"], 1)

    def test_reexport_binding_is_not_a_definition(self):
        # `export const verify = Other.verify` renders as a function TYPE but
        # defines no function body; scip-typescript gives it no enclosing_range.
        self.assertEqual(by_name(self.golden, "verify"), [])
        self.assertEqual(self.stats.disagreements[
            "term:enclosing_range=False vs rendered_fn_type=True"], 1)

    def test_plain_constant_is_dropped(self):
        self.assertEqual(by_name(self.golden, "METHODS"), [])
        self.assertEqual(self.stats.dropped["binding"], 2)

    def test_function_declaration(self):
        f = self.one("parseBody")
        self.assertEqual((f["kind"], f["owner"]), ("function", None))
        self.assertEqual((f["start_line"], f["end_line"]), (51, 56))

    def test_file_module_is_dropped_but_a_real_namespace_is_kept(self):
        self.assertEqual(self.stats.dropped["file_module"], 4)
        ns = self.one("JSX")
        self.assertEqual(ns["kind"], "namespace")
        self.assertEqual((ns["start_line"], ns["end_line"]), (58, 58))
        # `namespace` is NOT excluded for TypeScript: loregrep emits it.
        self.assertEqual(self.golden["policy"]["excluded_kinds"], [])

    def test_declarations_inside_a_namespace_are_kept(self):
        e = self.one("Element")
        self.assertEqual((e["kind"], e["owner"]), ("type_alias", None))

    def test_quoted_module_declaration_is_dropped(self):
        self.assertEqual([s for s in self.golden["symbols"] if "'" in s["name"]], [])
        self.assertEqual(self.stats.dropped["quoted_module_declaration"], 1)

    def test_members_parameters_type_params_and_locals_are_dropped(self):
        self.assertEqual(by_name(self.golden, "count"), [])
        self.assertEqual(by_name(self.golden, "name"), [])
        self.assertEqual(by_name(self.golden, "HS256"), [])
        self.assertEqual(by_name(self.golden, "res"), [])
        self.assertEqual(by_name(self.golden, "T"), [])
        d = self.stats.dropped
        self.assertEqual(d["member_binding"], 4)      # name, HS256, count, res
        self.assertEqual(d["descriptor:macro"], 1)    # `name0:`
        self.assertEqual(d["descriptor:parameter"], 1)
        self.assertEqual(d["descriptor:type_parameter"], 1)
        self.assertEqual(d["local"], 1)

    def test_descriptor_and_doc_disagreement_is_counted_not_silent(self):
        w = self.one("weird")
        self.assertEqual(w["kind"], "function")
        self.assertEqual(self.stats.disagreements["descriptor:method vs doc:type-ish"], 1)

    def test_only_definitions_are_emitted(self):
        self.assertEqual(len(by_name(self.golden, "splitPath")), 1)

    def test_external_package_never_enters_the_golden(self):
        self.assertEqual(by_name(self.golden, "ZodType"), [])
        self.assertEqual(self.stats.dropped["external_package"], 1)
        self.assertEqual(dict(self.stats.packages_dropped), {"zod": 1})
        self.assertEqual(list(self.stats.packages_kept), ["demo"])

    def test_first_party_inference_uses_file_module_ownership(self):
        self.assertEqual(s2g.infer_local_packages_typescript(typescript_fixture_index()),
                         {"demo"})

    def test_test_flag_from_path(self):
        s = self.one("setup", "src/mod.test.ts")
        self.assertEqual(s["flags"], ["test"])
        self.assertEqual(self.one("parseBody")["flags"], [])

    def test_ambient_flag_from_dts_path(self):
        self.assertEqual(self.one("Deno", "src/globals.d.ts")["flags"], ["ambient"])

    def test_visibility_always_abstains(self):
        # scip-typescript renders neither `export` nor `private`/`protected`.
        self.assertEqual({s["visibility"] for s in self.golden["symbols"]}, {"unknown"})

    def test_excluded_paths_count_definitions_not_files(self):
        golden, stats = convert_ts_fixture(exclude=["benchmarks"])
        self.assertEqual(by_name(golden, "main"), [])
        self.assertEqual(stats.dropped["excluded_path_file"], 1)
        self.assertEqual(stats.dropped["excluded_path"], 2)  # module + main()
        self.assertEqual(golden["policy"]["excluded_paths"], ["benchmarks"])

    def test_nested_functions_are_declared_absent(self):
        # scip-typescript makes every definition inside a function body a
        # `local N`, so a TypeScript golden structurally cannot contain one.
        self.assertIs(self.golden["policy"]["include_nested_functions"], False)
        self.assertEqual([s for s in self.golden["symbols"]
                          if "nested" in s["flags"]], [])

    def test_generated_is_never_flagged(self):
        self.assertEqual([s for s in self.golden["symbols"]
                          if "generated" in s["flags"]], [])
        self.assertIs(self.golden["policy"]["include_generated"], False)

    def test_deterministic_sort_and_paths(self):
        keys = [(s["file"], s["start_line"], s["name"]) for s in self.golden["symbols"]]
        self.assertEqual(keys, sorted(keys))
        for s in self.golden["symbols"]:
            self.assertFalse(s["file"].startswith("./"))
            self.assertNotIn("\\", s["file"])

    def test_generator_provenance(self):
        gen = self.golden["generator"]
        self.assertEqual((gen["indexer"], gen["indexer_version"]),
                         ("scip-typescript", "0.4.0"))
        self.assertEqual(self.golden["language"], "typescript")

    def test_schema_conformance(self):
        with open(SCHEMA_PATH) as fh:
            schema = json.load(fh)
        self.assertEqual(validate(self.golden, schema), [])


# --------------------------------------------------------------------------
# Schema conformance
# --------------------------------------------------------------------------

SCHEMA_PATH = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "golden.schema.json")


def validate(instance, schema, root=None, path="$"):
    """Minimal draft-07 validator covering the keywords golden.schema.json uses.

    A dependency-free check is the point: the converter must not need a pip
    install to prove it honours its own contract.
    """
    root = root or schema
    errors = []
    if "$ref" in schema:
        target = root
        for part in schema["$ref"].lstrip("#/").split("/"):
            target = target[part]
        return validate(instance, target, root, path)
    if "const" in schema and instance != schema["const"]:
        errors.append("%s: expected const %r, got %r" % (path, schema["const"], instance))
    if "enum" in schema and instance not in schema["enum"]:
        errors.append("%s: %r not in %r" % (path, instance, schema["enum"]))
    types = schema.get("type")
    if types:
        types = [types] if isinstance(types, str) else types
        pytypes = {"object": dict, "array": list, "string": str, "integer": int,
                   "number": (int, float), "boolean": bool, "null": type(None)}
        if not any(isinstance(instance, pytypes[t]) and not (t == "integer" and isinstance(instance, bool))
                   for t in types):
            errors.append("%s: %r is not of type %s" % (path, instance, types))
            return errors
    if isinstance(instance, dict) and (schema.get("type") == "object" or "properties" in schema):
        for req in schema.get("required", []):
            if req not in instance:
                errors.append("%s: missing required property %r" % (path, req))
        props = schema.get("properties", {})
        if schema.get("additionalProperties") is False:
            for key in instance:
                if key not in props:
                    errors.append("%s: additional property %r" % (path, key))
        for key, sub in props.items():
            if key in instance:
                errors.extend(validate(instance[key], sub, root, "%s.%s" % (path, key)))
    if isinstance(instance, list) and "items" in schema:
        for i, item in enumerate(instance):
            errors.extend(validate(item, schema["items"], root, "%s[%d]" % (path, i)))
    if isinstance(instance, str) and "pattern" in schema:
        import re as _re
        if not _re.search(schema["pattern"], instance):
            errors.append("%s: %r does not match %r" % (path, instance, schema["pattern"]))
    if isinstance(instance, int) and not isinstance(instance, bool) and "minimum" in schema:
        if instance < schema["minimum"]:
            errors.append("%s: %r < minimum %r" % (path, instance, schema["minimum"]))
    return errors


class TestSchemaConformance(unittest.TestCase):
    def test_fixture_golden_validates(self):
        with open(SCHEMA_PATH) as fh:
            schema = json.load(fh)
        golden, _ = convert_fixture()
        self.assertEqual(validate(golden, schema), [])

    def test_validator_catches_a_bad_kind(self):
        with open(SCHEMA_PATH) as fh:
            schema = json.load(fh)
        golden, _ = convert_fixture()
        golden["symbols"][0]["kind"] = "widget"
        self.assertTrue(validate(golden, schema))


class TestCli(unittest.TestCase):
    def test_end_to_end_json_input(self):
        with tempfile.TemporaryDirectory() as tmp:
            src = os.path.join(tmp, "index.json")
            out = os.path.join(tmp, "nested", "golden-symbols.json")
            with open(src, "w") as fh:
                json.dump(fixture_index(), fh)
            rc = s2g.main(["--scip-json", src, "--repo", "demo", "--commit", COMMIT,
                           "--language", "rust", "--out", out])
            self.assertEqual(rc, 0)
            with open(out) as fh:
                text = fh.read()
            golden = json.loads(text)
            self.assertEqual(golden["schema_version"], 1)
            self.assertEqual(golden["repo"], "demo")
            self.assertEqual(golden["commit"], COMMIT)
            self.assertTrue(golden["symbols"])
            self.assertIn("\n  ", text)  # pretty-printed, so diffs are reviewable

    def test_bad_commit_is_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            src = os.path.join(tmp, "index.json")
            with open(src, "w") as fh:
                json.dump(fixture_index(), fh)
            self.assertRaises(SystemExit, s2g.main,
                              ["--scip-json", src, "--repo", "demo", "--commit", "nope",
                               "--language", "rust", "--out", os.path.join(tmp, "g.json")])


class TestKindExclusion(unittest.TestCase):
    def test_namespaces_are_excluded_by_default(self):
        golden, _ = convert_fixture()
        self.assertEqual([s for s in golden["symbols"] if s["kind"] == "namespace"], [])
        self.assertEqual(golden["policy"]["excluded_kinds"], ["namespace"])

    def test_test_flag_still_propagates_with_modules_excluded(self):
        # Module entries are what mark their children as test symbols, so the
        # exclusion must happen AFTER propagation, not during conversion.
        golden, _ = convert_fixture()
        flagged = [s for s in golden["symbols"] if "test" in (s.get("flags") or [])]
        self.assertTrue(flagged, "test propagation must survive module exclusion")


if __name__ == "__main__":
    unittest.main(verbosity=2)
