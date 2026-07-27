use super::*;

use crate::domain::{dialect::Dialect, sexpr::SymbolName};
use proptest::prelude::*;

mod add_export;
mod merge_options;
mod rename;
mod sort_exports;
mod sort_options;

fn assert_ordered(input: &str, needles: &[&str]) {
    let mut offset = 0usize;
    for needle in needles {
        let position = input[offset..]
            .find(needle)
            .unwrap_or_else(|| panic!("missing {needle} in {input}"));
        offset += position + needle.len();
    }
}

fn package_name_strategy() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9-]{0,8}(\\.[a-z][a-z0-9-]{0,8}){0,2}".prop_filter("not reserved", |symbol| {
        !matches!(symbol.as_str(), "cl" | "common-lisp" | "keyword")
    })
}

fn symbol_strategy() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9-]{0,8}".prop_filter("not reserved", |symbol| {
        !matches!(
            symbol.as_str(),
            "defpackage" | "in-package" | "use" | "export"
        )
    })
}

fn option_fixture(index: usize) -> &'static str {
    match index {
        0 => "(:nicknames #:d)",
        1 => "(:use #:cl)",
        2 => "(:shadow #:car)",
        3 => "(:import-from #:dep #:x)",
        4 => "(:local-nicknames (#:dep #:dep.impl))",
        _ => "(:export #:main)",
    }
}

fn option_label(index: usize) -> &'static str {
    match index {
        0 => ":nicknames #:d",
        1 => ":use #:cl",
        2 => ":shadow #:car",
        3 => ":import-from #:dep",
        4 => ":local-nicknames",
        _ => ":export #:main",
    }
}
