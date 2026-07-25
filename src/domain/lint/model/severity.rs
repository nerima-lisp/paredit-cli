//! How serious a rule's findings are.

/// How serious a rule's findings are. `Error` marks a likely or certain bug;
/// `Warning` marks correct-but-redundant/non-idiomatic code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

impl Severity {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
        }
    }

    const fn rank(self) -> u8 {
        match self {
            Self::Error => 2,
            Self::Warning => 1,
        }
    }

    /// Whether this severity is at least as serious as `threshold`.
    pub fn at_least(self, threshold: Severity) -> bool {
        self.rank() >= threshold.rank()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_outranks_warning() {
        assert!(Severity::Error.at_least(Severity::Warning));
        assert!(!Severity::Warning.at_least(Severity::Error));
    }

    #[test]
    fn a_severity_meets_its_own_threshold() {
        assert!(Severity::Warning.at_least(Severity::Warning));
        assert!(Severity::Error.at_least(Severity::Error));
    }
}
