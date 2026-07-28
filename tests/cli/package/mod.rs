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

fn assert_substrings_in_order(input: &str, needles: &[&str]) {
    let mut offset = 0usize;
    for needle in needles {
        let position = input[offset..]
            .find(needle)
            .unwrap_or_else(|| panic!("missing {needle} in {input}"));
        offset += position + needle.len();
    }
}

mod add_export;
mod merge_options;
mod pbt;
mod rename;
mod report;
mod sort_exports;
mod sort_options;
