#![doc = include_str!("../README.md")]

pub mod data_check_report;

// The composition root sees each slice's Args type and run fn (section 4.2).
pub use data_check_report::cli::{DataCheckReportArgs, data_check_report};
