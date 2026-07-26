#![doc = include_str!("../README.md")]

pub mod args;
pub mod gate;
// `io`, `diff` and `macos_acl` are `shared`'s submodules, declared there with
// `#[path]` so they can sit as sibling files. `macos_acl` is macOS-only.
pub mod shared;
