## Summary

- describe the change
- describe the user-visible effect

## Verification

- [ ] `cargo fmt --all`
- [ ] `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] `cargo test`
- [ ] `cargo nextest run --locked`
- [ ] `cargo doc --no-deps`
- [ ] `nix flake check`
- [ ] `nix build .#`

## Notes for Reviewers

- risk areas:
- follow-up work:
