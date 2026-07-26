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


def rewrite(name: str, slices: list[str]) -> int:
    src = ROOT / "packages" / "feature" / name / "src"
    if not src.is_dir():
        print(f"  ERROR: {src} does not exist - run the git mv first")
        return 1
    crate = f"paredit_feature_{name.replace('-', '_')}"
    counts: collections.Counter = collections.Counter()

    for path in src.rglob("*.rs"):
        text = original = path.read_text()

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

        # 3. Anything still addressing the old cli root is a shared helper.
        text, n = re.subn(r"\bcrate::presentation::cli\b", "paredit_core_cli::shared", text)
        counts["cli_root"] += n
        # ...but a visibility cannot name another crate, so undo it there.
        text, n = re.subn(r"pub\(in paredit_core_cli::shared\)", "pub", text)
        counts["visibility_fix"] += n

        # 4. Visibility that cannot cross a crate boundary.
        text, n = re.subn(r"\bpub\(crate\)", "pub", text)
        counts["pub_crate"] += n
        text, n = re.subn(r"\bpub\(super\)", "pub", text)
        counts["pub_super"] += n

        # 5. Doctests can only import the crate they compile into.
        text, n = re.subn(r"\bparedit_cli::", f"{crate}::", text)
        counts["doctests"] += n

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
            at = next((i for i, l in enumerate(lines) if l.startswith("use ")), 0)
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
