# Releasing paredit-cli

Only a maintainer with the required registry and repository permissions should
perform a release. Run the steps from a clean checkout of the intended release
commit.

## Choose the version

`paredit-cli` follows [Semantic Versioning](https://semver.org/), and the
[release and compatibility guide](releases.md) defines exactly which
surfaces are covered. Before picking a number, diff the command catalog of the
release candidate against the previous tag:

```sh
cargo run -q -- inspect capabilities --output json > /tmp/next.json
git worktree add /tmp/prev "$(git describe --tags --abbrev=0)"
cargo run -q --manifest-path /tmp/prev/Cargo.toml -- \
  inspect capabilities --output json > /tmp/prev.json
diff -u /tmp/prev.json /tmp/next.json
```

- **Major** — a stable surface was removed, renamed, or changed meaning: a
  command path, a flag, an exit code, a documented JSON field, or a Nix output.
- **Minor** — new commands, flags, fields, lint rules, dialects, or a raised
  MSRV.
- **Patch** — fixes and text-output changes only.

The Rust library API is deliberately absent from that list. It is not a stable
surface — see [what `1.x` guarantees](releases.md#what-1x-guarantees) — because
the crate is `publish = false` and the CLI is the supported interface. A change
confined to the library, however sweeping, does not on its own force a major
release.

## Prepare the release commit

1. Set the new version in `Cargo.toml`.
2. Refresh `Cargo.lock` (`cargo update --workspace --offline`, or any build)
   so the recorded `paredit-cli` version matches.
3. Add a `## [X.Y.Z] - YYYY-MM-DD` section to `CHANGELOG.md`. This is not
   optional: the "Extract release notes" step in `release.yml` builds the
   GitHub release body by running an `awk` script over this file, and it fails
   the workflow if the section for the pushed tag is missing or empty. The
   heading must match `## [X.Y.Z] - YYYY-MM-DD` exactly — the script matches it
   literally, so a malformed heading yields an empty body rather than an error.
4. Update the documentation for anything the release changes, including the
   `vX.Y.Z` in the install examples in `installation.md` and `releases.md`.
5. Commit as `chore(release): vX.Y.Z`.

`nix flake check` verifies steps 1–4: `compatibility_contract` runs
`release.yml`'s real `awk` script against the real `CHANGELOG.md`, so a missing
section fails the gate rather than the tag push.

## Verify the release candidate

```sh
nix flake check
cargo +1.85 test --locked
cargo audit --deny warnings
nix build .# && ./result/bin/paredit --version
```

Confirm that `Cargo.toml` contains the intended version, `Cargo.lock` matches,
`paredit inspect capabilities --output json` reports that version, and the
README and the MkDocs site describe the released command surface.

## Publish and announce

`paredit-cli` is distributed as a Git tag consumed by Nix and
`cargo install --git`; it is not published to a package registry, so the tag
*is* the release artifact.

1. Create the annotated Git tag on the verified commit and push the branch and
   the tag: `git push origin main && git push origin vX.Y.Z`.
2. Publish the GitHub release from that tag with the notes below.
3. Confirm the GitHub Pages documentation build and that
   `nix run github:nerima-lisp/paredit-cli/vX.Y.Z -- --version` reports the
   released version.
4. If the release changes JSON output, command paths, flags, exit codes, the
   MSRV, or Nix interfaces, call out the migration in the release notes.

The release process does not replace the compatibility rules in the
[agent interface](agents.md) and the
[release and compatibility guide](releases.md).
