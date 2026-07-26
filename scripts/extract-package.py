#!/usr/bin/env python3
"""Extract a set of root-crate modules into a `packages/<kind>/<name>` member.

Implements the move half of the package-by-feature migration
(SPEC-package-by-feature.md sections 6 and 11.4), including the five procedural
gaps section 11.6 records from the Phase 1 pilot.

    scripts/extract-package.py core syntax sexpr dialect common_lisp
    scripts/extract-package.py --layer infrastructure core workspace workspace fs_identity

It deliberately stops before `git commit`. Section 13.1 requires the pure
`git mv` to be its own commit so rename detection cannot fail, and the caller
is expected to review the rewrites in between.

What it does NOT do, because each needs judgement:

  * write the README (section 3.3 requires six sections of prose that only
    someone who understands the boundary can write)
  * decide the facade's per-module visibility
  * resolve dependency cycles - run --check first, it refuses to extract a
    module set that is not closed
"""

from __future__ import annotations

import argparse
import collections
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
LAYERS = ("domain", "application", "infrastructure", "presentation")


def module_files(layer: str, module: str) -> list[pathlib.Path]:
    """Every file of `module`, whether it is `m.rs`, `m/mod.rs`, or both."""
    base = ROOT / "src" / layer
    found: list[pathlib.Path] = []
    if (base / f"{module}.rs").is_file():
        found.append(base / f"{module}.rs")
    if (base / module).is_dir():
        found.extend(sorted((base / module).rglob("*.rs")))
    return found


def code_of(line: str) -> str:
    """The line with any comment stripped.

    Doc links resolve differently from code: a `crate::domain::x` inside `///`
    only breaks rustdoc, while one in code breaks the build. They are reported
    separately so the caller can tell a real cycle from a stale link.
    """
    return line.split("//")[0]


def scan(layer: str, modules: list[str]) -> tuple[collections.Counter, collections.Counter]:
    """Outbound references from the module set, split into code and doc."""
    inside = set(modules)
    code_out: collections.Counter = collections.Counter()
    doc_out: collections.Counter = collections.Counter()
    for module in modules:
        for path in module_files(layer, module):
            for line in path.read_text().splitlines():
                head, _, tail = line.partition("//")
                for other_layer, target in re.findall(
                    r"crate::(domain|application|infrastructure|presentation)::([a-z_0-9]+)", head
                ):
                    if other_layer != layer or target not in inside:
                        code_out[f"{other_layer}::{target}"] += 1
                for other_layer, target in re.findall(
                    r"crate::(domain|application|infrastructure|presentation)::([a-z_0-9]+)", tail
                ):
                    if other_layer != layer or target not in inside:
                        doc_out[f"{other_layer}::{target}"] += 1
    return code_out, doc_out


def check(layer: str, modules: list[str], allow: set[str]) -> bool:
    missing = [m for m in modules if not module_files(layer, m)]
    if missing:
        print(f"  ERROR: no such module(s): {', '.join(missing)}")
        return False

    code_out, doc_out = scan(layer, modules)
    unresolved = {k: v for k, v in code_out.items() if k.split("::")[1] not in allow}

    total = sum(len(module_files(layer, m)) for m in modules)
    lines = sum(
        len(p.read_text().splitlines()) for m in modules for p in module_files(layer, m)
    )
    print(f"  {len(modules)} modules, {total} files, {lines} lines")

    if unresolved:
        print("  NOT CLOSED - these code references leave the package:")
        for key, count in sorted(unresolved.items(), key=lambda kv: -kv[1]):
            print(f"    {count:5d}  crate::{key}")
        print("  Resolve each one (move the type down, or the module up) before extracting.")
        return False

    print("  closed: no outbound code references beyond already-extracted packages")
    if doc_out:
        print("  doc-comment-only references (rewrite or demote to code spans):")
        for key, count in sorted(doc_out.items(), key=lambda kv: -kv[1])[:10]:
            print(f"    {count:5d}  crate::{key}")
    return True


def git(*args: str) -> None:
    subprocess.run(["git", *args], cwd=ROOT, check=True)


def extract(kind: str, name: str, layer: str, modules: list[str], crate: str) -> None:
    dest = ROOT / "packages" / kind / name / "src"
    dest.mkdir(parents=True, exist_ok=True)

    for module in modules:
        base = ROOT / "src" / layer
        if (base / f"{module}.rs").is_file():
            git("mv", str((base / f"{module}.rs").relative_to(ROOT)),
                str((dest / f"{module}.rs").relative_to(ROOT)))
        if (base / module).is_dir():
            git("mv", str((base / module).relative_to(ROOT)),
                str((dest / module).relative_to(ROOT)))
    print(f"  moved {len(modules)} modules into packages/{kind}/{name}/src")
    print("  COMMIT NOW, content-free, before running --rewrite (section 13.1)")


def rewrite(kind: str, name: str, layer: str, crate: str) -> None:
    src = ROOT / "packages" / kind / name / "src"
    paths = list(src.rglob("*.rs"))

    counts: collections.Counter = collections.Counter()
    for path in paths:
        text = original = path.read_text()

        # `crate::<layer>::x` is `crate::x` once the layer module is the crate root.
        counts["paths"] += text.count(f"crate::{layer}::")
        text = text.replace(f"crate::{layer}::", "crate::")

        # `pub(in crate::<layer>)` named a path that no longer exists; the items
        # are reached across a crate boundary now, so they have to be `pub`.
        counts["pub_in"] += len(re.findall(rf"pub\(in crate::{layer}\)", text))
        text = re.sub(rf"pub\(in crate::{layer}\)", "pub", text)

        # `pub(crate)` now means "inside this package", so every item the root
        # still uses reads as dead code. Widen, and control the real surface
        # through the facade's per-module visibility instead (section 11.6).
        counts["pub_crate"] += len(re.findall(r"\bpub\(crate\)", text))
        text = re.sub(r"\bpub\(crate\)", "pub", text)

        # A doctest can only import the crate it is compiled into. Left alone,
        # `compile_fail` doctests keep passing while asserting nothing.
        counts["doctests"] += text.count("paredit_cli::")
        text = text.replace("paredit_cli::", f"{crate}::")

        if text != original:
            path.write_text(text)

    print(f"  rewrote {counts['paths']} module paths, {counts['pub_in']} pub(in ...), "
          f"{counts['pub_crate']} pub(crate), {counts['doctests']} doctest imports")

    leftovers = collections.Counter()
    for path in paths:
        for line in path.read_text().splitlines():
            for target in re.findall(r"crate::(?:domain|application|infrastructure|presentation)::[a-z_0-9]+", line):
                leftovers[target] += 1
    if leftovers:
        print("  STILL UNRESOLVED - fix by hand before compiling:")
        for key, count in leftovers.most_common(10):
            print(f"    {count:5d}  {key}")

    print("\n  Remaining manual steps:")
    print(f"    1. packages/{kind}/{name}/Cargo.toml   (needs `[lints] workspace = true`)")
    print(f"    2. packages/{kind}/{name}/README.md    (section 3.3's six sections)")
    print(f"    3. packages/{kind}/{name}/src/lib.rs   (#![doc = include_str!(\"../README.md\")])")
    print(f"    4. src/{layer}/mod.rs facade, mirroring each module's ORIGINAL visibility")
    print(f"    5. root Cargo.toml: {crate.replace('_', '-')} = {{ path = \"packages/{kind}/{name}\" }}")
    print(f"    6. git add -N packages/{kind}/{name}   (Nix only sees tracked files)")
    print("    7. grep -rn '\"src/%s/' tests/   for contract fixtures reading moved paths" % layer)
    print("    8. cargo clippy --fix -p %s   then  nix fmt" % crate.replace("_", "-"))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("kind", choices=("core", "feature"))
    parser.add_argument("name", help="directory under packages/<kind>/")
    parser.add_argument("modules", nargs="+")
    parser.add_argument("--layer", default="domain", choices=LAYERS)
    parser.add_argument("--check", action="store_true",
                        help="only report whether the module set is dependency-closed")
    parser.add_argument("--rewrite", action="store_true",
                        help="rewrite paths and visibility after the move has been committed")
    parser.add_argument("--allow", default="",
                        help="comma-separated modules already extracted, so references to them are fine")
    args = parser.parse_args()

    crate = f"paredit_{args.kind}_{args.name.replace('-', '_')}"
    allow = set(args.modules) | {m for m in args.allow.split(",") if m}

    print(f"packages/{args.kind}/{args.name}  ({crate})")

    if args.rewrite:
        rewrite(args.kind, args.name, args.layer, crate)
        return 0

    if not check(args.layer, args.modules, allow):
        return 1
    if args.check:
        return 0

    extract(args.kind, args.name, args.layer, args.modules, crate)
    return 0


if __name__ == "__main__":
    sys.exit(main())
