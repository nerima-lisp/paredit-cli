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
3. Draft the release notes. There is no `CHANGELOG.md` in this repository: the
   GitHub Release description is the only canonical history, and `release.yml`
   deliberately writes no body at all. Read `git log <previous-tag>..HEAD` and
   select entries by "does a user of `paredit` have to change their own code".
   Keep the text to hand — it is pasted in at publish time, in the section
   below.
4. Update the documentation for anything the release changes, including the
   `vX.Y.Z` in the install examples in `installation.md` and `releases.md`.
5. Commit as `chore(release): vX.Y.Z`.

`nix flake check` verifies steps 1, 2 and 4: `compatibility_contract` asserts
that `release.yml` still creates an empty draft rather than publishing a
machine-written body, and that this page still explains how to publish it.

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
2. Wait for `release.yml` to go green. It verifies the tag against
   `Cargo.toml`, runs `nix flake check` on the tagged tree, and creates the
   GitHub Release as an empty **draft** — it writes no body.
3. Paste in the notes drafted above and publish the draft:

   ```sh
   gh release edit vX.Y.Z --notes-file <file> --draft=false
   ```

   The draft is deliberate. A draft appears neither under "Latest release" nor
   in the default output of `gh release list`, so a release whose notes were
   forgotten never reaches downstream.
4. Confirm the GitHub Pages documentation build and that
   `nix run github:nerima-lisp/paredit-cli/vX.Y.Z -- --version` reports the
   released version.
5. If the release changes JSON output, command paths, flags, exit codes, the
   MSRV, or Nix interfaces, call out the migration in the release notes.

The release process does not replace the compatibility rules in the
[agent interface](agents.md) and the
[release and compatibility guide](releases.md).
