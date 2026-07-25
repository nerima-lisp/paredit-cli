# Releasing paredit-cli

Only a maintainer with the required registry and repository permissions should
perform a release. Run the steps from a clean checkout of the intended release
commit.

## Choose the version

`paredit-cli` follows [Semantic Versioning](https://semver.org/), and the
[release and compatibility guide](docs/src/releases.md) defines exactly which
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
  command path, a flag, an exit code, a documented JSON field, a `paredit_cli`
  crate-root export, or a Nix output.
- **Minor** — new commands, flags, fields, lint rules, dialects, or a raised
  MSRV.
- **Patch** — fixes and text-output changes only.

## Prepare the release commit

1. Set the new version in `Cargo.toml`.
2. Refresh `Cargo.lock` (`cargo update --workspace --offline`, or any build)
   so the recorded `paredit-cli` version matches.
3. Update the documentation for anything the release changes.
4. Commit as `release: vX.Y.Z`.

## Verify the release candidate

```sh
nix flake check
cargo +1.85 test --locked
cargo audit --deny warnings
cargo package --locked
cargo publish --dry-run --locked
```

Confirm that `Cargo.toml` contains the intended version, `Cargo.lock` matches,
`paredit inspect capabilities --output json` reports that version, the README
and the MkDocs site describe the released command surface, and the generated
package contains the public crate documents.

## Publish and announce

1. Publish the verified crate with `cargo publish --locked`.
2. Create the corresponding annotated Git tag and GitHub release from the
   verified commit.
3. Confirm the package page on crates.io, the library API on docs.rs, and the
   GitHub Pages documentation build.
4. If the release changes JSON output, command paths, flags, exit codes, the
   MSRV, or Nix interfaces, call out the migration in the release notes.

The release process does not replace the compatibility rules in the
[agent interface](docs/src/agents.md) and the
[release and compatibility guide](docs/src/releases.md).
