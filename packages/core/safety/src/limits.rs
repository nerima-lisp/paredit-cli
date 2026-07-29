//! How much one invocation is allowed to read.
//!
//! Every bound here already existed, as a constant somewhere in the tree:
//! `MAX_SOURCE_INPUT_BYTES` beside the reader, `WorkspaceLimits` beside the
//! traversal. Constants are the right default and the wrong ceiling — a 64 MB
//! document is fine on a workstation and an out-of-memory kill in a 512 MB CI
//! container, and neither caller could say so.
//!
//! Collecting them into one type has a second effect worth more than the
//! configurability: the bounds become enumerable. `ResourceLimits::rows`
//! renders the effective set, so a run can state what it refused to exceed
//! instead of failing with a number that appears nowhere in the help text.

use std::fmt;
use std::sync::OnceLock;

use thiserror::Error;

/// The default single-document read ceiling, in bytes.
///
/// Matches the `MAX_SOURCE_INPUT_BYTES` that the reader has always applied.
pub const DEFAULT_MAX_INPUT_BYTES: u64 = 64 * 1024 * 1024;
/// The default per-file ceiling applied during workspace traversal, in bytes.
pub const DEFAULT_MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
/// The default cumulative read ceiling for one traversal, in bytes.
pub const DEFAULT_MAX_TOTAL_BYTES: u64 = 512 * 1024 * 1024;
/// The default ceiling on files a single traversal may yield.
pub const DEFAULT_MAX_FILES: usize = 50_000;
/// The default ceiling on directory entries a single traversal may visit.
pub const DEFAULT_MAX_ENTRIES: usize = 100_000;
/// The default ceiling on raw root arguments, applied before resolution.
pub const DEFAULT_MAX_ROOTS: usize = 1_024;

/// A bound that was asked for but cannot be honoured.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum LimitError {
    #[error("{flag} must be greater than zero")]
    Zero { flag: &'static str },
    #[error("{flag} value {value:?} is not a byte size (try 4096, 512KB, or 2MiB)")]
    Unparsable { flag: &'static str, value: String },
    #[error("{flag} value {value:?} overflows a 64-bit byte count")]
    Overflow { flag: &'static str, value: String },
    #[error(
        "{flag} value {requested} exceeds the built-in ceiling {maximum}; limits may be lowered, never raised"
    )]
    AboveCeiling {
        flag: &'static str,
        requested: u64,
        maximum: u64,
    },
}

/// The effective bounds for one invocation.
///
/// Every field defaults to the constant that was previously compiled in, so a
/// command that never sets one behaves exactly as it did before this type
/// existed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceLimits {
    /// Ceiling on a single document read through the reader.
    pub max_input_bytes: u64,
    /// Ceiling on one file encountered during traversal.
    pub max_file_bytes: u64,
    /// Ceiling on the bytes one traversal may read in total.
    pub max_total_bytes: u64,
    /// Ceiling on the files one traversal may yield.
    pub max_files: usize,
    /// Ceiling on the directory entries one traversal may visit.
    pub max_entries: usize,
    /// Ceiling on raw root arguments, applied before filesystem resolution.
    pub max_roots: usize,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_input_bytes: DEFAULT_MAX_INPUT_BYTES,
            max_file_bytes: DEFAULT_MAX_FILE_BYTES,
            max_total_bytes: DEFAULT_MAX_TOTAL_BYTES,
            max_files: DEFAULT_MAX_FILES,
            max_entries: DEFAULT_MAX_ENTRIES,
            max_roots: DEFAULT_MAX_ROOTS,
        }
    }
}

impl ResourceLimits {
    /// Applies the byte overrides a caller supplied, leaving the rest at their
    /// defaults.
    ///
    /// Overrides may only *lower* a bound. A flag that could raise the ceiling
    /// would let an untrusted `paredit.toml` or CI variable re-enable exactly
    /// the exhaustion this type exists to prevent, so a larger value is an
    /// error rather than a silent clamp: a caller that asked for 2 GB and got
    /// 512 MB should be told, not quietly obeyed in part.
    pub fn with_overrides(
        mut self,
        overrides: &ResourceLimitOverrides,
    ) -> Result<Self, LimitError> {
        if let Some(value) = overrides.max_input_bytes {
            self.max_input_bytes =
                check_ceiling("--max-input-bytes", value, DEFAULT_MAX_INPUT_BYTES)?;
        }
        if let Some(value) = overrides.max_file_bytes {
            self.max_file_bytes = check_ceiling("--max-file-bytes", value, DEFAULT_MAX_FILE_BYTES)?;
        }
        if let Some(value) = overrides.max_total_bytes {
            self.max_total_bytes =
                check_ceiling("--max-total-bytes", value, DEFAULT_MAX_TOTAL_BYTES)?;
        }
        if let Some(value) = overrides.max_files {
            if value == 0 {
                return Err(LimitError::Zero {
                    flag: "--max-files",
                });
            }
            if value > DEFAULT_MAX_FILES {
                return Err(LimitError::AboveCeiling {
                    flag: "--max-files",
                    requested: as_u64(value),
                    maximum: as_u64(DEFAULT_MAX_FILES),
                });
            }
            self.max_files = value;
        }
        Ok(self)
    }

    /// The effective bounds as ordered `(name, value)` rows.
    ///
    /// Ordered and complete so a JSON report can restate the whole set without
    /// a second list drifting from this one.
    #[must_use]
    pub fn rows(&self) -> Vec<(&'static str, u64)> {
        vec![
            ("max_input_bytes", self.max_input_bytes),
            ("max_file_bytes", self.max_file_bytes),
            ("max_total_bytes", self.max_total_bytes),
            ("max_files", as_u64(self.max_files)),
            ("max_entries", as_u64(self.max_entries)),
            ("max_roots", as_u64(self.max_roots)),
        ]
    }

    /// Whether any bound differs from the compiled-in default.
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// A count as a `u64`, saturating on the 128-bit-`usize` platforms that do not
/// exist. Present so the limit rows contain no `as` cast to audit.
fn as_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

const fn check_ceiling(flag: &'static str, value: u64, maximum: u64) -> Result<u64, LimitError> {
    if value == 0 {
        return Err(LimitError::Zero { flag });
    }
    if value > maximum {
        return Err(LimitError::AboveCeiling {
            flag,
            requested: value,
            maximum,
        });
    }
    Ok(value)
}

/// The bounds this process resolved at startup.
///
/// A process-global rather than a parameter because the alternative is
/// threading one immutable value through 130 command workflows and the reader
/// beneath them, where every new command is a chance to forget it — and a
/// forgotten limit reads as "unbounded", which is the failure this type
/// exists to prevent. Written once, from the composition root, before any
/// command runs.
static EFFECTIVE_LIMITS: OnceLock<ResourceLimits> = OnceLock::new();

/// Publishes the bounds for this process. The first call wins.
///
/// Returns the already-installed value on a second call rather than
/// overwriting it: a limit that can be raised mid-run is not a limit.
pub fn install(limits: ResourceLimits) -> Result<(), ResourceLimits> {
    EFFECTIVE_LIMITS.set(limits).map_err(|_| effective())
}

/// The bounds in force, or the defaults when nothing was installed.
///
/// Library callers and tests that never call [`install`] see exactly the
/// behaviour that predates this module.
#[must_use]
pub fn effective() -> ResourceLimits {
    EFFECTIVE_LIMITS.get().copied().unwrap_or_default()
}

/// How many workers a multi-file analysis may use.
///
/// Separate from [`ResourceLimits`] because it is not a ratchet: a caller may
/// legitimately ask for *more* parallelism than the default as well as less,
/// and unlike a byte ceiling, a large value cannot exhaust anything a small
/// one would not.
static EFFECTIVE_JOBS: OnceLock<usize> = OnceLock::new();

/// Publishes the worker count for this process. The first call wins.
///
/// `0` means "as many as the machine reports", which is also the default.
pub fn install_jobs(jobs: usize) -> Result<(), usize> {
    EFFECTIVE_JOBS.set(jobs).map_err(|_| effective_jobs())
}

/// The worker count in force, or `0` for "as many as the machine reports".
#[must_use]
pub fn effective_jobs() -> usize {
    EFFECTIVE_JOBS.get().copied().unwrap_or(0)
}

/// Reads the `PAREDIT_MAX_*` environment overrides.
///
/// The environment is how a container states its own budget, and it reaches
/// commands whose flags a CI harness does not control. Same ratchet as the
/// flags: an environment variable may lower a bound, never raise it.
pub fn overrides_from_environment() -> Result<ResourceLimitOverrides, LimitError> {
    Ok(ResourceLimitOverrides {
        max_input_bytes: environment_bytes("PAREDIT_MAX_INPUT_BYTES")?,
        max_file_bytes: environment_bytes("PAREDIT_MAX_FILE_BYTES")?,
        max_total_bytes: environment_bytes("PAREDIT_MAX_TOTAL_BYTES")?,
        max_files: environment_count("PAREDIT_MAX_FILES")?,
    })
}

/// Parses one byte-size environment variable, naming the variable in any
/// rejection so the operator knows which one to fix.
fn environment_bytes(name: &'static str) -> Result<Option<u64>, LimitError> {
    std::env::var(name)
        .ok()
        .map(|value| parse_byte_size(name, &value))
        .transpose()
}

fn environment_count(name: &'static str) -> Result<Option<usize>, LimitError> {
    std::env::var(name)
        .ok()
        .map(|value| {
            value
                .trim()
                .parse::<usize>()
                .map_err(|_| LimitError::Unparsable { flag: name, value })
        })
        .transpose()
}

/// The subset of bounds a caller asked to change, before validation.
#[derive(Debug, Clone, Default)]
pub struct ResourceLimitOverrides {
    pub max_input_bytes: Option<u64>,
    pub max_file_bytes: Option<u64>,
    pub max_total_bytes: Option<u64>,
    pub max_files: Option<usize>,
}

impl ResourceLimitOverrides {
    /// Parses the byte-size spellings a caller typed.
    ///
    /// A rejection quotes the text as written rather than the number it parsed
    /// to, so `--max-file-bytes 12MBB` names `12MBB` and not `12`.
    pub fn from_text(
        max_input_bytes: Option<&str>,
        max_file_bytes: Option<&str>,
        max_total_bytes: Option<&str>,
        max_files: Option<usize>,
    ) -> Result<Self, LimitError> {
        Ok(Self {
            max_input_bytes: parse_optional("--max-input-bytes", max_input_bytes)?,
            max_file_bytes: parse_optional("--max-file-bytes", max_file_bytes)?,
            max_total_bytes: parse_optional("--max-total-bytes", max_total_bytes)?,
            max_files,
        })
    }

    /// Whether the caller changed anything at all.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.max_input_bytes.is_none()
            && self.max_file_bytes.is_none()
            && self.max_total_bytes.is_none()
            && self.max_files.is_none()
    }
}

fn parse_optional(flag: &'static str, value: Option<&str>) -> Result<Option<u64>, LimitError> {
    value.map(|value| parse_byte_size(flag, value)).transpose()
}

/// Parses a byte size written as a plain count or with a unit suffix.
///
/// Both the decimal (`KB`, `MB`, `GB`) and binary (`KiB`, `MiB`, `GiB`)
/// families are accepted, case-insensitively, because a limit typed by a
/// human and a limit copied from `ulimit` output rarely agree on which one
/// they mean. A bare number is bytes.
pub fn parse_byte_size(flag: &'static str, value: &str) -> Result<u64, LimitError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(LimitError::Unparsable {
            flag,
            value: value.to_owned(),
        });
    }

    let digits_end = trimmed
        .find(|character: char| !character.is_ascii_digit())
        .unwrap_or(trimmed.len());
    let (digits, suffix) = trimmed.split_at(digits_end);
    if digits.is_empty() {
        return Err(LimitError::Unparsable {
            flag,
            value: value.to_owned(),
        });
    }

    let multiplier = match suffix.trim().to_ascii_lowercase().as_str() {
        "" | "b" => 1_u64,
        "k" | "kb" => 1_000,
        "m" | "mb" => 1_000_000,
        "g" | "gb" => 1_000_000_000,
        "kib" => 1024,
        "mib" => 1024 * 1024,
        "gib" => 1024 * 1024 * 1024,
        _ => {
            return Err(LimitError::Unparsable {
                flag,
                value: value.to_owned(),
            });
        }
    };

    digits
        .parse::<u64>()
        .ok()
        .and_then(|count| count.checked_mul(multiplier))
        .ok_or_else(|| LimitError::Overflow {
            flag,
            value: value.to_owned(),
        })
}

/// Renders a byte count the way the limit rows print it.
#[must_use]
pub fn format_byte_size(bytes: u64) -> String {
    ByteSize(bytes).to_string()
}

struct ByteSize(u64);

impl fmt::Display for ByteSize {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Exact binary multiples print as such; everything else stays a plain
        // count, because a rounded "1.3MiB" in an error message cannot be
        // pasted back in as a flag value.
        for (unit, scale) in [
            ("GiB", 1024 * 1024 * 1024_u64),
            ("MiB", 1024 * 1024),
            ("KiB", 1024),
        ] {
            if self.0 >= scale && self.0 % scale == 0 {
                return write!(formatter, "{}{unit}", self.0 / scale);
            }
        }
        write!(formatter, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_MAX_FILE_BYTES, DEFAULT_MAX_INPUT_BYTES, DEFAULT_MAX_TOTAL_BYTES, LimitError,
        ResourceLimitOverrides, ResourceLimits, format_byte_size, parse_byte_size,
    };

    #[test]
    fn defaults_reproduce_the_previously_compiled_in_constants() {
        let limits = ResourceLimits::default();
        assert_eq!(limits.max_input_bytes, 64 * 1024 * 1024);
        assert_eq!(limits.max_file_bytes, 16 * 1024 * 1024);
        assert_eq!(limits.max_total_bytes, 512 * 1024 * 1024);
        assert_eq!(limits.max_files, 50_000);
        assert_eq!(limits.max_entries, 100_000);
        assert_eq!(limits.max_roots, 1_024);
        assert!(limits.is_default());
    }

    #[test]
    fn plain_counts_and_both_unit_families_parse() {
        for (text, expected) in [
            ("4096", 4096_u64),
            ("512", 512),
            ("1b", 1),
            ("2k", 2_000),
            ("2KB", 2_000),
            ("2KiB", 2_048),
            ("3mb", 3_000_000),
            ("3MiB", 3 * 1024 * 1024),
            ("1GB", 1_000_000_000),
            ("1gib", 1024 * 1024 * 1024),
        ] {
            assert_eq!(
                parse_byte_size("--max-total-bytes", text),
                Ok(expected),
                "parsing {text}"
            );
        }
    }

    #[test]
    fn a_unit_that_is_not_a_unit_is_rejected_with_the_text_typed() {
        assert_eq!(
            parse_byte_size("--max-file-bytes", "12 furlongs"),
            Err(LimitError::Unparsable {
                flag: "--max-file-bytes",
                value: "12 furlongs".to_owned(),
            })
        );
        assert_eq!(
            parse_byte_size("--max-file-bytes", "MB"),
            Err(LimitError::Unparsable {
                flag: "--max-file-bytes",
                value: "MB".to_owned(),
            })
        );
    }

    #[test]
    fn a_size_that_overflows_a_u64_is_an_error_not_a_wrap() {
        assert_eq!(
            parse_byte_size("--max-total-bytes", "99999999999999GiB"),
            Err(LimitError::Overflow {
                flag: "--max-total-bytes",
                value: "99999999999999GiB".to_owned(),
            })
        );
    }

    #[test]
    fn overrides_lower_a_bound() {
        let overrides =
            ResourceLimitOverrides::from_text(Some("1MiB"), Some("512KiB"), Some("8MiB"), Some(10))
                .expect("parse overrides");
        let limits = ResourceLimits::default()
            .with_overrides(&overrides)
            .expect("apply overrides");

        assert_eq!(limits.max_input_bytes, 1024 * 1024);
        assert_eq!(limits.max_file_bytes, 512 * 1024);
        assert_eq!(limits.max_total_bytes, 8 * 1024 * 1024);
        assert_eq!(limits.max_files, 10);
        assert!(!limits.is_default());
    }

    /// A limit flag is a ratchet. Accepting a larger value would make the
    /// bound a suggestion, and the exhaustion it prevents is exactly what an
    /// attacker-supplied configuration would want to switch off.
    #[test]
    fn overrides_may_not_raise_a_bound() {
        let overrides = ResourceLimitOverrides::from_text(Some("1GiB"), None, None, None)
            .expect("parse overrides");

        assert_eq!(
            ResourceLimits::default().with_overrides(&overrides),
            Err(LimitError::AboveCeiling {
                flag: "--max-input-bytes",
                requested: 1024 * 1024 * 1024,
                maximum: DEFAULT_MAX_INPUT_BYTES,
            })
        );
    }

    #[test]
    fn a_zero_bound_is_rejected_rather_than_disabling_every_read() {
        let overrides = ResourceLimitOverrides::from_text(None, Some("0"), None, None)
            .expect("parse overrides");
        assert_eq!(
            ResourceLimits::default().with_overrides(&overrides),
            Err(LimitError::Zero {
                flag: "--max-file-bytes"
            })
        );

        let overrides =
            ResourceLimitOverrides::from_text(None, None, None, Some(0)).expect("parse overrides");
        assert_eq!(
            ResourceLimits::default().with_overrides(&overrides),
            Err(LimitError::Zero {
                flag: "--max-files"
            })
        );
    }

    #[test]
    fn empty_overrides_leave_every_default_in_place() {
        let overrides = ResourceLimitOverrides::from_text(None, None, None, None)
            .expect("parse empty overrides");
        assert!(overrides.is_empty());
        assert_eq!(
            ResourceLimits::default()
                .with_overrides(&overrides)
                .expect("apply empty overrides"),
            ResourceLimits::default()
        );
    }

    #[test]
    fn rows_enumerate_every_bound_in_a_fixed_order() {
        let rows = ResourceLimits::default().rows();
        assert_eq!(
            rows.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
            [
                "max_input_bytes",
                "max_file_bytes",
                "max_total_bytes",
                "max_files",
                "max_entries",
                "max_roots",
            ]
        );
        assert_eq!(rows[0].1, DEFAULT_MAX_INPUT_BYTES);
        assert_eq!(rows[1].1, DEFAULT_MAX_FILE_BYTES);
        assert_eq!(rows[2].1, DEFAULT_MAX_TOTAL_BYTES);
    }

    /// A formatted size must be usable as a flag value; that is the whole
    /// reason non-multiples stay a plain count.
    #[test]
    fn formatted_sizes_round_trip_through_the_parser() {
        for bytes in [0_u64, 1, 999, 1024, 1536, 1024 * 1024, 64 * 1024 * 1024] {
            let text = format_byte_size(bytes);
            if bytes == 0 {
                assert_eq!(text, "0");
                continue;
            }
            assert_eq!(
                parse_byte_size("--max-total-bytes", &text),
                Ok(bytes),
                "round-tripping {bytes} through {text:?}"
            );
        }
    }
}
