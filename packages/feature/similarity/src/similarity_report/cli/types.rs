use std::str::FromStr;

use crate::application::usecase::similarity_report::SimilarityErrorPolicy;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ErrorPolicy {
    Fail,
    Skip,
}

impl ErrorPolicy {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Fail => "fail",
            Self::Skip => "skip",
        }
    }
}

impl FromStr for ErrorPolicy {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "fail" => Ok(Self::Fail),
            "skip" => Ok(Self::Skip),
            _ => Err(format!("unknown error policy: {value}")),
        }
    }
}

impl From<ErrorPolicy> for SimilarityErrorPolicy {
    fn from(value: ErrorPolicy) -> Self {
        match value {
            ErrorPolicy::Fail => Self::Fail,
            ErrorPolicy::Skip => Self::Skip,
        }
    }
}
