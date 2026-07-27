#!/usr/bin/env python3
"""Rewrite, scaffold and wire a themed lint package after its `git mv`.

Steps 3-5 of Phase 5, established by lint-string-char:

    scripts/wire-lint-package.py string-char --description "..." char_case_fold format_newline

Rewrites paths, generates the manifest / lib.rs / per-rule mod.rs, repoints the
root's REGISTRY at the package, drops the moved `mod` lines from
domain/lint/rules/mod.rs, and adds the three layer facades.

REGISTRY itself never moves. It stays in the root naming each rule's `META` and
`RULE` across the crate boundary, which is what section 4.2 requires: a
registry lives in neither the engine nor the rules, or it forms a cycle with
one of them. `RULE_COUNT`'s const assertion turns "a rule was left behind" into
a compile error.

The README is not generated - section 3.3 wants the rule list written out,
because for a themed package that list IS the explanation of the boundary.
"""

from __future__ import annotations

import argparse
import collections
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent

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

ARGS_ITEMS = ["DialectArg", "OutputFormat", "MoveInsert", "ParameterInsert", "ThreadStyleArg",
              "SourceInput", "EditTargetArgs", "TargetArgs", "WrapDelimiter"]


def rewrite(theme: str, rules: list[str]) -> collections.Counter:
    src = ROOT / "packages/feature" / f"lint-{theme}" / "src"
    counts: collections.Counter = collections.Counter()
    for path in src.rglob("*.rs"):
        text = original = path.read_text()

        for module, owner in OWNER.items():
            for layer in ("domain", "infrastructure", "presentation::cli", "application::usecase"):
                text, n = re.subn(rf"\bcrate::{layer}::{module}\b", f"{owner}::{module}", text)
                counts["core"] += n

        text, n = re.subn(r"\bcrate::domain::lint::(engine|model|policy|rule|registry)\b",
                          r"paredit_core_lint_engine::\1", text)
        counts["engine"] += n
        text, n = re.subn(r"\bcrate::domain::lint\b", "paredit_core_lint_engine", text)
        counts["engine"] += n

        # A rule's four files collapse into one slice directory.
        for rule in rules:
            for layer, dest in (("domain", "domain"),
                                ("application::usecase", "usecase"),
                                ("presentation::cli", "cli")):
                text, n = re.subn(rf"\bcrate::{layer}::{rule}_report\b",
                                  f"crate::{rule}::{dest}", text)
                counts["own"] += n
            text, n = re.subn(rf"\bcrate::domain::lint::rules::{rule}\b",
                              f"crate::{rule}::rule", text)
            counts["own"] += n
            text, n = re.subn(rf"\bsuper::super::{rule}\b", f"crate::{rule}::rule", text)
            counts["own"] += n

        text, n = re.subn(r"\bcrate::presentation::cli\b", "paredit_core_cli::shared", text)
        counts["cli"] += n
        text, n = re.subn(r"pub\(in paredit_core_cli::shared\)", "pub", text)
        counts["cli"] += n

        # cli.rs re-exported from both args and shared; the value enums are args'.
        def split_shared(match: re.Match[str]) -> str:
            names = [x.strip() for x in match.group(1).split(",") if x.strip()]
            args = [x for x in names if x.split(" as ")[0].strip() in ARGS_ITEMS]
            rest = [x for x in names if x not in args]
            out = []
            if rest:
                out.append(f"use paredit_core_cli::shared::{{{', '.join(rest)}}};")
            if args:
                out.append(f"use paredit_core_cli::args::{{{', '.join(args)}}};")
            return "\n".join(out)

        text, n = re.subn(r"use paredit_core_cli::shared::\{([^}]*)\};", split_shared, text)
        counts["cli"] += n
        for name in ARGS_ITEMS:
            text, n = re.subn(rf"\bparedit_core_cli::shared::{name}\b",
                              f"paredit_core_cli::args::{name}", text)
            counts["cli"] += n

        for pattern in (r"\bpub\(crate\)", r"\bpub\(super\)", r"\bpub\(in crate::[a-z_0-9:]+\)"):
            text, n = re.subn(pattern, "pub", text)
            counts["vis"] += n
        # ...but a `pub` glob re-exports nothing and clippy rejects it.
        text, n = re.subn(r"^(\s*)pub use super::\*;$", r"\1pub(super) use super::*;",
                          text, flags=re.M)
        counts["vis"] += n

        if "safe_text!" in text and "use paredit_core_cli::safe_text;" not in text:
            lines = text.splitlines(keepends=True)
            at = next((i for i, l in enumerate(lines) if l.startswith("use ")), 0)
            lines.insert(at, "use paredit_core_cli::safe_text;\n")
            text = "".join(lines)
            counts["safe_text"] += 1

        if text != original:
            path.write_text(text)

    leftovers = collections.Counter()
    for path in src.rglob("*.rs"):
        for hit in re.findall(
            r"crate::(?:domain|application|infrastructure|presentation)::[a-z_0-9]+",
            path.read_text(),
        ):
            leftovers[hit] += 1
    if leftovers:
        print("  STILL UNRESOLVED:", dict(leftovers))
    return counts


def scaffold(theme: str, rules: list[str], description: str) -> None:
    pkg = ROOT / "packages/feature" / f"lint-{theme}"
    for rule in rules:
        layers = [l for l in ("rule", "domain", "usecase", "cli")
                  if (pkg / "src" / rule / f"{l}.rs").is_file() or (pkg / "src" / rule / l).is_dir()]
        (pkg / "src" / rule / "mod.rs").write_text(
            f"//! The `{rule.replace('_', '-')}` lint rule: its adapter, detection, "
            "use case and command.\n//!\n"
            "//! One rule, one directory. `rule` is what the registry registers; the\n"
            "//! rest is the report it drives.\n\n"
            + "".join(f"pub mod {l};\n" for l in layers)
        )
    lib = '#![doc = include_str!("../README.md")]\n\n'
    lib += "".join(f"pub mod {r};\n" for r in sorted(rules))
    lib += ("\n// The root's REGISTRY names each rule's META and RULE across this crate\n"
            "// boundary (section 4.2), and each slice's cli owns its own subcommand.\n")
    (pkg / "src" / "lib.rs").write_text(lib)
    (pkg / "Cargo.toml").write_text(f'''[package]
name = "paredit-feature-lint-{theme}"
description = "{description}"
readme = "README.md"
publish = false
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
repository.workspace = true

[dependencies]
paredit-core-syntax = {{ path = "../../core/syntax" }}
paredit-core-semantics = {{ path = "../../core/semantics" }}
paredit-core-lint-engine = {{ path = "../../core/lint-engine" }}
paredit-core-workspace = {{ path = "../../core/workspace" }}
paredit-core-cli = {{ path = "../../core/cli" }}
anyhow.workspace = true
clap.workspace = true
serde_json.workspace = true
thiserror.workspace = true

[dev-dependencies]
proptest.workspace = true

# Mandatory: without it this package silently opts out of the workspace lint
# table, including `unsafe_code = "deny"`, with no error at all.
[lints]
workspace = true
''')


def wire(theme: str, rules: list[str]) -> None:
    crate = f"paredit_feature_lint_{theme.replace('-', '_')}"

    manifest = ROOT / "Cargo.toml"
    text = manifest.read_text()
    dep = f'paredit-feature-lint-{theme} = {{ path = "packages/feature/lint-{theme}" }}'
    if dep not in text:
        manifest.write_text(text.replace('\nanyhow = "1.0"', f"\n{dep}\nanyhow = \"1.0\"", 1))

    # REGISTRY keeps naming every rule; only the path changes.
    registry = ROOT / "src/domain/lint/registry/mod.rs"
    text = registry.read_text()
    for rule in rules:
        text = text.replace(f"rules::{rule}::", f"{crate}::{rule}::rule::")
    registry.write_text(text)

    rules_mod = ROOT / "src/domain/lint/rules/mod.rs"
    text = rules_mod.read_text()
    for rule in rules:
        text = re.sub(rf"^(pub )?mod {rule};\n", "", text, flags=re.M)
    rules_mod.write_text(text)

    marker = "// Facade re-exports for extracted feature packages (section 4.1)."
    for rel, layer in (("src/domain/mod.rs", "domain"),
                       ("src/application/usecase/mod.rs", "usecase"),
                       ("src/presentation/cli.rs", "cli")):
        path = ROOT / rel
        text = path.read_text()
        for rule in rules:
            for vis in ("pub ", "pub(crate) ", ""):
                text = text.replace(f"{vis}mod {rule}_report;\n", "")
        vis = "use" if layer == "cli" else "pub use"
        block = "\n".join(f"{vis} {crate}::{r}::{layer} as {r}_report;" for r in rules)
        if marker in text:
            text = text.replace(marker, marker + "\n" + block, 1)
        else:
            at = text.find("#[cfg(test)]")
            new = marker + "\n" + block + "\n"
            text = (text[:at] + new + "\n" + text[at:]) if at != -1 else text.rstrip("\n") + "\n\n" + new
        path.write_text(text)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("theme")
    parser.add_argument("rules", nargs="+")
    parser.add_argument("--description", required=True)
    args = parser.parse_args()

    print(f"packages/feature/lint-{args.theme}  ({len(args.rules)} rules)")
    print(f"  {dict(rewrite(args.theme, args.rules))}")
    scaffold(args.theme, args.rules, args.description)
    wire(args.theme, args.rules)
    print("  scaffolded, REGISTRY repointed, facades written")
    print(f"  WRITE packages/feature/lint-{args.theme}/README.md listing every rule (section 3.3)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
