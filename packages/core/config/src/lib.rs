#![doc = include_str!("../README.md")]

pub mod error;
pub mod load;
pub mod schema;
pub mod settings;
pub mod toml;

pub use error::{ConfigError, ConfigResult, Diagnostic, DiagnosticCode, Severity};
pub use load::{LoadOptions, Loaded, Source, load};
pub use settings::{Layer, Origin, Resolved, Settings, Vocabulary};
