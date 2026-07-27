#!/usr/bin/env python3
"""Wire a feature package into the root crate: path dependency plus facades.

Step 5 of the per-feature procedure in SPEC-package-by-feature.md section 6:

    scripts/wire-feature-facade.py binding introduce_let split_let flatten_progn

Removes each slice's old `mod` declaration from the three layer roots and adds
a re-export in its place, so every existing `crate::domain::<slice>` path and
the public `paredit_cli::domain::…` API keep resolving (section 4.1). Benches
and examples depend on that and must never need editing.

Re-exports are emitted only for layers that exist on disk. Not every slice
spans all three - some own no subcommand, some no domain - and emitting a
re-export for a layer that is not there fails with an unresolved import
listing every slice at once, which is a slow way to learn it.
"""

from __future__ import annotations

import argparse
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent

LAYER_ROOTS = {
    "domain": ROOT / "src/domain/mod.rs",
    "usecase": ROOT / "src/application/usecase/mod.rs",
    "cli": ROOT / "src/presentation/cli.rs",
}
MARKER = "// Facade re-exports for extracted feature packages (section 4.1)."


def layers_of(src: pathlib.Path, slice_: str) -> list[str]:
    return [layer for layer in ("domain", "usecase", "cli")
            if (src / slice_ / f"{layer}.rs").is_file() or (src / slice_ / layer).is_dir()]


def wire(name: str, slices: list[str]) -> int:
    crate = f"paredit_feature_{name.replace('-', '_')}"
    src = ROOT / "packages" / "feature" / name / "src"
    if not src.is_dir():
        print(f"  ERROR: {src} does not exist")
        return 1

    # root manifest
    manifest = ROOT / "Cargo.toml"
    text = manifest.read_text()
    dep = f'paredit-feature-{name} = {{ path = "packages/feature/{name}" }}'
    if dep not in text:
        anchor = "\nanyhow = \"1.0\""
        text = text.replace(anchor, f"\n{dep}{anchor}", 1)
        manifest.write_text(text)
        print(f"  added path dependency for paredit-feature-{name}")

    per_layer: dict[str, list[str]] = {layer: [] for layer in LAYER_ROOTS}
    for slice_ in slices:
        found = layers_of(src, slice_)
        if not found:
            print(f"  ERROR: {slice_} has no layers on disk")
            return 1
        for layer in found:
            per_layer[layer].append(slice_)
        missing = [l for l in ("domain", "usecase", "cli") if l not in found]
        note = f"   (no {', '.join(missing)})" if missing else ""
        print(f"  {slice_:34s} {'+'.join(found)}{note}")

    for layer, path in LAYER_ROOTS.items():
        text = path.read_text()
        for slice_ in slices:
            for vis in ("pub", "pub(crate)", ""):
                decl = f"{vis} mod {slice_};\n".lstrip()
                text = text.replace(decl, "")
        # `cli.rs` declared these as private `mod x;`, so a `pub use` here would
        # widen the crate's public API for no reason. domain and usecase were
        # `pub mod`, and benches/examples reach them through those paths.
        vis = "use" if layer == "cli" else "pub use"
        lines = [f"{vis} {crate}::{s}::{layer} as {s};" for s in per_layer[layer]]
        if not lines:
            continue
        block = "\n".join(lines)
        if MARKER in text:
            text = text.replace(MARKER, f"{MARKER}\n{block}", 1)
        else:
            # Before any `#[cfg(test)] mod tests`, not at end of file: clippy's
            # items_after_test_module rejects items following a test module.
            at = text.find("#[cfg(test)]")
            new = f"{MARKER}\n{block}\n"
            if at == -1:
                text = text.rstrip("\n") + f"\n\n{new}"
            else:
                text = text[:at] + new + "\n" + text[at:]
        path.write_text(text)
        print(f"  {path.relative_to(ROOT)}: +{len(lines)} re-exports")

    print("\n  Next: cargo check --all-targets, then clippy --fix -p, then nix fmt.")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("name")
    parser.add_argument("slices", nargs="+")
    args = parser.parse_args()
    print(f"packages/feature/{args.name}")
    return wire(args.name, args.slices)


if __name__ == "__main__":
    sys.exit(main())
