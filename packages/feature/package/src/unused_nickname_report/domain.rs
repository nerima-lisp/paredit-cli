//! Common Lisp "unused nickname" detection: a `defpackage` `:nicknames`
//! entry that is never used as a qualifier — not in a `:use`/`:import-from`
//! clause, not in a qualified symbol (`nickname:sym`/`nickname::sym`) —
//! anywhere in the analyzed fileset. A nickname is just another designator
//! for the same package, so it shares [`crate::domain::unused_package_report`]'s
//! reference collector outright: anywhere a primary package name could be
//! "used", a nickname could have been used instead, and this report asks
//! whether it ever was.
//!
//! Same caveat as `unused-packages`/`unused-exports`: a nickname reached
//! only from files outside the analyzed fileset cannot be told apart from
//! one nobody ever adopted — `--fail-on-unused` stays opt-in for exactly
//! that reason.
//!
//! Scope: Common Lisp only, since `:nicknames` is a `defpackage` option.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::common_lisp::{
    common_lisp_symbol_reference_needle, normalize_common_lisp_package_designator,
};
use crate::domain::dialect::Dialect;
use crate::domain::package_report::build_package_report;
use crate::domain::sexpr::{ByteSpan, SyntaxTree};

pub use crate::domain::unused_package_report::collect_referenced_package_names;

#[derive(Debug, Clone)]
pub struct DeclaredNickname {
    pub path: PathBuf,
    pub span: ByteSpan,
    pub package: String,
    pub nickname: String,
}

impl DeclaredNickname {
    pub fn new(
        path: PathBuf,
        span: ByteSpan,
        package: impl Into<String>,
        nickname: impl Into<String>,
    ) -> Self {
        Self {
            path,
            span,
            package: package.into(),
            nickname: nickname.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct UnusedNicknameItem {
    pub path: PathBuf,
    pub span: ByteSpan,
    pub package: String,
    pub nickname: String,
}

#[derive(Debug)]
pub struct UnusedNicknameSummary {
    pub declared_count: usize,
    pub unused: Vec<UnusedNicknameItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct UnusedNicknamePolicyOptions {
    fail_on_unused: bool,
}

impl UnusedNicknamePolicyOptions {
    #[must_use]
    pub const fn new(fail_on_unused: bool) -> Self {
        Self { fail_on_unused }
    }

    #[must_use]
    pub const fn fail_on_unused(self) -> bool {
        self.fail_on_unused
    }
}

#[derive(Debug)]
pub struct UnusedNicknamePolicy {
    pub fail_on_unused: bool,
    pub declared_count: usize,
    pub unused_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Collects every `(:nicknames ...)` entry declared by a `defpackage` form
/// in one file, paired with that package's own (normalized) primary name.
pub fn collect_declared_nicknames(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<Vec<DeclaredNickname>> {
    if dialect != Dialect::CommonLisp {
        return Ok(Vec::new());
    }

    let report = build_package_report(tree, dialect)?;
    Ok(report
        .defpackages
        .into_iter()
        .flat_map(|defpackage| {
            let path = path.to_path_buf();
            let package = normalize_common_lisp_package_designator(&defpackage.name).to_owned();
            let span = defpackage.span;
            defpackage.nicknames.into_iter().map(move |nickname| {
                DeclaredNickname::new(
                    path.clone(),
                    span,
                    package.clone(),
                    normalize_common_lisp_package_designator(&nickname),
                )
            })
        })
        .collect())
}

#[must_use]
pub fn analyze_unused_nicknames(
    declared: &[DeclaredNickname],
    referenced: &[String],
) -> UnusedNicknameSummary {
    let referenced_needles: BTreeSet<String> = referenced
        .iter()
        .map(|name| common_lisp_symbol_reference_needle(name))
        .collect();

    let unused = declared
        .iter()
        .filter(|declared| {
            !referenced_needles.contains(&common_lisp_symbol_reference_needle(&declared.nickname))
        })
        .map(|declared| UnusedNicknameItem {
            path: declared.path.clone(),
            span: declared.span,
            package: declared.package.clone(),
            nickname: declared.nickname.clone(),
        })
        .collect();

    UnusedNicknameSummary {
        declared_count: declared.len(),
        unused,
    }
}

#[must_use]
pub fn evaluate_unused_nickname_policy(
    options: UnusedNicknamePolicyOptions,
    summary: &UnusedNicknameSummary,
) -> UnusedNicknamePolicy {
    let unused_count = summary.unused.len();
    let mut violations = Vec::new();
    if options.fail_on_unused() && unused_count > 0 {
        violations.push(format!("unused_count {unused_count} exceeds 0"));
    }

    UnusedNicknamePolicy {
        fail_on_unused: options.fail_on_unused(),
        declared_count: summary.declared_count,
        unused_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declared(path: &str, input: &str) -> Vec<DeclaredNickname> {
        let tree = SyntaxTree::parse(input).expect("parse input");
        collect_declared_nicknames(&PathBuf::from(path), Dialect::CommonLisp, &tree)
            .expect("collect declared nicknames")
    }

    fn referenced(input: &str) -> Vec<String> {
        let tree = SyntaxTree::parse(input).expect("parse input");
        collect_referenced_package_names(Dialect::CommonLisp, &tree)
            .expect("collect referenced package names")
    }

    #[test]
    fn flags_a_nickname_never_used_as_a_qualifier() {
        let declared_nicknames = declared("lib.lisp", "(defpackage :lib (:nicknames :l))");
        let summary = analyze_unused_nicknames(&declared_nicknames, &[]);

        assert_eq!(summary.declared_count, 1);
        assert_eq!(summary.unused.len(), 1);
        assert_eq!(summary.unused[0].nickname, "l");
        assert_eq!(summary.unused[0].package, "lib");
    }

    #[test]
    fn does_not_flag_a_nickname_used_in_a_qualified_symbol() {
        let declared_nicknames = declared("lib.lisp", "(defpackage :lib (:nicknames :l))");
        let referenced_names = referenced("(defun f () (l:public-api))");

        let summary = analyze_unused_nicknames(&declared_nicknames, &referenced_names);
        assert!(summary.unused.is_empty());
    }

    #[test]
    fn does_not_flag_a_nickname_used_in_a_use_clause() {
        let declared_nicknames = declared("lib.lisp", "(defpackage :lib (:nicknames :l))");
        let referenced_names = referenced("(defpackage :app (:use :l))");

        let summary = analyze_unused_nicknames(&declared_nicknames, &referenced_names);
        assert!(summary.unused.is_empty());
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse("(ns app)").expect("parse input");
        let declared_nicknames =
            collect_declared_nicknames(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect declared nicknames");
        assert!(declared_nicknames.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let declared_nicknames = declared("lib.lisp", "(defpackage :lib (:nicknames :l))");
        let summary = analyze_unused_nicknames(&declared_nicknames, &[]);

        let quiet =
            evaluate_unused_nickname_policy(UnusedNicknamePolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.unused_count, 1);

        let strict =
            evaluate_unused_nickname_policy(UnusedNicknamePolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
