#!/usr/bin/env python3
"""Rewrite a freshly-moved `packages/feature/<name>` so it compiles as a crate.

Run after the pure `git mv` commit (SPEC-package-by-feature.md section 13.1),
once per feature:

    scripts/rewrite-feature-package.py similarity similarity_report duplicate_report

It performs the rewrites the F6 pilot established, in an order that matters:
cross-package paths first, then the slice's own layer paths, then visibility,
then the imports that `src/presentation/cli.rs` used to supply ambiently.

That last one is the part unique to features. Core packages were largely
self-contained; a feature's `cli/` files inherited `anyhow::Result`,
`clap::Args`, `serde_json::json!`, `PathBuf`, `DialectArg`, `OutputFormat` and
`safe_text!` from a glob import and a textually-scoped `macro_rules!` in
cli.rs, without ever naming them. Crossing a crate boundary makes every one of
those explicit.

It does NOT write the README, the manifest, or the facade: those need
judgement. It prints what remains to be done.
"""

from __future__ import annotations

import argparse
import collections
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent

# Modules that already left the root crate, and who owns them now.
OWNER: dict[str, str] = {}
for _m in ("sexpr dialect common_lisp definition leading_trivia expression_equality "
           "form_shape graph view_query").split():
    OWNER[_m] = "paredit_core_syntax"
for _m in "semantics lexical_scope binding_index callable_scope definition_reference".split():
    OWNER[_m] = "paredit_core_semantics"
for _m in ("mutation_safety refactor_plan refactor_preview refactor_execute extract_shared "
           "let_binding progn local_function_binding let_composition let_star_composition "
           "flet_composition convert_control").split():
    OWNER[_m] = "paredit_core_edit"
for _m in "workspace fs_identity".split():
    OWNER[_m] = "paredit_core_workspace"
for _m in "args shared gate".split():
    OWNER[_m] = "paredit_core_cli"

# Names a feature's cli/ files used without importing, because cli.rs had them
# in scope. `safe_text` is a macro, so it needs a `use` to be callable bare.
ARGS_ITEMS = [
    "DialectArg", "OutputFormat", "MoveInsert", "ParameterInsert", "ThreadStyleArg",
    "SourceInput", "EditTargetArgs", "TargetArgs", "AnalyzeArgs", "FormatArgs",
    "RepairArgs", "ReplaceArgs", "WrapArgs", "WrapDelimiter",
]
SHARED_ITEMS = [
    "MAX_SOURCE_INPUT_BYTES", "apply_byte_span_edits", "bounded_preview",
    "matching_symbol_occurrences", "read_input_and_dialect", "read_input_dialect_and_tree",
    "read_text_file_with_limit", "read_text_with_limit", "require_output_file",
    "resolve_target", "stable_text_hash", "terminal_safe", "terminal_safe_error_chain",
    "unified_diff", "write_artifact_with_rollback", "write_file_with_rollback",
    "write_files_with_rollback",
]

AMBIENT = [
    (r"\bResult<", "use anyhow::Result;", "Result"),
    (r"#\[derive\([^)]*\bArgs\b", "use clap::Args;", "Args"),
    (r"\bjson!", "use serde_json::json;", "json"),
    (r"\bPathBuf\b", "use std::path::PathBuf;", "PathBuf"),
    (r"\bDialectArg\b", "use paredit_core_cli::args::DialectArg;", "DialectArg"),
    (r"\bOutputFormat\b", "use paredit_core_cli::args::OutputFormat;", "OutputFormat"),
    (r"\bsafe_text!", "use paredit_core_cli::safe_text;", "safe_text"),
    # Everything cli.rs re-exported from `shared`, which every feature's
    # workflow used without importing.
    (r"\bMAX_SOURCE_INPUT_BYTES\b", "use paredit_core_cli::shared::MAX_SOURCE_INPUT_BYTES;", "MAX_SOURCE_INPUT_BYTES"),
    (r"\bapply_byte_span_edits\b", "use paredit_core_cli::shared::apply_byte_span_edits;", "apply_byte_span_edits"),
    (r"\bbounded_preview\b", "use paredit_core_cli::shared::bounded_preview;", "bounded_preview"),
    (r"\bmatching_symbol_occurrences\b", "use paredit_core_cli::shared::matching_symbol_occurrences;", "matching_symbol_occurrences"),
    (r"\bread_input_and_dialect\b", "use paredit_core_cli::shared::read_input_and_dialect;", "read_input_and_dialect"),
    (r"\bread_input_dialect_and_tree\b", "use paredit_core_cli::shared::read_input_dialect_and_tree;", "read_input_dialect_and_tree"),
    (r"\bread_text_file_with_limit\b", "use paredit_core_cli::shared::read_text_file_with_limit;", "read_text_file_with_limit"),
    (r"\bread_text_with_limit\b", "use paredit_core_cli::shared::read_text_with_limit;", "read_text_with_limit"),
    (r"\brequire_output_file\b", "use paredit_core_cli::shared::require_output_file;", "require_output_file"),
    (r"\bresolve_target\b", "use paredit_core_cli::shared::resolve_target;", "resolve_target"),
    (r"\bstable_text_hash\b", "use paredit_core_cli::shared::stable_text_hash;", "stable_text_hash"),
    (r"\bterminal_safe\b", "use paredit_core_cli::shared::terminal_safe;", "terminal_safe"),
    (r"\bterminal_safe_error_chain\b", "use paredit_core_cli::shared::terminal_safe_error_chain;", "terminal_safe_error_chain"),
    (r"\bunified_diff\b", "use paredit_core_cli::shared::unified_diff;", "unified_diff"),
    (r"\bwrite_artifact_with_rollback\b", "use paredit_core_cli::shared::write_artifact_with_rollback;", "write_artifact_with_rollback"),
    (r"\bwrite_file_with_rollback\b", "use paredit_core_cli::shared::write_file_with_rollback;", "write_file_with_rollback"),
    (r"\bwrite_files_with_rollback\b", "use paredit_core_cli::shared::write_files_with_rollback;", "write_files_with_rollback"),
    # cli.rs's own top-level imports, equally inherited by every feature.
    (r"\bByteOffset\b", "use paredit_core_syntax::sexpr::ByteOffset;", "ByteOffset"),
    (r"\bByteSpan\b", "use paredit_core_syntax::sexpr::ByteSpan;", "ByteSpan"),
    (r"\bPath\b", "use paredit_core_syntax::sexpr::Path;", "Path"),
    (r"\bSymbolName\b", "use paredit_core_syntax::sexpr::SymbolName;", "SymbolName"),
    (r"\bSyntaxTree\b", "use paredit_core_syntax::sexpr::SyntaxTree;", "SyntaxTree"),
    (r"\bDialect\b", "use paredit_core_syntax::dialect::Dialect;", "Dialect"),
    (r"\bDefinitionCategory\b", "use paredit_core_syntax::definition::DefinitionCategory;", "DefinitionCategory"),
    (r"\bWorkspaceDiscoveryOptions\b", "use paredit_core_workspace::workspace::WorkspaceDiscoveryOptions;", "WorkspaceDiscoveryOptions"),
    (r"\bdiscover_workspace_files\b", "use paredit_core_workspace::workspace::discover_workspace_files;", "discover_workspace_files"),
    (r"\bAnalyzeArgs\b", "use paredit_core_cli::args::AnalyzeArgs;", "AnalyzeArgs"),
    (r"\bFormatArgs\b", "use paredit_core_cli::args::FormatArgs;", "FormatArgs"),
    (r"\bRepairArgs\b", "use paredit_core_cli::args::RepairArgs;", "RepairArgs"),
    (r"\bTargetArgs\b", "use paredit_core_cli::args::TargetArgs;", "TargetArgs"),
    (r"\bEditTargetArgs\b", "use paredit_core_cli::args::EditTargetArgs;", "EditTargetArgs"),
    (r"\bReplaceArgs\b", "use paredit_core_cli::args::ReplaceArgs;", "ReplaceArgs"),
    (r"\bWrapArgs\b", "use paredit_core_cli::args::WrapArgs;", "WrapArgs"),
    (r"\bWrapDelimiter\b", "use paredit_core_cli::args::WrapDelimiter;", "WrapDelimiter"),
    (r"\bMoveInsert\b", "use paredit_core_cli::args::MoveInsert;", "MoveInsert"),
    (r"\bParameterInsert\b", "use paredit_core_cli::args::ParameterInsert;", "ParameterInsert"),
    (r"\bThreadStyleArg\b", "use paredit_core_cli::args::ThreadStyleArg;", "ThreadStyleArg"),
    (r"\bSourceInput\b", "use paredit_core_cli::args::SourceInput;", "SourceInput"),
    (r"\bParser\b", "use clap::Parser;", "Parser"),
    (r"\bValueEnum\b", "use clap::ValueEnum;", "ValueEnum"),
    (r"\bContext\b", "use anyhow::Context;", "Context"),
    (r"\bValue\b", "use serde_json::Value;", "Value"),
]



def split_grouped_layer_imports(text: str, slices: list[str]) -> tuple[str, int]:
    """Expand `use crate::<layer>::{a::.., b::..};` into one `use` per entry.

    The single-line rewrites cannot see a module name that sits on a later
    line, so a grouped import survives them intact and then fails to resolve.
    Splitting first lets every other rule apply normally.
    """
    # Both the multi-line form and the single-line one; neither puts a module
    # name on the same line as `crate::<layer>::` in a way the other rules see.
    pattern = re.compile(
        r"^use crate::(domain|application::usecase|presentation::cli)::\{\n(.*?)^\};\n"
        r"|^use crate::(domain|application::usecase|presentation::cli)::\{([^}\n]*)\};\n",
        re.M | re.S,
    )
    count = 0

    def expand(match: re.Match[str]) -> str:
        nonlocal count
        layer = match.group(1) or match.group(3)
        body = match.group(2) if match.group(1) else match.group(4)
        entries, depth, current = [], 0, ""
        for ch in body:
            if ch == "{":
                depth += 1
            elif ch == "}":
                depth -= 1
            if ch == "," and depth == 0:
                entries.append(current.strip())
                current = ""
            else:
                current += ch
        if current.strip():
            entries.append(current.strip())
        out = []
        for entry in entries:
            if not entry:
                continue
            count += 1
            out.append(f"use crate::{layer}::{entry};")
        return "\n".join(out) + "\n"

    return pattern.sub(expand, text), count


def rewrite(name: str, slices: list[str]) -> int:
    src = ROOT / "packages" / "feature" / name / "src"
    if not src.is_dir():
        print(f"  ERROR: {src} does not exist - run the git mv first")
        return 1
    crate = f"paredit_feature_{name.replace('-', '_')}"
    counts: collections.Counter = collections.Counter()

    for path in src.rglob("*.rs"):
        text = original = path.read_text()

        # 0. Grouped multi-line imports first: every rule below matches a module
        #    name on the same line as `crate::<layer>::`, which a grouped import
        #    does not have.
        text, n = split_grouped_layer_imports(text, slices)
        counts["grouped_imports"] += n

        # 1. Cross-package paths FIRST. The layer-collapsing rules below would
        #    otherwise claim a module this package does not own.
        for module, owner in OWNER.items():
            for layer in ("domain", "infrastructure", "presentation::cli", "application::usecase"):
                text, n = re.subn(rf"\bcrate::{layer}::{module}\b", f"{owner}::{module}", text)
                counts["cross_package"] += n
        text, n = re.subn(r"\bcrate::domain::lint\b", "paredit_core_lint_engine", text)
        counts["cross_package"] += n

        # 2. This package's own slices: the three layer paths collapse into the
        #    slice directory, which is the whole point of the slice-first layout.
        for slice_ in slices:
            for layer, dest in (("domain", "domain"),
                                ("application::usecase", "usecase"),
                                ("presentation::cli", "cli")):
                text, n = re.subn(rf"\bcrate::{layer}::{slice_}\b",
                                  f"crate::{slice_}::{dest}", text)
                counts["own_slices"] += n
            # `use crate::domain::x::{self, ..}` bound the name `x`. After the
            # collapse `self` would bind `domain`, silently orphaning every
            # unqualified `x::` call site in the file.
            for dest in ("domain", "usecase", "cli"):
                text, n = re.subn(rf"use crate::{slice_}::{dest}::\{{self,",
                                  f"use crate::{slice_}::{dest}::{{self as {slice_},", text)
                counts["self_alias"] += n
                text, n = re.subn(rf"^use crate::{slice_}::{dest};$",
                                  f"use crate::{slice_}::{dest} as {slice_};",
                                  text, flags=re.M)
                counts["self_alias"] += n

        # 3. Anything still addressing the old cli root is a shared helper...
        text, n = re.subn(r"\bcrate::presentation::cli\b", "paredit_core_cli::shared", text)
        counts["cli_root"] += n
        # ...but cli.rs re-exported from BOTH args and shared, so the value
        # enums have to be sent to args instead.
        def split_shared_group(match: re.Match[str]) -> str:
            names = [n.strip() for n in match.group(1).split(",") if n.strip()]
            args = [n for n in names if n.split(" as ")[0].strip() in ARGS_ITEMS]
            rest = [n for n in names if n not in args]
            out = []
            if rest:
                out.append(f"use paredit_core_cli::shared::{{{', '.join(rest)}}};")
            if args:
                out.append(f"use paredit_core_cli::args::{{{', '.join(args)}}};")
            return "\n".join(out)

        text, n = re.subn(r"use paredit_core_cli::shared::\{([^}]*)\};",
                          split_shared_group, text)
        counts["cli_args"] += n
        for name in ARGS_ITEMS:
            text, n = re.subn(rf"\bparedit_core_cli::shared::{name}\b",
                              f"paredit_core_cli::args::{name}", text)
            counts["cli_args"] += n
        # A file inside a cli subdirectory reached those same helpers through
        # `super::super::`, which names the old cli module and no longer exists.
        text, n = re.subn(r"use super::super::\{([^}]*)\};",
                          lambda m: "use paredit_core_cli::shared::{" + m.group(1) + "};",
                          text)
        counts["cli_super"] += n
        for name in SHARED_ITEMS:
            text, n = re.subn(rf"\bsuper::super::{name}\b",
                              f"paredit_core_cli::shared::{name}", text)
            counts["cli_super"] += n
        # ...but a visibility cannot name another crate, so undo it there.
        text, n = re.subn(r"pub\(in paredit_core_cli::shared\)", "pub", text)
        counts["visibility_fix"] += n

        # 4. Visibility that cannot cross a crate boundary.
        text, n = re.subn(r"\bpub\(crate\)", "pub", text)
        counts["pub_crate"] += n
        text, n = re.subn(r"\bpub\(super\)", "pub", text)
        counts["pub_super"] += n
        # ...except a glob, which re-exports nothing at `pub` and is rejected.
        text, n = re.subn(r"^(\s*)pub use super::\*;$", r"\1pub(super) use super::*;",
                          text, flags=re.M)
        counts["glob_narrowed"] += n
        # A `pub use` cannot re-export an item that is `pub(in ...)`. Those
        # paths also no longer name anything after the layer collapse.
        text, n = re.subn(r"\bpub\(in crate::[a-z_0-9:]+\)", "pub", text)
        counts["pub_in"] += n

        # 4b. Safety net: any `crate::<m>` still naming a module another package
        #     owns. Earlier rules key on `crate::<layer>::<m>`, so a path that
        #     lost its layer segment some other way slips past them.
        for module, owner in OWNER.items():
            if module in slices:
                continue
            text, n = re.subn(rf"\bcrate::{module}::", f"{owner}::{module}::", text)
            counts["stray_paths"] += n

        # 5. Doctests can only import the crate they compile into.
        text, n = re.subn(r"\bparedit_cli::", f"{crate}::", text)
        counts["doctests"] += n

        # Several rules can converge on the same import; keep the first.
        seen, kept = set(), []
        for line in text.splitlines(keepends=True):
            if line.startswith("use ") and line.rstrip("\n").endswith(";"):
                if line in seen:
                    counts["deduped"] += 1
                    continue
                seen.add(line)
            kept.append(line)
        text = "".join(kept)

        if text != original:
            path.write_text(text)

    # 6. The ambient imports, added only where the name is actually used.
    for path in src.rglob("*.rs"):
        text = path.read_text()
        missing = []
        for pattern, import_line, symbol in AMBIENT:
            if not re.search(pattern, text):
                continue
            if import_line in text:
                continue
            if re.search(rf"use [A-Za-z_:]*\{{[^}}]*\b{symbol}\b", text):
                continue
            # Never shadow a definition in this very file: `inline_function`
            # defines its own `apply_byte_span_edits`, and importing core's
            # alongside it is an E0255 name collision.
            if re.search(rf"\b(fn|struct|enum|trait|type|const|static)\s+{symbol}\b", text):
                continue
            missing.append(import_line)
        if missing:
            lines = text.splitlines(keepends=True)
            at = next((i for i, l in enumerate(lines) if l.startswith("use ")), None)
            if at is None:
                # No existing `use` to anchor on. Inserting at 0 would put the
                # imports above the module's `//!` inner doc comment, which must
                # come first (E0753).
                at = 0
                while at < len(lines) and (
                    lines[at].startswith(("//!", "#!["))
                    or lines[at].strip() == ""
                    or lines[at].lstrip().startswith("//")
                ):
                    at += 1
            for line in reversed(missing):
                lines.insert(at, line + "\n")
            path.write_text("".join(lines))
            counts["ambient_imports"] += len(missing)

    print(f"  {dict(counts)}")

    leftovers: collections.Counter = collections.Counter()
    for path in src.rglob("*.rs"):
        for hit in re.findall(
            r"crate::(?:domain|application|infrastructure|presentation)::[a-z_0-9]+",
            path.read_text(),
        ):
            leftovers[hit] += 1
    if leftovers:
        print("  STILL UNRESOLVED - fix by hand:")
        for key, count in leftovers.most_common(10):
            print(f"    {count:5d}  {key}")
    else:
        print("  no unresolved layer paths remain")

    print("\n  Remaining manual steps:")
    print(f"    1. packages/feature/{name}/Cargo.toml  (needs `[lints] workspace = true`)")
    print(f"    2. packages/feature/{name}/README.md   (section 3.3's six sections)")
    print(f"    3. src/lib.rs for the package, plus a mod.rs per slice")
    print(f"    4. each slice's cli publishes its Args type and run fn (section 4.2)")
    print(f"    5. root facade in src/{{domain,application/usecase,presentation}}, and the")
    print(f"       root Cargo.toml path dependency")
    print(f"    6. cargo clippy --fix -p paredit-feature-{name}   then  nix fmt")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("name", help="directory under packages/feature/")
    parser.add_argument("slices", nargs="+", help="slice names, as they were spelled in src/domain")
    args = parser.parse_args()
    print(f"packages/feature/{args.name}  ({len(args.slices)} slices)")
    return rewrite(args.name, args.slices)


if __name__ == "__main__":
    sys.exit(main())
