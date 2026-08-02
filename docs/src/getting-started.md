# Getting Started

`paredit` ships as a single binary. Nix is the primary distribution channel;
Cargo works anywhere a Rust toolchain is available.

## Run without installing (Nix)

```sh
nix run github:nerima-lisp/paredit-cli -- inspect check --file source.lisp
```

The companion lint and format gates are exposed as flake apps:

```sh
nix run github:nerima-lisp/paredit-cli#lint -- .
nix run github:nerima-lisp/paredit-cli#format -- --check .
```

## Install into a Nix profile

```sh
nix profile install github:nerima-lisp/paredit-cli
```

Prebuilt binaries are published to the public
[`takeokunn-paredit-cli`](https://takeokunn-paredit-cli.cachix.org) Cachix
cache, so neither command has to compile the crate from source:

```sh
cachix use takeokunn-paredit-cli
```

## Use as a flake input

Add the flake and pick the packages or the overlay:

```nix
{
  inputs.paredit-cli.url = "github:nerima-lisp/paredit-cli";

  outputs = { nixpkgs, paredit-cli, ... }: {
    # Directly as a package:
    #   paredit-cli.packages.${system}.default
    # Or through the overlay, which provides pkgs.paredit-cli,
    # pkgs.paredit-lint, pkgs.paredit-format, and pkgs.paredit-format-files:
    #   nixpkgs.overlays = [ paredit-cli.overlays.default ];
  };
}
```

The flake also exports `lib.${system}.mkLintCheck`, `mkFormatCheck`, and
`treefmtFormatter` for wiring structural checks into another project's
`nix flake check` — see [Integrations](guide/integrations.md).

## Install with Cargo

```sh
cargo install --git https://github.com/nerima-lisp/paredit-cli --locked
```

The minimum supported Rust version is `1.85` (edition 2024).

## Pin automation

The examples above follow the latest default branch. For CI, production
automation, or a reproducible developer environment, pin a release tag:

```sh
nix run github:nerima-lisp/paredit-cli/v1.4.0 -- --help
nix profile install github:nerima-lisp/paredit-cli/v1.4.0
cargo install --git https://github.com/nerima-lisp/paredit-cli --tag v1.4.0 --locked
```

A tag is the release artifact — the crate is not published to a package
registry. For the strongest guarantee, pin an immutable commit instead and
replace `<commit>` with a full commit SHA that you have reviewed:

```sh
nix run github:nerima-lisp/paredit-cli/<commit> -- --help
nix profile install github:nerima-lisp/paredit-cli/<commit>
cargo install --git https://github.com/nerima-lisp/paredit-cli --rev <commit> --locked
```

When upgrading a pin, inspect the release notes and compare the machine-readable
command catalog before accepting the new revision:

```sh
paredit inspect capabilities --output json
```

## Verify

```sh
paredit --help
paredit inspect --help
```

## Run one task

Check a file's structure, format it, and rename a symbol across it:

```sh
paredit inspect check --file source.lisp
paredit edit format --file source.lisp
paredit refactor rename-symbol --file source.lisp --from old-name --to new-name
```

`inspect` is read-only. `edit` writes to stdout by default — add `--diff` for a
unified diff, or `--write` to update the file in place. `refactor` is the
planned, previewable path for semantic changes; see the
[refactor workflow](guide/workflows.md).

## Next steps

- [Command Model](reference/api.md) for the full subcommand surface.
- [Selecting Forms](reference/selectors.md) for tree paths and byte offsets.
- [Agent Interface](guide/agents.md) for exit codes, JSON output, and
  `paredit inspect capabilities`.
- [Development](project/development.md) for the contributor loop.
