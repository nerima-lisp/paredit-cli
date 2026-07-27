#!/usr/bin/env python3
"""Move a themed group of lint rules into `packages/feature/lint-<theme>`.

Phase 5 of SPEC-package-by-feature.md. A lint rule is four files, not one:

    src/domain/lint/rules/<rule>.rs        the rule adapter: META, RULE, heads
    src/domain/<rule>_report.rs            the detection logic
    src/application/usecase/<rule>_report.rs
    src/presentation/cli/<rule>_report/    the inspect subcommand

so each rule becomes one slice directory holding all four:

    <rule>/{rule.rs, domain.rs, usecase.rs, cli/}

`REGISTRY` stays in the root and reaches each rule's `META` and `RULE` across
the crate boundary. Section 4.2 requires that: a registry naming every rule,
in a crate every rule depends on, is a cycle. `RULE_COUNT`'s const assertion is
what detects a rule accidentally left behind.

    scripts/move-lint-package.py --check string-char char_case_fold format_newline
    scripts/move-lint-package.py string-char char_case_fold format_newline
"""

from __future__ import annotations

import argparse
import collections
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent

# A handful of rules name their report module in the singular, so the
# report is not simply `<rule>_report`.
REPORT_ALIAS = {'duplicate_cond_tests': 'duplicate_cond_test', 'identical_if_branches': 'identical_if_branch', 'duplicate_boolean_operands': 'duplicate_boolean_operand', 'duplicate_case_keys': 'duplicate_case_key', 'duplicate_let_bindings': 'duplicate_let_binding', 'duplicate_parameters': 'duplicate_parameter', 'duplicate_setf_places': 'duplicate_setf_place'}

# rule name -> (source path, destination name inside the slice)
def parts_for(rule: str) -> list[tuple[pathlib.Path, str]]:
    report = f"{REPORT_ALIAS.get(rule, rule)}_report"
    candidates = [
        (ROOT / "src/domain/lint/rules" / f"{rule}.rs", "rule.rs"),
        (ROOT / "src/domain" / f"{report}.rs", "domain.rs"),
        (ROOT / "src/domain" / report, "domain"),
        (ROOT / "src/application/usecase" / f"{report}.rs", "usecase.rs"),
        (ROOT / "src/application/usecase" / report, "usecase"),
        (ROOT / "src/presentation/cli" / f"{report}.rs", "cli.rs"),
        (ROOT / "src/presentation/cli" / report, "cli"),
    ]
    return [(src, dst) for src, dst in candidates if src.exists()]


def rs_files(path: pathlib.Path) -> list[pathlib.Path]:
    return sorted(path.rglob("*.rs")) if path.is_dir() else [path]


def check(rules: list[str], extracted: set[str]) -> bool:
    files: list[pathlib.Path] = []
    missing = []
    for rule in rules:
        got = parts_for(rule)
        if not got:
            missing.append(rule)
            continue
        for src, _ in got:
            files += rs_files(src)
    if missing:
        print(f"  ERROR: no files for {', '.join(missing)}")
        return False

    own = set(rules) | {f"{REPORT_ALIAS.get(r, r)}_report" for r in rules}
    outbound: collections.Counter = collections.Counter()
    for path in files:
        for line in path.read_text().splitlines():
            code = line.split("//")[0]
            for layer, target in re.findall(
                r"crate::(domain|application|infrastructure|presentation)::([a-z_0-9]+)", code
            ):
                if target in own or target in extracted:
                    continue
                outbound[f"{layer}::{target}"] += 1

    lines = sum(len(p.read_text().splitlines()) for p in files)
    print(f"  {len(rules)} rules, {len(files)} files, {lines} lines")
    if outbound:
        print("  NOT CLOSED:")
        for key, count in sorted(outbound.items(), key=lambda kv: -kv[1])[:12]:
            print(f"    {count:5d}  crate::{key}")
        return False
    print("  closed")
    return True


def git(*args: str) -> None:
    subprocess.run(["git", *args], cwd=ROOT, check=True)


def move(theme: str, rules: list[str]) -> None:
    dest_root = ROOT / "packages/feature" / f"lint-{theme}" / "src"
    for rule in rules:
        (dest_root / rule).mkdir(parents=True, exist_ok=True)
        for src, dst in parts_for(rule):
            git("mv", str(src.relative_to(ROOT)),
                str((dest_root / rule / dst).relative_to(ROOT)))
    print(f"  moved {len(rules)} rules into packages/feature/lint-{theme}/src")
    print("  COMMIT NOW, content-free (section 13.1)")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("theme")
    parser.add_argument("rules", nargs="+")
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--extracted", default="",
                        help="comma-separated modules already in packages")
    args = parser.parse_args()

    extracted = {m for m in args.extracted.split(",") if m}
    print(f"packages/feature/lint-{args.theme}")
    if not check(args.rules, extracted):
        return 1
    if args.check:
        return 0
    move(args.theme, args.rules)
    return 0


if __name__ == "__main__":
    sys.exit(main())
