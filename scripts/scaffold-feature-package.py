#!/usr/bin/env python3
"""Generate a feature package's manifest, lib.rs, and one mod.rs per slice.

Step 4 of the per-feature procedure in SPEC-package-by-feature.md section 6:

    scripts/scaffold-feature-package.py binding \\
        --description "Reshaping let, let*, flet and progn binding forms" \\
        introduce_let split_let merge_nested_let

Each slice's `mod.rs` is generated from the layers that actually exist on disk.
A slice is not required to span all three: `sort_definitions` and `split_file`
own no subcommand and so have no `cli` layer, and manufacturing an empty one to
make the shape uniform would be worse than the asymmetry.

`lib.rs` re-exports the two names the composition root needs from each slice
that has a `cli` layer - its `clap` argument type and the function that runs it
(section 4.2) - and nothing else.

The README is NOT generated. Section 3.3 asks for six sections of prose that
only someone who understands the boundary can write, and a generated one would
be exactly the box-ticking that section is trying to prevent. Remember to label
its fenced blocks `text` or `rust,ignore`: `include_str!` turns an unlabelled
fence into a compiled doctest.
"""

from __future__ import annotations

import argparse
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent

# Only what a feature can legitimately reach. A feature depending on another
# feature is possible (section 2.2 measured 89 such edges) but is a decision,
# so it is not defaulted here - add the path dependency by hand.
CORE_DEPS = {
    "syntax": 'paredit-core-syntax = { path = "../../core/syntax" }',
    "semantics": 'paredit-core-semantics = { path = "../../core/semantics" }',
    "edit": 'paredit-core-edit = { path = "../../core/edit" }',
    "workspace": 'paredit-core-workspace = { path = "../../core/workspace" }',
    "lint-engine": 'paredit-core-lint-engine = { path = "../../core/lint-engine" }',
    "cli": 'paredit-core-cli = { path = "../../core/cli" }',
}


def layers_of(src: pathlib.Path, slice_: str) -> list[str]:
    return [layer for layer in ("cli", "domain", "usecase")
            if (src / slice_ / f"{layer}.rs").is_file() or (src / slice_ / layer).is_dir()]


def published(src: pathlib.Path, slice_: str) -> list[tuple[str, str]]:
    """The (Args type, run fn) pairs a slice's cli actually publishes.

    Not derivable from the slice name: `convert_sequential_binding` publishes
    `convert_do_star_to_do` and `convert_prog_star_to_prog`, and a slice may
    own more than one subcommand.
    """
    parts = [src / slice_ / "cli.rs", src / slice_ / "cli"]
    text = ""
    for part in parts:
        if part.is_file():
            text += part.read_text()
        elif part.is_dir():
            for f in part.rglob("*.rs"):
                text += f.read_text()
    args_types = set(re.findall(r"pub struct ([A-Za-z0-9_]+Args)\b", text))
    pairs = []
    for fn, arg in re.findall(r"pub fn ([a-z0-9_]+)\s*\(\s*[a-z0-9_]+\s*:\s*([A-Za-z0-9_]+Args)\b", text):
        if arg in args_types:
            pairs.append((arg, fn))
    pairs = sorted(set(pairs))

    # When `cli` is a directory, each pair sits in a submodule, so the slice's
    # cli/mod.rs must hoist them to `<slice>::cli::<name>` - the path lib.rs
    # and the composition root use.
    cli_dir = src / slice_ / "cli"
    if cli_dir.is_dir() and pairs:
        by_module: dict[str, list[str]] = {}
        for f in sorted(cli_dir.glob("*.rs")):
            if f.name == "mod.rs":
                continue
            body = f.read_text()
            for arg, fn in pairs:
                if re.search(rf"pub struct {arg}\b", body) or \
                   re.search(rf"pub fn {fn}\s*\(", body):
                    for name in (arg, fn):
                        if (re.search(rf"pub struct {name}\b", body)
                                or re.search(rf"pub fn {name}\s*\(", body)):
                            by_module.setdefault(f.stem, []).append(name)
        mod_rs = cli_dir / "mod.rs"
        base = mod_rs.read_text() if mod_rs.is_file() else ""
        hoist = "".join(
            f"pub use {module}::{{{', '.join(sorted(set(names)))}}};\n"
            for module, names in sorted(by_module.items())
        )
        if hoist and "// Hoisted for the composition root" not in base:
            mod_rs.write_text(
                base.rstrip("\n")
                + "\n\n// Hoisted for the composition root (section 4.2): the argument type and\n"
                  "// run function of each subcommand this slice owns.\n"
                + hoist
            )
    return pairs


def scaffold(name: str, slices: list[str], description: str, deps: list[str]) -> int:
    pkg = ROOT / "packages" / "feature" / name
    src = pkg / "src"
    if not src.is_dir():
        print(f"  ERROR: {src} does not exist - run the move first")
        return 1

    with_cli = []
    for slice_ in slices:
        layers = layers_of(src, slice_)
        if not layers:
            print(f"  ERROR: {slice_} has no layers on disk")
            return 1
        if "cli" in layers:
            with_cli.append(slice_)
        note = ("" if "cli" in layers else
                "//!\n//! No `cli` layer: this slice owns no subcommand and is driven by\n"
                "//! another command's workflow.\n")
        (src / slice_ / "mod.rs").write_text(
            f"//! One slice, one directory; the layers are names, not directories.\n{note}\n"
            + "".join(f"pub mod {layer};\n" for layer in layers)
        )
        print(f"  {slice_:34s} {'+'.join(layers)}")

    lib = '#![doc = include_str!("../README.md")]\n\n'
    lib += "".join(f"pub mod {s};\n" for s in sorted(slices))
    if with_cli:
        lib += ("\n// The contract with the composition root (section 4.2): each slice that\n"
                "// owns a subcommand publishes its `clap` argument type and the function\n"
                "// that runs it. command.rs and dispatch.rs need these two names and no more.\n")
        for slice_ in sorted(with_cli):
            for arg, fn in published(src, slice_):
                lib += f"pub use {slice_}::cli::{{{arg}, {fn}}};\n"
    (src / "lib.rs").write_text(lib)

    dep_lines = "\n".join(CORE_DEPS[d] for d in deps if d in CORE_DEPS)
    (pkg / "Cargo.toml").write_text(f'''[package]
name = "paredit-feature-{name}"
description = "{description}"
readme = "README.md"
publish = false
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
repository.workspace = true

[dependencies]
{dep_lines}
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
    print(f"\n  {len(slices)} slices, {len(with_cli)} with a subcommand")
    print(f"  WRITE packages/feature/{name}/README.md by hand (section 3.3's six sections),")
    print("  labelling fenced blocks `text` or `rust,ignore`.")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("name")
    parser.add_argument("slices", nargs="+")
    parser.add_argument("--description", required=True)
    parser.add_argument("--deps", default="syntax,semantics,edit,cli",
                        help="comma-separated core packages (default: %(default)s)")
    args = parser.parse_args()
    print(f"packages/feature/{args.name}")
    return scaffold(args.name, args.slices, args.description, args.deps.split(","))


if __name__ == "__main__":
    sys.exit(main())
