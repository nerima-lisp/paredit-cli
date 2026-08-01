{
  description = "paredit-cli: structure-editing CLI for S-expression refactoring";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    # crane splits "compile the 181 locked dependencies" out of "compile this
    # workspace" into a derivation keyed on Cargo.lock alone. Without it every
    # Rust check is one monolithic `buildRustPackage` that recompiles the whole
    # dependency graph from scratch, and a one-byte source change invalidates
    # all of them at once.
    crane.url = "github:ipetkov/crane";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    treefmt-nix = {
      url = "github:numtide/treefmt-nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      crane,
      rust-overlay,
      treefmt-nix,
    }:
    let
      inherit (nixpkgs) lib;

      cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);
      msrvToolchainVersion = "${cargoToml.package.rust-version}.0";

      # x86_64-linux and nothing else. Only what a gate verifies is declared,
      # and the only gate is CI. aarch64-linux, x86_64-darwin and
      # aarch64-darwin were declared without anything ever building them, which
      # advertised support no run confirms. Every output -- packages, checks,
      # apps AND devShells -- comes from this one list, so development happens
      # on Linux. See PACKAGE_STANDARD.md "systems".
      systems = [
        "x86_64-linux"
      ];

      pkgsFor = lib.genAttrs systems (
        system:
        import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        }
      );

      forAllSystems = f: lib.genAttrs systems (system: f pkgsFor.${system});

      commonCrateArgs = {
        # The whole tracked tree is the build input on purpose: the crate's
        # contract tests assert on README, docs/, action.yml, and workflow
        # files, so documentation belongs to the test fixture surface. This is
        # also why crane's `cleanCargoSource` is not used - it strips exactly
        # those files, and every contract test would fail to find them.
        src = ./.;
        pname = "paredit-cli";
        version = cargoToml.package.version;
        strictDeps = true;
      };

      # Every cargo derivation for one `pkgs`, wired so that no two of them
      # ever compile the same thing twice:
      #
      #   depsRelease -> package            release profile, this is what ships
      #   depsDev     -> clippy, nextest    dev profile, one artifact for both
      #   depsMsrv    -> msrv               dev profile, 1.85 toolchain
      #
      # A `cargoArtifacts` derivation is a packed-up cargo `target` directory
      # built from dummified sources, so its hash depends on Cargo.lock and the
      # member manifests and *not* on any .rs file. That is the whole point: a
      # pull request that only edits Rust sources substitutes all three straight
      # from the binary cache instead of recompiling 181 crates four times.
      #
      # Every attribute below is lazy, so a consumer applying `overlays.default`
      # to their own nixpkgs only forces `package`; `msrv` (which needs the
      # rust-overlay for a pinned 1.85 toolchain) is never evaluated for them.
      mkCargoSet =
        pkgs:
        let
          craneLib = crane.mkLib pkgs;
          msrvCraneLib = craneLib.overrideToolchain (p: p.rust-bin.stable.${msrvToolchainVersion}.default);

          # `doCheck = false` keeps `cargo test --no-run` out of the release
          # dependency build: nothing downstream of it compiles a test target.
          depsRelease = craneLib.buildDepsOnly (
            commonCrateArgs
            // {
              pname = "paredit-cli-release";
              doCheck = false;
              cargoCheckExtraArgs = "";
            }
          );

          # Left on crane's default `doCheck = true`, which adds a
          # `cargo test --no-run` pass so proptest, criterion and assert_cmd
          # land in the artifact too - `clippy --all-targets` and nextest both
          # need the dev-dependency graph.
          depsDev = craneLib.buildDepsOnly (
            commonCrateArgs
            // {
              pname = "paredit-cli-dev";
              CARGO_PROFILE = "dev";
            }
          );

          depsMsrv = msrvCraneLib.buildDepsOnly (
            commonCrateArgs
            // {
              pname = "paredit-cli-msrv";
              CARGO_PROFILE = "dev";
            }
          );

          # PR-only fast path: thin-LTO, consumed solely by
          # treefmt-pr-check/lint-format-integration-pr-check, which need a
          # *working* paredit binary to run its formatter/lint plugin, not the
          # binary that ships. `package` keeps building depsRelease under fat
          # LTO, unaffected by this.
          depsPrCheck = craneLib.buildDepsOnly (
            commonCrateArgs
            // {
              pname = "paredit-cli-pr-check";
              CARGO_PROFILE = "pr-check";
              doCheck = false;
              cargoCheckExtraArgs = "";
            }
          );
        in
        {
          package = craneLib.buildPackage (
            commonCrateArgs
            // {
              cargoArtifacts = depsRelease;
              # The test suite belongs to the `nextest` check and nowhere else.
              # crane's default would compile every test target a second time
              # under `lto = "fat"` + `codegen-units = 1` and then re-run all
              # 5656 tests - measured at ~1100s of CPU for no extra coverage.
              doCheck = false;
              meta = {
                description = cargoToml.package.description;
                homepage = cargoToml.package.homepage;
                license = lib.licenses.mit;
                mainProgram = "paredit";
              };
            }
          );

          # Same source, thin-LTO: see depsPrCheck above.
          packagePrCheck = craneLib.buildPackage (
            commonCrateArgs
            // {
              pname = "paredit-cli-pr-check";
              cargoArtifacts = depsPrCheck;
              CARGO_PROFILE = "pr-check";
              doCheck = false;
            }
          );

          clippy = craneLib.cargoClippy (
            commonCrateArgs
            // {
              cargoArtifacts = depsDev;
              CARGO_PROFILE = "dev";
              cargoClippyExtraArgs = "--all-targets --all-features -- -D warnings";
              # A check has no consumer for its `target` directory, and packing
              # one up costs more than the check itself at this workspace size.
              doInstallCargoArtifacts = false;
            }
          );

          nextest = craneLib.cargoNextest (
            commonCrateArgs
            // {
              cargoArtifacts = depsDev;
              CARGO_PROFILE = "dev";
              cargoExtraArgs = "--locked";
              doInstallCargoArtifacts = false;
            }
          );

          # MSRV asks exactly one question - "does this still compile on
          # 1.85?" - so it stops at `cargo check`. Codegen and a third full
          # test run answer nothing the other checks have not answered already.
          msrv = msrvCraneLib.mkCargoDerivation (
            commonCrateArgs
            // {
              pname = "paredit-cli-msrv";
              cargoArtifacts = depsMsrv;
              CARGO_PROFILE = "dev";
              buildPhaseCargoCommand = "cargoWithProfile check --locked --all-targets";
              doInstallCargoArtifacts = false;
            }
          );
        };

      cargoFor = lib.genAttrs systems (system: mkCargoSet pkgsFor.${system});

      mkParedit = pkgs: (mkCargoSet pkgs).package;
      mkParateditPrCheck = pkgs: (mkCargoSet pkgs).packagePrCheck;

      mkDocs =
        pkgs:
        pkgs.stdenvNoCC.mkDerivation {
          pname = "paredit-cli-docs";
          version = cargoToml.package.version;
          src = lib.fileset.toSource {
            root = ./docs;
            fileset = lib.fileset.unions [
              ./docs/mkdocs.yml
              ./docs/src
            ];
          };
          nativeBuildInputs = [ pkgs.python3Packages.mkdocs-material ];
          # Build fully offline: Material for MkDocs bundles all of its assets,
          # so no network access is required inside the Nix sandbox. --strict
          # promotes broken links and unlisted pages to build failures.
          buildPhase = ''
            runHook preBuild
            mkdocs build --strict --config-file mkdocs.yml --site-dir "$out"
            runHook postBuild
          '';
          dontInstall = true;
          meta = {
            description = "Rendered MkDocs (Material) documentation for paredit-cli";
            homepage = cargoToml.package.homepage;
            license = lib.licenses.mit;
          };
        };

      # `paredit` is threaded through explicitly (rather than each caller
      # hardcoding `mkParedit pkgs`) so the PR-only fast-path checks can
      # supply `mkParateditPrCheck pkgs` instead without duplicating the
      # script body.
      mkLintFor =
        pkgs: paredit:
        pkgs.writeShellApplication {
          name = "paredit-lint";
          runtimeInputs = [
            paredit
            pkgs.jq
          ];
          meta.description = "Fail when discovered Lisp sources contain structural parse errors";
          text = ''
            # Structural lint gate: fail when any discovered Lisp source does
            # not parse as a balanced S-expression document.
            if [ "$#" -eq 0 ]; then
              set -- .
            fi
            report=$(paredit inspect workspace --output json "$@")
            jq -r '.files[] | select(.status == "parse-error") | "\(.path): \(.error)"' <<<"$report"
            if [ "''${GITHUB_ACTIONS:-}" = "true" ]; then
              jq -r '.files[] | select(.status == "parse-error") | "::error file=\(.path)::structural parse error: \(.error)"' <<<"$report"
            fi
            errors=$(jq -r '.parse_error_count' <<<"$report")
            parsed=$(jq -r '.parsed_count' <<<"$report")
            echo "paredit-lint: $parsed file(s) parsed, $errors parse error(s)"
            [ "$errors" -eq 0 ]
          '';
        };
      mkLint = pkgs: mkLintFor pkgs (mkParedit pkgs);
      mkLintPrCheck = pkgs: mkLintFor pkgs (mkParateditPrCheck pkgs);

      mkFormatFor =
        pkgs: paredit:
        pkgs.writeShellApplication {
          name = "paredit-format";
          runtimeInputs = [
            paredit
            pkgs.jq
          ];
          meta.description = "Rewrite discovered Lisp sources into canonical paredit edit format";
          text = ''
            # Canonical formatter over discovered Lisp sources.
            # Default mode rewrites files in place; --check only reports
            # files whose canonical rendering differs and exits non-zero.
            check=0
            args=()
            for arg in "$@"; do
              case "$arg" in
                --check) check=1 ;;
                *) args+=("$arg") ;;
              esac
            done
            if [ "''${#args[@]}" -eq 0 ]; then
              args=(.)
            fi
            report=$(paredit inspect workspace --output json "''${args[@]}")
            fail=0
            changed=0
            while IFS= read -r file; do
              formatted=$(paredit edit format --file "$file")
              if ! printf '%s\n' "$formatted" | cmp -s - "$file"; then
                if [ "$check" -eq 1 ]; then
                  echo "would reformat: $file"
                  if [ "''${GITHUB_ACTIONS:-}" = "true" ]; then
                    echo "::error file=$file::not in canonical paredit edit format (run: paredit-format $file)"
                  fi
                  fail=1
                else
                  printf '%s\n' "$formatted" > "$file"
                  echo "reformatted: $file"
                  changed=$((changed + 1))
                fi
              fi
            done < <(jq -r '.files[] | select(.status == "parsed") | .path' <<<"$report")
            errors=$(jq -r '.parse_error_count' <<<"$report")
            if [ "$errors" -gt 0 ]; then
              echo "paredit-format: skipped $errors file(s) with parse errors (run paredit-lint first)"
            fi
            if [ "$check" -eq 1 ]; then
              [ "$fail" -eq 0 ]
            else
              echo "paredit-format: reformatted $changed file(s)"
            fi
          '';
        };
      mkFormat = pkgs: mkFormatFor pkgs (mkParedit pkgs);
      mkFormatPrCheck = pkgs: mkFormatFor pkgs (mkParateditPrCheck pkgs);

      # Mirrors Dialect::from_extension in src/domain/dialect/mod.rs; keep the
      # two in sync so the Nix helpers cover every file paredit can parse.
      lispIncludes = [
        "*.lisp"
        "*.lsp"
        "*.cl"
        "*.asd"
        "*.el"
        "*.lfe"
        "*.scm"
        "*.ss"
        "*.sld"
        "*.sls"
        "*.sps"
        "*.rkt"
        "*.rktl"
        "*.rktd"
        "*.clj"
        "*.cljc"
        "*.cljs"
        "*.cljd"
        "*.edn"
        "*.bb"
        "*.hy"
        "*.carp"
        "*.janet"
        "*.fnl"
      ];

      mkFormatFilesFor =
        pkgs: paredit:
        pkgs.writeShellApplication {
          name = "paredit-format-files";
          runtimeInputs = [ paredit ];
          meta.description = "treefmt-style formatter: rewrite each argument file in place";
          text = ''
            # treefmt-style formatter: rewrite each argument file in place.
            # Files that do not parse are left untouched; paredit-lint owns
            # structural failures.
            for file in "$@"; do
              if formatted=$(paredit edit format --file "$file"); then
                printf '%s\n' "$formatted" | cmp -s - "$file" || printf '%s\n' "$formatted" > "$file"
              fi
            done
          '';
        };
      mkFormatFiles = pkgs: mkFormatFilesFor pkgs (mkParedit pkgs);
      mkFormatFilesPrCheck = pkgs: mkFormatFilesFor pkgs (mkParateditPrCheck pkgs);

      mkTreefmtModuleFor = pkgs: formatFiles: {
        projectRootFile = "flake.nix";
        programs.rustfmt.enable = true;
        programs.rustfmt.edition = "2024";
        programs.nixfmt.enable = true;
        settings.formatter.paredit = {
          command = lib.getExe formatFiles;
          includes = lispIncludes;
          # Test fixtures are byte-exact parser inputs; formatting them would
          # change the spans the tests assert on.
          #
          # A fuzz corpus is the same argument at its strongest: a seed's whole
          # value is its exact bytes, and a crasher artifact that a formatter
          # rewrote has stopped reproducing the crash it was saved for.
          excludes = [
            "tests/fixtures/*"
            "fuzz/corpus/*"
            "fuzz/artifacts/*"
          ];
        };
      };
      mkTreefmtModule = pkgs: mkTreefmtModuleFor pkgs (mkFormatFiles pkgs);
      mkTreefmtModulePrCheck = pkgs: mkTreefmtModuleFor pkgs (mkFormatFilesPrCheck pkgs);

      treefmtFor = lib.genAttrs systems (
        system: treefmt-nix.lib.evalModule pkgsFor.${system} (mkTreefmtModule pkgsFor.${system})
      );
      # PR-only fast path: same treefmt config, thin-LTO paredit formatter
      # binary. See mkParateditPrCheck / [profile.pr-check] in Cargo.toml.
      treefmtPrCheckFor = lib.genAttrs systems (
        system: treefmt-nix.lib.evalModule pkgsFor.${system} (mkTreefmtModulePrCheck pkgsFor.${system})
      );

      # The checks that actually build something, without the conformance
      # aliases layered on below. CI turns `builtins.attrNames` of this into one
      # GitHub job per check (see `lib.<system>.ciCheckNames`), and the aliases
      # have to stay out of that list: `default`, `formatting` and `docs` are
      # the *same* store paths as `nextest`, `treefmt` and `documentation`, so
      # including them would put three of the most expensive derivations on two
      # runners each with nothing shared between them.
      mkCoreChecks =
        system:
        let
          pkgs = pkgsFor.${system};
          cargoChecks = cargoFor.${system};
          # Shared body for lint-format-integration and its PR-only
          # thin-LTO sibling: only which paredit-lint/paredit-format
          # binaries are on PATH differs between the two.
          mkLintFormatIntegrationCheck =
            name: lintPkg: formatPkg:
            pkgs.runCommand name
              {
                nativeBuildInputs = [
                  lintPkg
                  formatPkg
                ];
              }
              ''
                mkdir demo
                printf '(defun ok (x)\n  (+ x 1))\n' > demo/good.lisp

                paredit-lint demo

                printf '(defun broken (' > demo/bad.lisp
                if paredit-lint demo; then
                  echo "expected paredit-lint to fail on demo/bad.lisp" >&2
                  exit 1
                fi
                rm demo/bad.lisp

                printf '(defun messy (x)\n(+ x\n1))\n' > demo/messy.lisp
                if paredit-format --check demo; then
                  echo "expected paredit-format --check to fail on demo/messy.lisp" >&2
                  exit 1
                fi
                paredit-format demo
                paredit-format --check demo

                touch $out
              '';
        in
        {
          treefmt = treefmtFor.${system}.config.build.check self;
          # PR-only fast path: same treefmt config, thin-LTO paredit binary.
          treefmt-pr-check = treefmtPrCheckFor.${system}.config.build.check self;
          actionlint =
            pkgs.runCommand "paredit-cli-actionlint"
              {
                nativeBuildInputs = [
                  pkgs.actionlint
                  pkgs.git
                ];
                src = self;
              }
              ''
                cp -r $src/. .
                chmod -R u+w .

                # actionlint locates the project root by walking up for a .git
                # directory, and `self` is a store copy that has none. Without a
                # project it cannot resolve a local `uses: ./.github/workflows/*`
                # reference, so it silently skips every [workflow-call] rule
                # instead of reporting that it could not check them.
                #
                # That blind spot let main.yml pass this gate while passing a
                # secret ci.yml did not declare, which is rejected when the run
                # is created: every push to main failed at startup, with no jobs
                # and no logs, for three days. An empty `git init` is enough --
                # no commit, no index, just the root marker.
                git init -q .

                actionlint -color .github/workflows/*.yml
                touch $out
              '';
          documentation =
            pkgs.runCommand "paredit-cli-documentation" { docs = self.packages.${system}.docs; }
              ''
                test -f "$docs/index.html"
                touch $out
              '';
          lint-format-integration =
            mkLintFormatIntegrationCheck "paredit-cli-lint-format-integration" (mkLint pkgs)
              (mkFormat pkgs);
          # PR-only fast path: same behavioural check, thin-LTO binaries.
          lint-format-integration-pr-check =
            mkLintFormatIntegrationCheck "paredit-cli-lint-format-integration-pr-check" (mkLintPrCheck pkgs)
              (mkFormatPrCheck pkgs);
          inherit (cargoChecks) clippy nextest msrv;
          package = self.packages.${system}.default;
        };
    in
    {
      packages = forAllSystems (pkgs: {
        default = mkParedit pkgs;
        docs = mkDocs pkgs;
        lint = mkLint pkgs;
        format = mkFormat pkgs;
        format-files = mkFormatFiles pkgs;
      });

      apps = lib.genAttrs systems (system: {
        default = {
          type = "app";
          program = lib.getExe self.packages.${system}.default;
          meta.description = "Run paredit-cli";
        };
        lint = {
          type = "app";
          program = lib.getExe self.packages.${system}.lint;
          meta.description = "Fail when discovered Lisp sources contain structural parse errors";
        };
        format = {
          type = "app";
          program = lib.getExe self.packages.${system}.format;
          meta.description = "Rewrite discovered Lisp sources into canonical paredit edit format (--check to verify only)";
        };
      });

      lib = lib.genAttrs systems (
        system:
        let
          pkgs = pkgsFor.${system};
        in
        {
          # The check names CI fans out over, one GitHub job each. Derived from
          # the checks themselves rather than restated in ci.yml, so a check
          # added here cannot silently stop being verified - and so the
          # conformance aliases, which are duplicate store paths, stay out.
          ciCheckNames = builtins.attrNames (mkCoreChecks system);
          treefmtFormatter = {
            command = lib.getExe (mkFormatFiles pkgs);
            includes = lispIncludes;
          };
          mkLintCheck =
            {
              src,
              name ? "paredit-lint-check",
            }:
            pkgs.runCommand name { nativeBuildInputs = [ (mkLint pkgs) ]; } ''
              paredit-lint ${src}
              touch $out
            '';
          mkFormatCheck =
            {
              src,
              name ? "paredit-format-check",
            }:
            pkgs.runCommand name { nativeBuildInputs = [ (mkFormat pkgs) ]; } ''
              paredit-format --check ${src}
              touch $out
            '';
        }
      );

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          packages = [
            pkgs.rust-bin.stable.latest.default
            pkgs.rust-analyzer
            pkgs.cargo-nextest
            pkgs.cargo-audit
            pkgs.rustfmt
            pkgs.clippy
            pkgs.python3Packages.mkdocs-material
          ];
          shellHook = ''
            cat <<'USAGE_EOF'

            === paredit-cli Development Shell ===

            Development loop:
              cargo fmt --all
              cargo clippy --all-targets --all-features -- -D warnings
              cargo test
              cargo nextest run --locked
              cargo doc --no-deps

            Quick verification:
              nix flake check  # treefmt + actionlint + clippy + nextest + package/MSRV build/tests + lint/format integration

            Build and run:
              nix build .#              # result/bin/paredit
              nix build .#docs          # result/index.html (MkDocs Material site)
              nix run .# -- inspect check --file source.lisp
              nix run .#lint -- .       # structural lint gate
              nix run .#format -- --check .

            Format everything (Rust + Nix + Lisp via treefmt):
              nix fmt

            USAGE_EOF
          '';
        };
      });

      formatter = lib.genAttrs systems (system: treefmtFor.${system}.config.build.wrapper);

      checks = lib.genAttrs systems (
        system:
        let
          core = mkCoreChecks system;
          inherit (core) treefmt nextest documentation;
        in
        core
        // {
          # Aliases satisfying the org-wide conformance check's checks.{default,formatting,docs} naming convention.
          default = nextest;
          formatting = treefmt;
          docs = documentation;
        }
      );

      overlays.default = final: _prev: {
        paredit-cli = mkParedit final;
        paredit-lint = mkLint final;
        paredit-format = mkFormat final;
        paredit-format-files = mkFormatFiles final;
      };
    };
}
