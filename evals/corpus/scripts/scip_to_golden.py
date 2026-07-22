#!/usr/bin/env python3
"""SCIP -> corpus definition golden converter (eval plan L1-S2).

Reads a SCIP index (binary `.scip`, converted via the `scip` CLI, or an
already-converted `scip print --json` document) and emits the neutral
definition golden described by evals/corpus/golden.schema.json.

Definitions only. References/edges are out of scope (they land with P3-7).

Stdlib only, on purpose: evals/retrieval/run.py has the same constraint, and a
golden generator that needs a pip environment is a golden generator that rots.

The inclusion policy and the SCIP-kind -> neutral-kind mapping this implements
are documented in evals/corpus/POLICY.md. Keep the two in sync; POLICY.md is
the decision record for what this file does.

Languages: rust (rust-analyzer SCIP), python (scip-python) and typescript
(scip-typescript). Each has its own converter function; `convert()` only
dispatches. Any other --language is refused rather than pretending one descriptor
grammar generalises to another.

Usage:
    python3 scip_to_golden.py --scip index.scip --repo loregrep \\
        --commit <40-hex> --language rust --out golden-symbols.json
"""

from __future__ import annotations

import argparse
import collections
import datetime
import fnmatch
import json
import os
import re
import subprocess
import sys

CONVERTER_VERSION = "1.1.0"
SCHEMA_VERSION = 1

# --------------------------------------------------------------------------
# SCIP constants
# --------------------------------------------------------------------------

# scip.proto Occurrence.symbol_roles is a bitset; bit 1 == Definition.
ROLE_DEFINITION = 0x1

# scip.proto SymbolInformation.Kind numbers observed in scip v0.9.0 /
# rust-analyzer 1.97.1 output. The numeric values are NOT stable across scip
# proto revisions (values are assigned out of alphabetical order and entries
# have been renamed/removed), so this table is a *secondary* signal only: the
# classifier prefers the rendered signature text, which is version independent.
# See POLICY.md.
SCIP_KIND_NAMES = {
    8: "Constant",
    11: "Enum",
    12: "EnumMember",
    15: "Field",
    17: "Function",
    26: "Method",
    29: "Module",
    37: "Parameter",
    44: "SelfParameter",
    49: "Struct",
    53: "Trait",
    55: "Type",
    58: "TypeParameter",
    61: "Variable",
    70: "TraitMethod",
    80: "StaticMethod",
}

# SCIP kind name -> neutral kind ("method" is resolved to "function" later when
# no owner can be determined). None means "deliberately not a golden symbol".
SCIP_KIND_TO_NEUTRAL = {
    "Function": "function",
    "Method": "method",
    "TraitMethod": "method",
    "StaticMethod": "method",
    "AbstractMethod": "method",
    "Struct": "struct",
    "Class": "class",
    "Enum": "enum",
    "Interface": "interface",
    "Trait": "trait",
    "Protocol": "interface",
    "TypeAlias": "type_alias",
    "Type": "type_alias",
    "Module": "namespace",
    "Namespace": "namespace",
    "Union": "struct",
    # Explicitly dropped (no neutral kind in the schema):
    "Constant": None,
    "EnumMember": None,
    "Field": None,
    "Parameter": None,
    "SelfParameter": None,
    "TypeParameter": None,
    "Variable": None,
    "Macro": None,
    "Property": None,
}

# Rust source keyword (as rendered in signature_documentation) -> neutral kind.
RUST_SIGNATURE_KEYWORDS = {
    "fn": "function",  # promoted to "method" when an owner exists
    "struct": "struct",
    "enum": "enum",
    "trait": "trait",
    "type": "type_alias",
    "mod": "namespace",
    "union": "struct",
    "const": None,
    "static": None,
    "impl": None,
    "macro_rules": None,
    "macro": None,
    "extern": None,  # `extern crate ...` pseudo-symbols
}

RUST_VISIBILITY_RE = re.compile(r"^(pub\s*\([^)]*\)|pub)\s+")
STDLIB_PACKAGES = {"std", "core", "alloc", "proc_macro", "test", "rust-std"}

# scip-python stamps every non-first-party symbol with the distribution it came
# from; the typeshed bundle it ships under is spelled exactly like this.
PYTHON_STDLIB_PACKAGES = {"python-stdlib"}


class ConversionError(Exception):
    pass


# --------------------------------------------------------------------------
# SCIP symbol string parsing
# --------------------------------------------------------------------------


class Descriptor(object):
    """One component of a SCIP symbol's descriptor suffix."""

    __slots__ = ("name", "kind")

    def __init__(self, name, kind):
        self.name = name
        self.kind = kind  # namespace|type|term|method|type_parameter|parameter|macro

    def __repr__(self):  # pragma: no cover - debugging aid
        return "Descriptor(%r, %r)" % (self.name, self.kind)


def _read_name(s, i):
    """Read a (possibly backtick-escaped) descriptor name starting at s[i]."""
    if i < len(s) and s[i] == "`":
        i += 1
        out = []
        while i < len(s):
            if s[i] == "`":
                if i + 1 < len(s) and s[i + 1] == "`":
                    out.append("`")
                    i += 2
                    continue
                return "".join(out), i + 1
            out.append(s[i])
            i += 1
        raise ConversionError("unterminated backtick in descriptor: %r" % s)
    start = i
    while i < len(s) and s[i] not in "/#.:!()[]":
        i += 1
    return s[start:i], i


def parse_descriptors(suffix):
    """Parse a SCIP descriptor suffix into components.

    Handles the forms rust-analyzer emits:
        storage/graph/resolve_rust/resolve_rust_import().
        loregrep/impl#[LoreGrep][Clone]clone().
        core/errors/LoreGrepError#NotScanned#
        MockFunction#name.
    """
    out = []
    i = 0
    n = len(suffix)
    while i < n:
        ch = suffix[i]
        if ch == "[":
            if i + 1 < n and suffix[i + 1] == "`":
                # Backtick-escaped: the name may itself contain ']'.
                name, k = _read_name(suffix, i + 1)
                if k >= n or suffix[k] != "]":
                    raise ConversionError("unterminated type parameter in %r" % suffix)
                out.append(Descriptor(name, "type_parameter"))
                i = k + 1
                continue
            j = suffix.index("]", i)
            out.append(Descriptor(suffix[i + 1 : j], "type_parameter"))
            i = j + 1
            continue
        if ch == "(":
            # A parameter descriptor: (name)
            j = suffix.index(")", i)
            out.append(Descriptor(suffix[i + 1 : j], "parameter"))
            i = j + 1
            continue
        name, i = _read_name(suffix, i)
        if i >= n:
            if name:
                out.append(Descriptor(name, "term"))
            break
        suf = suffix[i]
        if suf == "/":
            out.append(Descriptor(name, "namespace"))
            i += 1
        elif suf == "#":
            out.append(Descriptor(name, "type"))
            i += 1
        elif suf == ":":
            out.append(Descriptor(name, "macro"))
            i += 1
        elif suf == "!":
            out.append(Descriptor(name, "macro"))
            i += 1
        elif suf == "(":
            # method: name(disambiguator).
            j = suffix.index(")", i)
            i = j + 1
            if i < n and suffix[i] == ".":
                i += 1
            out.append(Descriptor(name, "method"))
        elif suf == ".":
            out.append(Descriptor(name, "term"))
            i += 1
        else:  # pragma: no cover - defensive
            raise ConversionError("unexpected descriptor char %r in %r" % (suf, suffix))
    return out


class ParsedSymbol(object):
    __slots__ = ("scheme", "manager", "package", "version", "descriptors", "is_local", "raw")

    def __init__(self, raw):
        self.raw = raw
        self.is_local = raw.startswith("local ")
        self.scheme = self.manager = self.package = self.version = ""
        self.descriptors = []
        if self.is_local:
            return
        parts = raw.split(" ", 4)
        if len(parts) < 5:
            # e.g. a scheme-only or malformed symbol; treat as opaque.
            self.scheme = parts[0] if parts else ""
            return
        self.scheme, self.manager, self.package, self.version, suffix = parts
        self.descriptors = parse_descriptors(suffix)


def normalize_type_name(raw):
    """`FileSet<'a>` -> FileSet.

    rust-analyzer backtick-escapes impl-target descriptors that contain
    generics or lifetimes; the golden wants the bare identifier a user would
    search for, so escaping and type arguments are stripped.
    """
    name = raw.strip()
    if name.startswith("`") and name.endswith("`") and len(name) >= 2:
        name = name[1:-1]
    name = name.lstrip("&").strip()
    for cut in ("<", " "):
        idx = name.find(cut)
        if idx > 0:
            name = name[:idx]
    return name.strip()


def descriptor_owner(descriptors):
    """Enclosing type name for a method-ish descriptor chain, or None.

    Rust-analyzer emits two shapes:
        <mod path>/<Type>#<method>().            (trait declaration / assoc item)
        <mod path>/impl#[<Type>][<Trait>]<m>().  (inherent or trait impl)
    In the second shape the *first* bracket is the implementing type; the
    second, when present, is the trait being implemented.
    """
    head = descriptors[:-1]
    for idx, d in enumerate(head):
        if d.kind == "type" and d.name == "impl":
            for nxt in head[idx + 1 :]:
                if nxt.kind == "type_parameter":
                    return normalize_type_name(nxt.name) or None
            return None
    for d in reversed(head):
        if d.kind == "type":
            return normalize_type_name(d.name) or None
    return None


# --------------------------------------------------------------------------
# Ranges
# --------------------------------------------------------------------------


def range_to_lines(rng):
    """SCIP range (0-indexed, half-open) -> (start_line, end_line) 1-indexed inclusive.

    Accepts both encodings:
        [startLine, startChar, endChar]           (single line)
        [startLine, startChar, endLine, endChar]  (multi line)
    """
    if rng is None:
        return None
    if len(rng) == 3:
        start_line, _start_char, _end_char = rng
        end_line = start_line
    elif len(rng) == 4:
        start_line, _start_char, end_line, end_char = rng
        # Half-open: an end character of 0 means the range stops at the start of
        # `end_line`, so the last line actually covered is end_line - 1.
        if end_char == 0 and end_line > start_line:
            end_line -= 1
    else:
        raise ConversionError("unexpected SCIP range arity: %r" % (rng,))
    return start_line + 1, end_line + 1


# --------------------------------------------------------------------------
# Rust classification
# --------------------------------------------------------------------------


def signature_text(sym_info):
    sig = (sym_info or {}).get("signature_documentation") or {}
    return (sig.get("text") or "").strip()


def strip_rust_modifiers(head):
    """Drop visibility and item modifiers so the item keyword is first.

    `const` is only a modifier when it precedes `fn`; `const NAME: T` is a
    constant item and must keep its keyword.
    """
    head = RUST_VISIBILITY_RE.sub("", head).strip()
    while True:
        m = re.match(r"(default|async|unsafe|extern\s+\"[^\"]*\"|extern)\s+", head)
        if m:
            head = head[m.end() :].lstrip()
            continue
        if re.match(r"const\s+(default\s+|async\s+|unsafe\s+|extern\s+)*fn\b", head):
            head = head[len("const") :].lstrip()
            continue
        return head


def rust_signature_keyword(sig):
    """First source keyword of a rendered Rust signature, visibility stripped."""
    if not sig:
        return None
    head = strip_rust_modifiers(sig.split("\n", 1)[0].strip())
    m = re.match(r"[A-Za-z_][A-Za-z0-9_]*", head)
    if not m:
        return None
    word = m.group(0)
    if word == "macro_rules":
        return "macro_rules"
    return word if word in RUST_SIGNATURE_KEYWORDS else None


def visibility_is_inherited(descriptors):
    """True when the definition site carries no visibility modifier of its own.

    Members of a trait declaration (`Trait#member().`) and of a trait impl
    (`impl#[Type][Trait]member().`) are exactly as visible as the trait; Rust
    forbids writing a modifier there, so SCIP cannot carry one either.
    """
    head = descriptors[:-1]
    for idx, d in enumerate(head):
        if d.kind == "type" and d.name == "impl":
            brackets = [x for x in head[idx + 1 :] if x.kind == "type_parameter"]
            return len(brackets) >= 2
    return any(d.kind == "type" for d in head)


def rust_visibility(sig, parsed, neutral_kind):
    """public / private / unknown. Never guesses — see POLICY.md."""
    if not sig:
        return "unknown"
    if neutral_kind == "method" and visibility_is_inherited(parsed.descriptors):
        return "unknown"
    head = sig.split("\n", 1)[0].strip()
    m = RUST_VISIBILITY_RE.match(head)
    if m:
        if m.group(1) == "pub":
            return "public"
        return "unknown"  # pub(crate)/pub(super)/pub(in ...) — restricted, not modelled
    return "private"


def rust_flags(sig, parsed, rel_path, neutral_kind):
    """Only flags derivable without guessing. `test` is applied separately,
    per-document, once the test-module spans of that document are known."""
    flags = []
    head = sig.split("\n", 1)[0].strip() if sig else ""
    stripped = RUST_VISIBILITY_RE.sub("", head).strip()
    if neutral_kind in ("function", "method") and re.match(
        r"^(const\s+|unsafe\s+)*async\s+(unsafe\s+)?fn\b", stripped
    ):
        flags.append("async")
    if is_test_path(rel_path) or has_test_module_descriptor(parsed):
        flags.append("test")
    return sorted(set(flags))


def is_test_path(rel_path):
    posix = rel_path.replace(os.sep, "/")
    return posix == "tests" or posix.startswith("tests/")


def has_test_module_descriptor(parsed):
    """Conventionally-named test module somewhere in the symbol's module path."""
    return any(d.kind == "namespace" and d.name in ("tests", "test") for d in parsed.descriptors)


def is_crate_root(parsed):
    """rust-analyzer emits one `<pkg> crate/` pseudo-symbol per crate target,
    whose definition is the whole root file and which has no display name.
    It is not an identifier anyone wrote; see POLICY.md."""
    return len(parsed.descriptors) == 1 and parsed.descriptors[0].kind == "namespace" and parsed.descriptors[0].name == "crate"


# --------------------------------------------------------------------------
# Conversion
# --------------------------------------------------------------------------


class Stats(object):
    def __init__(self):
        self.dropped = collections.Counter()
        self.kinds = collections.Counter()
        self.disagreements = collections.Counter()
        self.packages_kept = collections.Counter()
        self.packages_dropped = collections.Counter()


def infer_local_packages(index):
    """Packages that are part of the indexed checkout.

    A crate that lives in this repo defines its own crate root and its own
    modules inside the indexed documents. A dependency never does: the only way
    a foreign package acquires a definition here is macro expansion, which
    produces types/members, not modules. So "owns a module or crate-root
    definition" separates in-repo crates (including every member of a
    workspace) from leaked dependency symbols. Falls back to "every non-stdlib
    package with a definition" if the index carries no module definitions at
    all, and can always be overridden with --package.
    """
    module_owners = set()
    any_owners = set()
    for doc in index.get("documents", []):
        info = {s["symbol"]: s for s in doc.get("symbols", [])}
        for occ in doc.get("occurrences", []):
            if not occ.get("symbol_roles", 0) & ROLE_DEFINITION:
                continue
            sym = occ.get("symbol", "")
            if sym.startswith("local "):
                continue
            p = ParsedSymbol(sym)
            if not p.package or p.package in STDLIB_PACKAGES or "://" in p.version:
                continue
            any_owners.add(p.package)
            kind = SCIP_KIND_NAMES.get((info.get(sym) or {}).get("kind"))
            if is_crate_root(p) or kind in ("Module", "Namespace"):
                module_owners.add(p.package)
    return module_owners or any_owners


def macro_generated_ranges(doc):
    """Ranges carrying more than one distinct definition symbol.

    Rust-analyzer attributes definitions produced by a macro expansion to the
    macro *call site*, so several unrelated symbols collapse onto one range.
    That collision is the only reliable in-band signal of macro-generated
    definitions available here; see POLICY.md.
    """
    by_range = collections.defaultdict(set)
    for occ in doc.get("occurrences", []):
        if not occ.get("symbol_roles", 0) & ROLE_DEFINITION:
            continue
        sym = occ.get("symbol", "")
        if sym.startswith("local "):
            continue
        by_range[tuple(occ.get("range") or ())].add(sym)
    return set(r for r, syms in by_range.items() if len(syms) > 1)



RUST_ITEM_KEYWORD_RE = re.compile(
    r"\b(fn|struct|enum|trait|type|union|mod|const|static|impl|macro_rules)\b"
)


def macro_generated(repo_root, rel_path, start_line):
    """True when the symbol's declaration line holds no Rust item keyword.

    rust-analyzer expands macros, so it reports `syntax!(literal1, ...)` as a
    definition of `literal1`; a syntax-level parser structurally cannot see it.
    Such a symbol's declaration line is a macro invocation (or a bare argument on
    its own line for a multi-line invocation) rather than `fn`/`struct`/... —
    that absence is the signal. Requires the source tree (--repo-root).
    """
    if not repo_root:
        return False
    path = os.path.join(repo_root, rel_path)
    try:
        with open(path, "r", errors="replace") as fh:
            lines = fh.readlines()
    except OSError:
        return False
    if not (1 <= start_line <= len(lines)):
        return False
    line = lines[start_line - 1].split("//")[0]
    return not RUST_ITEM_KEYWORD_RE.search(line)


# Kinds deliberately absent from the golden; see the policy block emitted below.
EXCLUDED_KINDS = ("namespace",)

CFG_TEST_RE = re.compile(r"#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]")


def cfg_test_module(repo_root, rel_path, start_line):
    """True when the module definition starting at `start_line` (1-indexed) is
    annotated `#[cfg(test)]`.

    `start_line` is the DECLARATION line (see the span convention in POLICY.md),
    so attributes sit ABOVE it — scan upwards through the contiguous run of
    attributes and doc comments immediately preceding the declaration. Requires
    the source tree; without --repo-root only the naming convention is used.
    """
    if not repo_root:
        return False
    path = os.path.join(repo_root, rel_path)
    try:
        with open(path, "r", errors="replace") as fh:
            lines = fh.readlines()
    except OSError:
        return False
    # Walk backwards from the line above the declaration. Blank lines, doc
    # comments and other attributes may intervene; anything else ends the run.
    idx = start_line - 2
    scanned = 0
    while idx >= 0 and scanned < 8:
        stripped = lines[idx].strip()
        idx -= 1
        scanned += 1
        if not stripped:
            continue
        if CFG_TEST_RE.search(stripped):
            return True
        if stripped.startswith("#") or stripped.startswith("//"):
            continue  # another attribute or a doc comment; keep looking
        return False
    return False


def convert(index, language, excluded_paths=None, packages=None, stats=None,
            repo_root=None, excluded_kinds=EXCLUDED_KINDS):
    """Dispatch to the per-language converter.

    Each language gets its own function rather than a pile of `if language ==`
    branches inside one loop: the descriptor grammars, the kind signals and the
    "what is first-party" rules genuinely differ, and a shared loop would make a
    Python change able to regress Rust. See POLICY.md.
    """
    impl = {"rust": convert_rust, "python": convert_python,
            "typescript": convert_typescript}.get(language)
    if impl is None:
        raise ConversionError(
            "language %r is not implemented; only 'rust', 'python' and "
            "'typescript' descriptors are understood by this converter "
            "(see POLICY.md)" % language
        )
    return impl(index, excluded_paths=excluded_paths, packages=packages, stats=stats,
                repo_root=repo_root, excluded_kinds=excluded_kinds)


def convert_rust(index, excluded_paths=None, packages=None, stats=None,
                 repo_root=None, excluded_kinds=EXCLUDED_KINDS):
    stats = stats or Stats()
    excluded = tuple(excluded_paths or ())
    allowed_packages = set(packages) if packages else infer_local_packages(index)

    symbols = []
    for doc in index.get("documents", []):
        rel_path = (doc.get("relative_path") or "").replace(os.sep, "/")
        while rel_path.startswith("./"):
            rel_path = rel_path[2:]
        if any(rel_path == e or rel_path.startswith(e.rstrip("/") + "/") for e in excluded):
            stats.dropped["excluded_path"] += 1
            continue
        info = {s["symbol"]: s for s in doc.get("symbols", [])}
        generated = macro_generated_ranges(doc)
        doc_entries = []
        test_spans = []

        for occ in doc.get("occurrences", []):
            if not occ.get("symbol_roles", 0) & ROLE_DEFINITION:
                continue  # a reference, not a definition
            raw = occ.get("symbol", "")
            if raw.startswith("local "):
                stats.dropped["local"] += 1
                continue
            parsed = ParsedSymbol(raw)
            if not parsed.descriptors:
                stats.dropped["unparseable_symbol"] += 1
                continue
            if parsed.package not in allowed_packages:
                stats.dropped["external_package"] += 1
                stats.packages_dropped[parsed.package] += 1
                continue
            if tuple(occ.get("range") or ()) in generated:
                stats.dropped["macro_generated"] += 1
                continue
            if is_crate_root(parsed):
                stats.dropped["crate_root"] += 1
                continue

            sym_info = info.get(raw, {})
            sig = signature_text(sym_info)
            kw = rust_signature_keyword(sig)
            scip_kind = SCIP_KIND_NAMES.get(sym_info.get("kind"))

            neutral = None
            if kw is not None:
                neutral = RUST_SIGNATURE_KEYWORDS[kw]
                if scip_kind is not None:
                    from_scip = SCIP_KIND_TO_NEUTRAL.get(scip_kind, "?")
                    normalized = "method" if (neutral == "function" and from_scip == "method") else neutral
                    if from_scip != "?" and from_scip != normalized:
                        stats.disagreements["%s vs sig:%s" % (scip_kind, kw)] += 1
            elif scip_kind is not None:
                neutral = SCIP_KIND_TO_NEUTRAL.get(scip_kind)
            elif parsed.descriptors[-1].kind == "method":
                # No SymbolInformation at all. The descriptor suffix still says
                # "callable"; it cannot say struct-vs-enum-vs-trait, so a
                # type-ish descriptor is dropped rather than guessed.
                neutral = "function"
            elif parsed.descriptors[-1].kind == "namespace":
                neutral = "namespace"
            if neutral is None:
                stats.dropped["kind:%s" % (kw or scip_kind or "unknown")] += 1
                continue

            name = sym_info.get("display_name")
            if not name:
                # Fall back to the trailing descriptor identifier.
                name = parsed.descriptors[-1].name
            if not name or name == "impl":
                stats.dropped["anonymous"] += 1
                continue

            owner = None
            if neutral in ("function", "method"):
                owner = descriptor_owner(parsed.descriptors)
                neutral = "method" if owner else "function"

            # Span convention (see POLICY.md). `enclosing_range` covers the item
            # INCLUDING its leading doc comments and attributes, which no
            # syntax-level extractor reports as part of the declaration — comparing
            # against it turns a doc comment into a span defect. So the golden's
            # start_line is the DECLARATION line: the line the symbol's own name
            # occurrence sits on (`pub fn foo(` / `struct Bar`), which both an
            # oracle and a parser can agree on. end_line still comes from
            # `enclosing_range`, so the span remains body-inclusive.
            name_span = range_to_lines(occ.get("range"))
            enclosing_span = range_to_lines(occ.get("enclosing_range") or None)
            if name_span is None and enclosing_span is None:
                stats.dropped["no_range"] += 1
                continue
            start_line = (name_span or enclosing_span)[0]
            end_line = (enclosing_span or name_span)[1]
            if end_line < start_line:
                end_line = start_line

            entry = {
                "name": name,
                "kind": neutral,
                "file": rel_path,
                "start_line": start_line,
                "end_line": end_line,
                "owner": owner,
                "visibility": rust_visibility(sig, parsed, neutral),
                "flags": rust_flags(sig, parsed, rel_path, neutral),
            }
            doc_entries.append(entry)
            stats.kinds[neutral] += 1
            stats.packages_kept[parsed.package] += 1
            if neutral == "namespace" and (
                "test" in entry["flags"] or cfg_test_module(repo_root, rel_path, start_line)
            ):
                test_spans.append((start_line, end_line))

        # A definition inside a `#[cfg(test)]` / conventionally-named test
        # module is a test symbol, whatever its own name.
        for entry in doc_entries:
            if macro_generated(repo_root, rel_path, entry["start_line"]):
                if "generated" not in entry["flags"]:
                    entry["flags"] = sorted(entry["flags"] + ["generated"])

            if "test" in entry["flags"]:
                continue
            if any(lo <= entry["start_line"] <= hi for lo, hi in test_spans):
                entry["flags"] = sorted(entry["flags"] + ["test"])
        # Excluded kinds are removed only AFTER the loop above: module entries are
        # what mark their children as test symbols, so dropping them earlier
        # silently un-flags every symbol inside a `#[cfg(test)] mod tests`.
        for entry in doc_entries:
            if entry["kind"] in (excluded_kinds or ()):
                stats.dropped["kind:%s" % entry["kind"]] += 1
                continue
            symbols.append(entry)

    # Deterministic: primary sort (file, start_line, name); the rest is
    # tie-breaking so equal keys can never reorder between runs.
    symbols.sort(key=lambda s: (s["file"], s["start_line"], s["name"], s["kind"], s["owner"] or "", s["end_line"]))

    # Identical duplicates can only be noise (the same definition reported
    # twice); collapse them.
    deduped = []
    seen = set()
    for s in symbols:
        key = (s["file"], s["start_line"], s["end_line"], s["name"], s["kind"], s["owner"])
        if key in seen:
            stats.dropped["duplicate"] += 1
            continue
        seen.add(key)
        deduped.append(s)

    return deduped, stats


# --------------------------------------------------------------------------
# Python classification (scip-python)
# --------------------------------------------------------------------------

# scip-python 0.6.6 emits NO `kind` field on SymbolInformation: a symbol carries
# only {symbol, documentation, relationships}. So the PRIMARY kind signal for
# Python is the symbol string's trailing descriptor — the structural,
# machine-generated part of the grammar — and the rendered `documentation[0]`
# fenced block is a SECONDARY, corroborating signal only. That is the mirror
# image of the Rust path (where a rendered signature is primary and a numeric,
# drift-prone `kind` enum is secondary): in each language the stable signal wins.
# Disagreements are counted in --stats, never silently resolved. See POLICY.md.
PYTHON_DESCRIPTOR_TO_NEUTRAL = {
    "method": "function",  # `foo().`      — promoted to "method" when a class encloses it
    "type": "class",       # `Foo#`        — every Python `class` statement
    "macro": "namespace",  # `__init__:`   — SCIP *meta* descriptor: the module itself
    "term": None,          # `x.`          — variable / class attribute / TypeVar / type alias
    "parameter": None,     # `foo().(x)`
    "type_parameter": None,
    "namespace": None,     # a bare module path with nothing after it
}

# The fenced block scip-python renders as the first documentation entry, e.g.
#     ```python\n@overload\ndef quote(\n  string: bytes\n) -> str:\n```
PYTHON_DOC_FENCE_RE = re.compile(r"^```python\n(.*?)\n?```\s*$", re.S)

PYTHON_TEST_BASENAME_GLOBS = ("test_*.py", "*_test.py", "tests.py", "conftest.py")


def python_doc_head(sym_info):
    """First non-decorator source line of the rendered ```python``` block, or ""."""
    docs = (sym_info or {}).get("documentation") or []
    if not docs:
        return ""
    first = (docs[0] or "").strip()
    m = PYTHON_DOC_FENCE_RE.match(first)
    if not m:
        # Module pseudo-symbols are rendered unfenced, as a bare `(module) a.b`.
        # Only pyright's parenthesised pseudo-kind renderings are trusted here;
        # anything else in an unfenced first entry is prose (a parameter's
        # docstring line), which must say nothing about kind.
        return first.split("\n", 1)[0] if first.startswith("(") else ""
    for line in m.group(1).split("\n"):
        line = line.strip()
        if not line or line.startswith("@"):
            continue  # decorators sit above the declaration, exactly as in source
        return line
    return ""


def python_doc_kind(head):
    """Neutral kind implied by a rendered declaration head, or None if it says nothing."""
    if not head:
        return None
    if head.startswith("async def ") or head.startswith("def "):
        return "function"
    if head.startswith("class ") or head == "class":
        return "class"
    if head.startswith("(module)"):
        return "namespace"
    # "(variable) x: T", "(type alias) X: ...", "undefined = t.TypeVar(...)" and
    # bare inferred-type renderings all describe bindings, not declarations.
    return None


def python_is_module(parsed):
    """True for the one `<module.path>/__init__:` pseudo-symbol scip-python emits per file.

    `:` is SCIP's *meta* descriptor; parse_descriptors labels it "macro" because
    Rust's `macro_rules!` uses the same suffix. Python emits it for exactly one
    thing — the module — so the label is unambiguous here.
    """
    return bool(parsed.descriptors) and parsed.descriptors[-1].kind == "macro"


def python_module_path(parsed):
    """Dotted module path of a module pseudo-symbol (`flask.app`), or ""."""
    for d in parsed.descriptors:
        if d.kind == "namespace":
            return d.name
    return ""


def python_name(parsed):
    """Bare identifier as written at the definition site.

    scip-python carries no `display_name`, so the name always comes from the
    trailing descriptor. The module pseudo-symbol is the exception: its trailing
    descriptor is the literal `__init__`, and the identifier a reader means is
    the last segment of the dotted module path.
    """
    if python_is_module(parsed):
        return (python_module_path(parsed).rsplit(".", 1) or [""])[-1]
    return parsed.descriptors[-1].name if parsed.descriptors else ""


def python_owner(parsed):
    """Immediately-enclosing class of a callable, or None.

    Only the *innermost* enclosing descriptor counts. A `def` nested inside a
    method (`Flask#run().poll().`) is a closure, not a method of `Flask`: Python
    binds it to the function's local scope and it is unreachable as `Flask.poll`.
    Walking further out (as the Rust path does, where no such nesting exists)
    would invent a method that does not exist.
    """
    head = parsed.descriptors[:-1]
    if head and head[-1].kind == "type":
        return head[-1].name or None
    return None


def python_is_nested(parsed):
    """True when any callable encloses this definition (inner def / inner class)."""
    return any(d.kind == "method" for d in parsed.descriptors[:-1])


def python_visibility(name):
    """PEP 8 underscore convention.

    Python has no visibility *construct*, so this is a naming convention rather
    than a language fact — recorded as such in POLICY.md. Dunders (`__init__`,
    `__call__`) are protocol members and count as public; a single or double
    leading underscore is the community's "private" marker.
    """
    if not name:
        return "unknown"
    if name.startswith("__") and name.endswith("__"):
        return "public"
    return "private" if name.startswith("_") else "public"


def python_is_test_path(rel_path):
    parts = rel_path.split("/")
    if any(p in ("tests", "test") for p in parts[:-1]):
        return True
    base = parts[-1]
    return any(fnmatch.fnmatch(base, g) for g in PYTHON_TEST_BASENAME_GLOBS)


def python_flags(rel_path, parsed, doc_head):
    flags = []
    if doc_head.startswith("async def "):
        flags.append("async")
    if python_is_nested(parsed):
        flags.append("nested")
    if python_is_test_path(rel_path):
        flags.append("test")
    return sorted(set(flags))


def infer_local_packages_python(index):
    """Packages that are part of the indexed checkout.

    A module pseudo-symbol (`<module.path>/__init__:`) is *defined* only in the
    file that is that module, so owning a module definition inside the indexed
    documents is exactly "this package's source is in this checkout". Werkzeug,
    click and python-stdlib appear only as references and never clear this bar.
    Falls back to "every non-stdlib package with any definition"; `--package`
    overrides.
    """
    module_owners = set()
    any_owners = set()
    for doc in index.get("documents", []):
        for occ in doc.get("occurrences", []):
            if not occ.get("symbol_roles", 0) & ROLE_DEFINITION:
                continue
            sym = occ.get("symbol", "")
            if sym.startswith("local "):
                continue
            p = ParsedSymbol(sym)
            if not p.package or p.package.lower() in PYTHON_STDLIB_PACKAGES:
                continue
            any_owners.add(p.package.lower())
            if python_is_module(p):
                module_owners.add(p.package.lower())
    return module_owners or any_owners


def convert_python(index, excluded_paths=None, packages=None, stats=None,
                   repo_root=None, excluded_kinds=EXCLUDED_KINDS):
    """scip-python index -> neutral definition golden. See POLICY.md §10."""
    stats = stats or Stats()
    excluded = tuple(excluded_paths or ())
    # Package-name casing is not stable in scip-python output (first-party
    # definitions say `flask 3.1.3`, ~108 reference occurrences say `Flask
    # 3.1.3`), so first-party membership is decided case-insensitively.
    allowed_packages = set(p.lower() for p in packages) if packages \
        else infer_local_packages_python(index)

    symbols = []
    for doc in index.get("documents", []):
        rel_path = (doc.get("relative_path") or "").replace(os.sep, "/")
        while rel_path.startswith("./"):
            rel_path = rel_path[2:]
        if any(rel_path == e or rel_path.startswith(e.rstrip("/") + "/") for e in excluded):
            # Counted per DEFINITION, not per document: "18 files" says nothing
            # about how much inventory the exclusion removed.
            stats.dropped["excluded_path_file"] += 1
            stats.dropped["excluded_path"] += sum(
                1 for o in doc.get("occurrences", [])
                if o.get("symbol_roles", 0) & ROLE_DEFINITION
                and not (o.get("symbol") or "").startswith("local ")
            )
            continue
        info = {s["symbol"]: s for s in doc.get("symbols", [])}

        for occ in doc.get("occurrences", []):
            if not occ.get("symbol_roles", 0) & ROLE_DEFINITION:
                continue  # a reference, not a definition
            raw = occ.get("symbol", "")
            if raw.startswith("local "):
                stats.dropped["local"] += 1
                continue
            parsed = ParsedSymbol(raw)
            if not parsed.descriptors:
                stats.dropped["unparseable_symbol"] += 1
                continue
            if parsed.package.lower() not in allowed_packages:
                stats.dropped["external_package"] += 1
                stats.packages_dropped[parsed.package] += 1
                continue

            sym_info = info.get(raw, {})
            doc_head = python_doc_head(sym_info)
            descriptor_kind = parsed.descriptors[-1].kind
            neutral = PYTHON_DESCRIPTOR_TO_NEUTRAL.get(descriptor_kind, None)

            from_doc = python_doc_kind(doc_head)
            if neutral is not None and from_doc is not None and from_doc != neutral:
                stats.disagreements["descriptor:%s vs doc:%s" % (descriptor_kind, from_doc)] += 1
            if neutral is None:
                stats.dropped["kind:%s" % descriptor_kind] += 1
                continue

            name = python_name(parsed)
            if not name:
                stats.dropped["anonymous"] += 1
                continue

            owner = None
            if neutral == "function":
                owner = python_owner(parsed)
                if owner:
                    neutral = "method"

            # Span convention, identical to the Rust path (see POLICY.md §4).
            # scip-python's `enclosing_range` opens at the first DECORATOR line,
            # so its start is not the declaration; the symbol's own name
            # occurrence is, and that is what a syntax-level parser reports.
            name_span = range_to_lines(occ.get("range"))
            enclosing_span = range_to_lines(occ.get("enclosing_range") or None)
            if name_span is None and enclosing_span is None:
                stats.dropped["no_range"] += 1
                continue
            start_line = (name_span or enclosing_span)[0]
            end_line = (enclosing_span or name_span)[1]
            if end_line < start_line:
                end_line = start_line

            entry = {
                "name": name,
                "kind": neutral,
                "file": rel_path,
                "start_line": start_line,
                "end_line": end_line,
                "owner": owner,
                "visibility": python_visibility(name),
                "flags": python_flags(rel_path, parsed, doc_head),
            }
            if entry["kind"] in (excluded_kinds or ()):
                stats.dropped["kind:%s" % entry["kind"]] += 1
                continue
            symbols.append(entry)
            stats.kinds[neutral] += 1
            stats.packages_kept[parsed.package] += 1

    symbols.sort(key=lambda s: (s["file"], s["start_line"], s["name"], s["kind"],
                                s["owner"] or "", s["end_line"]))
    deduped = []
    seen = set()
    for s in symbols:
        key = (s["file"], s["start_line"], s["end_line"], s["name"], s["kind"], s["owner"])
        if key in seen:
            stats.dropped["duplicate"] += 1
            continue
        seen.add(key)
        deduped.append(s)
    return deduped, stats


# --------------------------------------------------------------------------
# TypeScript classification (scip-typescript)
# --------------------------------------------------------------------------

# scip-typescript 0.4.0, like scip-python and unlike rust-analyzer, emits NO
# `kind` and no `display_name` on SymbolInformation: an entry carries only
# {symbol, documentation, relationships}. Unlike Python, though, the descriptor
# suffix is NOT sufficient here: a `#` (type) descriptor covers `class`,
# `interface`, `type X = ...` AND `enum` alike, and TypeScript is the one corpus
# language whose neutral kinds have to tell those four apart. So for TypeScript
# the PRIMARY kind signal is the rendered ```ts documentation block, and the
# descriptor suffix is the SECONDARY, corroborating one. See POLICY.md §11.2.
TS_FENCE_RE = re.compile(r"^```ts\n(.*?)\n?```\s*$", re.S)

# Descriptor suffix -> the coarse class the symbol string can vouch for. The
# rendered doc head must land inside the same class or the two signals disagree.
TS_DESCRIPTOR_CLASS = {
    "type": "type-ish",       # `X#`   — class | interface | type_alias | enum
    "method": "callable",     # `f().` — function | method | ctor | accessor
    "term": "binding",        # `x.`   — const/let/var, property, enum member
    "namespace": "namespace",  # `N/`  — a `namespace`/`module` block, or the file
    "macro": None,            # `t0:`  — SCIP meta: object/type-literal scopes
    "parameter": None,
    "type_parameter": None,
}

# Rendered declaration head -> (neutral kind, coarse class). `None` neutral means
# "recognised, and deliberately not a golden symbol".
TS_DOC_HEAD_RULES = (
    (re.compile(r"^type\b"), "type_alias", "type-ish"),
    (re.compile(r"^interface\b"), "interface", "type-ish"),
    (re.compile(r"^(abstract\s+)?class\b"), "class", "type-ish"),
    (re.compile(r"^(const\s+)?enum\b"), "enum", "type-ish"),
    (re.compile(r"^(declare\s+)?function\b"), "function", "callable"),
    (re.compile(r"^\(method\)"), "method", "callable"),
    (re.compile(r"^constructor\b"), "method", "callable"),
    (re.compile(r"^(get|set)\s+\S"), "method", "callable"),
    (re.compile(r"^module\s+[\"']"), None, "namespace"),     # the per-file module
    (re.compile(r"^(var|const|let)\b"), None, "binding"),
    (re.compile(r"^\((property|parameter|enum member|local [a-z ]+)\)"), None, "binding"),
)

TS_IDENTIFIER_RE = re.compile(r"^[A-Za-z_$][A-Za-z0-9_$]*$")
TS_TEST_BASENAME_GLOBS = ("*.test.ts", "*.test.tsx", "*.test.js", "*.test.jsx",
                          "*.spec.ts", "*.spec.tsx", "*.spec.js", "*.spec.jsx")
TS_TEST_DIR_COMPONENTS = ("test", "tests", "__tests__", "runtime-tests", "spec", "specs")


def ts_doc_head(sym_info):
    """First line of the rendered ```ts``` block, or "" when it says nothing."""
    docs = (sym_info or {}).get("documentation") or []
    if not docs:
        return ""
    first = (docs[0] or "").strip()
    m = TS_FENCE_RE.match(first)
    body = m.group(1) if m else first
    for line in body.split("\n"):
        line = line.strip()
        if line:
            return line
    return ""


def ts_doc_kind(head):
    """(neutral kind, coarse class) implied by a rendered head; (None, None) if silent."""
    for rx, neutral, klass in TS_DOC_HEAD_RULES:
        if rx.match(head):
            return neutral, klass
    return None, None


def ts_is_file_module(parsed, rel_path):
    """True for the one `<dir>/`<file.ts>`/` pseudo-symbol emitted per document.

    scip-typescript models every file as a module, so a `namespace`-final symbol
    is ambiguous: it is either that per-file pseudo-symbol or a real
    `namespace X {}` / `declare namespace X {}` block, which loregrep DOES emit
    (`TypeKind::Namespace`). They are told apart structurally: the file module's
    trailing descriptor is the document's own basename. Getting this wrong in
    either direction is expensive — see POLICY.md §11.4.
    """
    if not parsed.descriptors or parsed.descriptors[-1].kind != "namespace":
        return False
    return parsed.descriptors[-1].name == rel_path.rsplit("/", 1)[-1]


def ts_normalize_name(raw):
    """`<constructor>` -> constructor, `<get>url` / `<set>url` -> url.

    scip-typescript spells accessors and constructors with an angle-bracketed
    role prefix that is not an identifier anyone writes or searches for.
    """
    if raw == "<constructor>":
        return "constructor"
    m = re.match(r"^<(get|set)>(.+)$", raw)
    return m.group(2) if m else raw


def ts_owner(descriptors):
    """Nearest enclosing type descriptor (`Cls#m().` -> Cls), or None.

    Interfaces own their method signatures exactly as classes own their methods;
    both are `<Type>#<member>().` in the symbol string.
    """
    for d in reversed(descriptors[:-1]):
        if d.kind == "type":
            return d.name or None
    return None


def ts_is_module_level_term(parsed):
    """A `.` descriptor whose only ancestors are namespaces (file/`namespace X {}`).

    Excludes class fields and interface/type-literal properties (`Cls#x.`,
    `T#typeLiteral0:x.`) and enum members (`E#V.`), none of which loregrep's
    TypeScript analyzer emits as a definition.
    """
    return all(d.kind == "namespace" for d in parsed.descriptors[:-1])


def ts_is_test_path(rel_path):
    parts = rel_path.split("/")
    if any(p in TS_TEST_DIR_COMPONENTS for p in parts[:-1]):
        return True
    return any(fnmatch.fnmatch(parts[-1], g) for g in TS_TEST_BASENAME_GLOBS)


def ts_rendered_type_is_callable(head):
    """True when `var x: <type>` renders a bare function type.

    Corroborating signal only (see POLICY.md §11.2): it is right about arrows
    whose type is written inline and wrong about arrows annotated with a named
    alias (`const h: MiddlewareHandler = (c) => …`), which is why it does not
    decide inclusion.
    """
    if ":" not in head:
        return False
    t = head.split(":", 1)[1].strip()
    if t.startswith("<"):  # generic arrow: <T>(x: T) => T
        depth = 0
        for i, ch in enumerate(t):
            if ch == "<":
                depth += 1
            elif ch == ">":
                depth -= 1
                if depth == 0:
                    t = t[i + 1:].strip()
                    break
        else:
            return False
    if not t.startswith("("):
        return False
    depth = 0
    for i, ch in enumerate(t):
        if ch == "(":
            depth += 1
        elif ch == ")":
            depth -= 1
            if depth == 0:
                return t[i + 1:].lstrip().startswith("=>")
    return False


def infer_local_packages_typescript(index):
    """Packages whose source is in this checkout.

    scip-typescript stamps every non-local symbol with
    `npm <package> <version>`; an npm dependency pulled in from node_modules is
    an ordinary symbol under a different package. A package owns a per-file
    module pseudo-symbol only for files it actually contributed to the index, so
    "owns a file module" is exactly "its source was indexed here". Falls back to
    "every package with any definition"; `--package` overrides.
    """
    module_owners = set()
    any_owners = set()
    for doc in index.get("documents", []):
        rel_path = (doc.get("relative_path") or "").replace(os.sep, "/")
        for occ in doc.get("occurrences", []):
            if not occ.get("symbol_roles", 0) & ROLE_DEFINITION:
                continue
            sym = occ.get("symbol", "")
            if sym.startswith("local "):
                continue
            p = ParsedSymbol(sym)
            if not p.package:
                continue
            any_owners.add(p.package)
            if ts_is_file_module(p, rel_path):
                module_owners.add(p.package)
    return module_owners or any_owners


def convert_typescript(index, excluded_paths=None, packages=None, stats=None,
                       repo_root=None, excluded_kinds=()):
    """scip-typescript index -> neutral definition golden. See POLICY.md §11."""
    stats = stats or Stats()
    excluded = tuple(excluded_paths or ())
    allowed_packages = set(packages) if packages else infer_local_packages_typescript(index)

    symbols = []
    for doc in index.get("documents", []):
        rel_path = (doc.get("relative_path") or "").replace(os.sep, "/")
        while rel_path.startswith("./"):
            rel_path = rel_path[2:]
        if any(rel_path == e or rel_path.startswith(e.rstrip("/") + "/") for e in excluded):
            stats.dropped["excluded_path_file"] += 1
            stats.dropped["excluded_path"] += sum(
                1 for o in doc.get("occurrences", [])
                if o.get("symbol_roles", 0) & ROLE_DEFINITION
                and not (o.get("symbol") or "").startswith("local ")
            )
            continue
        info = {s["symbol"]: s for s in doc.get("symbols", [])}

        for occ in doc.get("occurrences", []):
            if not occ.get("symbol_roles", 0) & ROLE_DEFINITION:
                continue  # a reference, not a definition
            raw = occ.get("symbol", "")
            if raw.startswith("local "):
                stats.dropped["local"] += 1
                continue
            parsed = ParsedSymbol(raw)
            if not parsed.descriptors:
                stats.dropped["unparseable_symbol"] += 1
                continue
            if parsed.package not in allowed_packages:
                stats.dropped["external_package"] += 1
                stats.packages_dropped[parsed.package] += 1
                continue

            descriptor_kind = parsed.descriptors[-1].kind
            klass = TS_DESCRIPTOR_CLASS.get(descriptor_kind)
            if klass is None:
                stats.dropped["descriptor:%s" % descriptor_kind] += 1
                continue

            head = ts_doc_head(info.get(raw, {}))
            doc_neutral, doc_class = ts_doc_kind(head)
            if doc_class is not None and doc_class != klass:
                stats.disagreements["descriptor:%s vs doc:%s" % (descriptor_kind, doc_class)] += 1

            has_body = occ.get("enclosing_range") is not None
            neutral = None
            if klass == "namespace":
                # The per-file module pseudo-symbol is NOT a `namespace`
                # declaration; a real `namespace X {}` block is. §11.4.
                if ts_is_file_module(parsed, rel_path):
                    stats.dropped["file_module"] += 1
                    continue
                if not TS_IDENTIFIER_RE.match(parsed.descriptors[-1].name or ""):
                    # `declare module '../..' {}` — the "name" is a module
                    # specifier string, not an identifier. §11.4.
                    stats.dropped["quoted_module_declaration"] += 1
                    continue
                neutral = "namespace"
            elif klass == "type-ish":
                # ONLY the rendered head can separate class/interface/type_alias/enum.
                neutral = doc_neutral
                if neutral is None:
                    stats.dropped["type_unclassified"] += 1
                    continue
            elif klass == "callable":
                neutral = "function"
            elif klass == "binding":
                if not ts_is_module_level_term(parsed):
                    stats.dropped["member_binding"] += 1
                    continue
                # A module-level binding is a *definition* only when its
                # initializer is a function-like expression, which is exactly
                # when scip-typescript gives the occurrence an `enclosing_range`
                # (a plain `const N = 1` or a re-export alias gets none). The
                # rendered type is the corroborating signal; §11.2 records the
                # 107 hono cases where the two differ and why the structural one
                # is right in both directions.
                if ts_rendered_type_is_callable(head) != has_body:
                    stats.disagreements["term:enclosing_range=%s vs rendered_fn_type=%s"
                                        % (has_body, not has_body)] += 1
                if not has_body:
                    stats.dropped["binding"] += 1
                    continue
                neutral = "function"

            name = ts_normalize_name(parsed.descriptors[-1].name or "")
            if not name:
                stats.dropped["anonymous"] += 1
                continue

            owner = None
            if neutral in ("function", "method"):
                owner = ts_owner(parsed.descriptors)
                neutral = "method" if owner else "function"

            # Span convention, identical to Rust (§4) and Python (§10.5):
            # start_line is the DECLARATION line from the symbol's own name
            # occurrence, end_line from `enclosing_range`.
            name_span = range_to_lines(occ.get("range"))
            enclosing_span = range_to_lines(occ.get("enclosing_range") or None)
            if name_span is None and enclosing_span is None:
                stats.dropped["no_range"] += 1
                continue
            start_line = (name_span or enclosing_span)[0]
            end_line = (enclosing_span or name_span)[1]
            if end_line < start_line:
                end_line = start_line

            flags = []
            if ts_is_test_path(rel_path):
                flags.append("test")
            if rel_path.endswith(".d.ts"):
                flags.append("ambient")

            entry = {
                "name": name,
                "kind": neutral,
                "file": rel_path,
                "start_line": start_line,
                "end_line": end_line,
                "owner": owner,
                # scip-typescript renders neither `export` nor
                # `private`/`protected`; abstaining beats guessing. §11.5.
                "visibility": "unknown",
                "flags": sorted(set(flags)),
            }
            if entry["kind"] in (excluded_kinds or ()):
                stats.dropped["kind:%s" % entry["kind"]] += 1
                continue
            symbols.append(entry)
            stats.kinds[neutral] += 1
            stats.packages_kept[parsed.package] += 1

    symbols.sort(key=lambda s: (s["file"], s["start_line"], s["name"], s["kind"],
                                s["owner"] or "", s["end_line"]))
    deduped = []
    seen = set()
    for s in symbols:
        key = (s["file"], s["start_line"], s["end_line"], s["name"], s["kind"], s["owner"])
        if key in seen:
            stats.dropped["duplicate"] += 1
            continue
        seen.add(key)
        deduped.append(s)
    return deduped, stats


# Per-language policy defaults recorded in the golden's `policy` block.
#
# include_nested_functions differs by language because the ORACLE differs, not
# because the languages do: rust-analyzer gives an inner `fn` a `local` symbol
# (so the Rust golden structurally cannot contain one), while scip-python gives
# an inner `def` a fully-qualified symbol (`create_app().index().`). Declaring
# `true` for Python is therefore a statement of fact about the golden's contents;
# declaring `false` would make the scorer strip loregrep's inner functions from
# the precision denominator while the golden still demanded them, i.e. it would
# manufacture false negatives. See POLICY.md §10.
#
# scip-typescript behaves like rust-analyzer here: every definition inside a
# function body is a `local N` symbol, so a TypeScript golden structurally
# cannot contain a nested function either. See POLICY.md §11.6.
NESTED_FUNCTIONS_INCLUDED = {"rust": False, "python": True, "typescript": False}

# Kinds outside the definition-parity contract, per language. Rust `mod` and
# Python modules are file properties loregrep never surfaces as searchable
# symbols; TypeScript `namespace X {}` IS surfaced (`TypeKind::Namespace`), so
# TypeScript excludes nothing by kind — its per-file module pseudo-symbols are
# dropped structurally instead, before they ever become a `namespace`. §11.4.
EXCLUDED_KINDS_BY_LANGUAGE = {
    "rust": EXCLUDED_KINDS,
    "python": EXCLUDED_KINDS,
    "typescript": (),
}


def build_golden(index, args, stats=None):
    excluded = list(args.exclude or [])
    excluded_kinds = getattr(args, "excluded_kinds", None)
    if excluded_kinds is None:
        excluded_kinds = EXCLUDED_KINDS_BY_LANGUAGE.get(args.language, EXCLUDED_KINDS)
    symbols, stats = convert(
        index,
        args.language,
        excluded_paths=excluded,
        packages=args.package,
        stats=stats,
        excluded_kinds=excluded_kinds,
        repo_root=getattr(args, "repo_root", None),
    )
    tool = (index.get("metadata") or {}).get("tool_info") or {}
    generated_at = os.environ.get("SOURCE_DATE_EPOCH")
    if generated_at:
        ts = datetime.datetime.fromtimestamp(int(generated_at), datetime.timezone.utc).replace(tzinfo=None)
    else:
        ts = datetime.datetime.now(datetime.timezone.utc).replace(tzinfo=None)
    golden = {
        "schema_version": SCHEMA_VERSION,
        "repo": args.repo,
        "commit": args.commit,
        "language": args.language,
        "generator": {
            "indexer": args.indexer or tool.get("name") or "unknown",
            "indexer_version": args.indexer_version or tool.get("version") or "unknown",
            "scip_cli_version": args.scip_cli_version or "unknown",
            "converter_version": CONVERTER_VERSION,
            "generated_at": ts.replace(microsecond=0).isoformat() + "Z",
        },
        "policy": {
            "include_nested_functions": NESTED_FUNCTIONS_INCLUDED.get(args.language, False),
            "include_test_symbols": True,
            "include_generated": False,
            # Rust `mod` declarations and Python modules. loregrep models a module
            # as a property of a file (TreeNode.declared_modules for Rust; nothing
            # at all for Python, whose analyzer emits only classes and functions),
            # never as a searchable symbol, so a module is out of the
            # definition-parity contract rather than a miss. Revisit if a module
            # ever becomes addressable by a tool. Empty for TypeScript, whose
            # `namespace X {}` loregrep DOES emit (POLICY.md §11.4).
            "excluded_kinds": list(excluded_kinds or []),
            "excluded_paths": excluded,
        },
        "symbols": symbols,
    }
    return golden, stats


# --------------------------------------------------------------------------
# I/O
# --------------------------------------------------------------------------


def scip_cli_version(scip_bin):
    try:
        out = subprocess.run([scip_bin, "--version"], capture_output=True, text=True, timeout=30)
    except (OSError, subprocess.SubprocessError):
        return None
    text = (out.stdout or out.stderr or "").strip()
    m = re.search(r"v?\d+\.\d+\.\d+", text)
    return m.group(0) if m else (text or None)


def load_index(args):
    if args.scip_json:
        with open(args.scip_json, "r") as fh:
            return json.load(fh)
    path = args.scip
    if path.endswith(".json"):
        with open(path, "r") as fh:
            return json.load(fh)
    proc = subprocess.run(
        [args.scip_bin, "print", "--json", path], capture_output=True, text=True
    )
    if proc.returncode != 0:
        raise ConversionError(
            "%s print --json failed (exit %d): %s" % (args.scip_bin, proc.returncode, proc.stderr[-2000:])
        )
    return json.loads(proc.stdout)


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    src = ap.add_mutually_exclusive_group(required=True)
    src.add_argument("--scip", help="Path to a .scip index (or an already-converted .json)")
    src.add_argument("--scip-json", help="Path to `scip print --json` output (no scip CLI needed)")
    ap.add_argument("--repo", required=True, help="Corpus repo id (directory name under evals/corpus/)")
    ap.add_argument("--commit", required=True, help="40-hex pinned commit the index was built from")
    ap.add_argument("--language", required=True, choices=["rust", "python", "typescript", "javascript"])
    ap.add_argument("--out", required=True, help="Output golden-symbols.json path ('-' for stdout)")
    ap.add_argument("--scip-bin", default=os.path.join("evals", ".tools", "scip"))
    ap.add_argument("--package", action="append", help="In-repo package name (repeatable). Default: inferred.")
    ap.add_argument("--exclude", action="append", help="Repo-relative path prefix to exclude (repeatable)")
    ap.add_argument(
        "--repo-root",
        help="Checkout the index was built from. Optional; only used to detect "
        "#[cfg(test)] modules whose name is not `tests`.",
    )
    ap.add_argument("--indexer", help="Override generator.indexer")
    ap.add_argument("--indexer-version", help="Override generator.indexer_version")
    ap.add_argument("--scip-cli-version", help="Override generator.scip_cli_version")
    ap.add_argument("--stats", action="store_true", help="Print a conversion report to stderr")
    args = ap.parse_args(argv)

    if not re.fullmatch(r"[0-9a-f]{40}", args.commit or ""):
        ap.error("--commit must be a 40-character lowercase hex sha")

    if not args.scip_cli_version:
        args.scip_cli_version = scip_cli_version(args.scip_bin) or "unknown"

    index = load_index(args)
    golden, stats = build_golden(index, args)

    text = json.dumps(golden, indent=2, sort_keys=False) + "\n"
    if args.out == "-":
        sys.stdout.write(text)
    else:
        d = os.path.dirname(os.path.abspath(args.out))
        if d and not os.path.isdir(d):
            os.makedirs(d)
        with open(args.out, "w") as fh:
            fh.write(text)

    if args.stats:
        err = sys.stderr
        err.write("symbols: %d\n" % len(golden["symbols"]))
        err.write("by kind:\n")
        for k, v in sorted(stats.kinds.items(), key=lambda kv: (-kv[1], kv[0])):
            err.write("  %-12s %d\n" % (k, v))
        err.write("dropped:\n")
        for k, v in sorted(stats.dropped.items(), key=lambda kv: (-kv[1], kv[0])):
            err.write("  %-24s %d\n" % (k, v))
        if stats.disagreements:
            err.write("kind-signal disagreements (scip kind vs signature):\n")
            for k, v in sorted(stats.disagreements.items()):
                err.write("  %-28s %d\n" % (k, v))
        err.write("packages kept: %s\n" % dict(stats.packages_kept))
        if stats.packages_dropped:
            err.write("packages dropped: %s\n" % dict(stats.packages_dropped))
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except ConversionError as exc:
        sys.stderr.write("error: %s\n" % exc)
        sys.exit(2)
