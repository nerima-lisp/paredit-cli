#!/usr/bin/env python3
"""Check a feature's dependency closure, then `git mv` it slice-first.

Steps 1-3 of the per-feature procedure in SPEC-package-by-feature.md section 6:

    scripts/move-feature-package.py --check inline inline_function inline_let
    scripts/move-feature-package.py inline inline_function inline_let

`--check` reports whether the slices depend only on packages already extracted.
Without it, the slices move to `packages/feature/<name>/src/<slice>/{domain,
usecase,cli}` and nothing else happens - commit that move on its own, with no
content change, so rename detection cannot fail (section 13.1).

Two things this handles that hand-rolling gets wrong:

  * A module can be BOTH `x.rs` and `x/` - Rust's 2018 style, where the file is
    the module root and the directory holds its children. 18 modules in this
    tree have that shape. Moving only the directory strands the root and the
    package will not resolve.
  * The closure check must count both halves too, or it reports a slice as
    closed while ignoring most of its code.
"""

from __future__ import annotations

import argparse
import collections
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent

LAYERS = [("domain", "src/domain"),
          ("usecase", "src/application/usecase"),
          ("cli", "src/presentation/cli")]

# Everything already extracted, by module name. A reference to one of these is
# fine; a reference to anything else means the slice set is not closed.
EXTRACTED = set("""
sexpr dialect common_lisp definition leading_trivia expression_equality form_shape graph
view_query semantics lexical_scope binding_index callable_scope definition_reference
workspace fs_identity lint mutation_safety refactor_plan refactor_preview refactor_execute
extract_shared let_binding progn local_function_binding let_composition let_star_composition
flet_composition convert_control args io diff shared gate macos_acl usecase cli
similarity_report duplicate_report form_similarity
extract_function extract_local_function extract_constant
inline_function inline_let inline_lambda inline_local_function inline_symbol_macro
""".split())


def parts_of(base: str, slice_: str) -> list[pathlib.Path]:
    """The module root file and/or its directory, whichever exist."""
    found = []
    file_ = ROOT / base / f"{slice_}.rs"
    dir_ = ROOT / base / slice_
    if file_.is_file():
        found.append(file_)
    if dir_.is_dir():
        found.append(dir_)
    return found


def rs_files(paths: list[pathlib.Path]) -> list[pathlib.Path]:
    out = []
    for p in paths:
        out.extend(sorted(p.rglob("*.rs")) if p.is_dir() else [p])
    return out


def check(slices: list[str]) -> bool:
    files: list[pathlib.Path] = []
    for layer, base in LAYERS:
        for slice_ in slices:
            got = rs_files(parts_of(base, slice_))
            files += got
            if got:
                lines = sum(len(p.read_text().splitlines()) for p in got)
                shape = "+".join(p.name if p.is_file() else f"{p.name}/"
                                 for p in parts_of(base, slice_))
                print(f"  {layer:8s} {slice_:28s} {len(got):3d} files {lines:6d} lines  [{shape}]")
    if not files:
        print("  ERROR: no files found for those slices")
        return False

    outbound: collections.Counter = collections.Counter()
    known = EXTRACTED | set(slices)
    for path in files:
        for line in path.read_text().splitlines():
            code = line.split("//")[0]
            for layer, target in re.findall(
                r"crate::(domain|application|infrastructure|presentation)::([a-z_0-9]+)", code
            ):
                if target not in known:
                    outbound[f"{layer}::{target}"] += 1
            # A file directly under a layer reaches its siblings as `super::x`,
            # which the `crate::` pattern above never sees. F7 depended on
            # `rename` through exactly this and was reported closed.
            # Only a file sitting DIRECTLY under the layer reaches a layer
            # sibling through `super::`. Deeper files reach their own module's
            # children that way, which is internal and must not be reported.
            if path.parent == (ROOT / base):
                for target in re.findall(r"\buse super::([a-z_0-9]+)(?:::|;)", code):
                    if target not in known and target != "super":
                        outbound[f"super::{target}"] += 1

    total = sum(len(p.read_text().splitlines()) for p in files)
    print(f"\n  {len(files)} files, {total} lines")
    if outbound:
        print("  NOT CLOSED - these references leave the feature:")
        for key, count in sorted(outbound.items(), key=lambda kv: -kv[1]):
            print(f"    {count:5d}  crate::{key}")
        print("  Either add the missing slice, or move the shared type down into core.")
        return False
    print("  closed: depends only on already-extracted packages")
    return True


def git(*args: str) -> None:
    subprocess.run(["git", *args], cwd=ROOT, check=True)


def move(name: str, slices: list[str]) -> None:
    dest_root = ROOT / "packages" / "feature" / name / "src"
    for slice_ in slices:
        (dest_root / slice_).mkdir(parents=True, exist_ok=True)
        for layer, base in LAYERS:
            file_ = ROOT / base / f"{slice_}.rs"
            dir_ = ROOT / base / slice_
            # Both halves move, and they keep their relationship: the root
            # becomes `<layer>.rs` beside the directory `<layer>/`.
            if file_.is_file():
                git("mv", str(file_.relative_to(ROOT)),
                    str((dest_root / slice_ / f"{layer}.rs").relative_to(ROOT)))
            if dir_.is_dir():
                git("mv", str(dir_.relative_to(ROOT)),
                    str((dest_root / slice_ / layer).relative_to(ROOT)))
    print(f"  moved {len(slices)} slices into packages/feature/{name}/src")
    print("  COMMIT NOW, content-free, before rewriting (section 13.1)")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("name")
    parser.add_argument("slices", nargs="+")
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()

    print(f"packages/feature/{args.name}")
    if not check(args.slices):
        return 1
    if args.check:
        return 0
    move(args.name, args.slices)
    return 0


if __name__ == "__main__":
    sys.exit(main())
