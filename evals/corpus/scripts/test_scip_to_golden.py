#!/usr/bin/env python3
"""Tests for scip_to_golden.py.

    python3 evals/corpus/scripts/test_scip_to_golden.py

Stdlib unittest only, and self-contained: the SCIP fixture below is written by
hand so the suite does not depend on the multi-megabyte sample index. It mirrors
the shapes rust-analyzer 1.97.1 / scip v0.9.0 actually emit (both range
encodings, `impl#[T][Trait]` descriptors, backtick-escaped generic impl targets,
`local N` symbols, per-target `crate/` pseudo-symbols).
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

    def test_non_rust_language_is_refused(self):
        self.assertRaises(s2g.ConversionError, convert_fixture, language="python")


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
