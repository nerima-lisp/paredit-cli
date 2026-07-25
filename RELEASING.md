# Releasing paredit-cli

Only a maintainer with the required registry and repository permissions should
perform a release. Run the steps from a clean checkout of the intended release
commit.

## Verify the release candidate

```sh
nix flake check
cargo +1.85 test --locked
cargo package --locked
cargo publish --dry-run --locked
```

Confirm that `Cargo.toml` contains the intended version, `Cargo.lock` matches,
the README and the MkDocs site describe the released command surface, and the
generated package contains the public crate documents.

## Publish and announce

1. Publish the verified crate with `cargo publish --locked`.
2. Create the corresponding annotated Git tag and GitHub release from the
   verified commit.
3. Confirm the package page on crates.io, the library API on docs.rs, and the
   GitHub Pages documentation build.
4. If the release changes JSON output, command paths, flags, or Nix interfaces,
   call out the migration in the release notes.

The release process does not replace the compatibility rules in the
[agent interface](docs/src/agents.md).
