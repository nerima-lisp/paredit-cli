pub(super) use super::*;

const fn cli_option_fixture(index: usize) -> &'static str {
    match index {
        0 => "(:nicknames #:d)",
        1 => "(:use #:cl)",
        2 => "(:shadow #:car)",
        3 => "(:import-from #:dep #:x)",
        4 => "(:local-nicknames (#:dep #:dep.impl))",
        _ => "(:export #:main)",
    }
}

mod add_export;
mod merge_options;
mod pbt;
mod rename;
mod report;
mod sort_exports;
mod sort_options;
