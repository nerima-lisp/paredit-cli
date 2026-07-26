//! Which package each form in a file belongs to.

use crate::domain::common_lisp::{
    common_lisp_operator_head_eq, common_lisp_symbol_reference_needle,
    has_common_lisp_package_qualifier, normalize_common_lisp_package_designator,
};
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, ExpressionKind, ExpressionView, SymbolName, SyntaxTree};

use super::super::model::{PackageId, QualifiedSymbol};

/// The package in effect over one span of a file.
///
/// A file can switch packages part-way with a second `in-package`, so this is
/// a sequence of regions rather than a single answer.
#[derive(Debug, Clone)]
pub struct PackageRegion {
    package: PackageId,
    span: ByteSpan,
}

impl PackageRegion {
    pub const fn package(&self) -> &PackageId {
        &self.package
    }

    pub const fn span(&self) -> ByteSpan {
        self.span
    }
}

/// The package regions of one file, in source order.
#[derive(Debug, Clone, Default)]
pub struct FilePackages {
    regions: Vec<PackageRegion>,
}

impl FilePackages {
    /// The package in effect at `offset`, or `None` before the file's first
    /// `in-package` — where the reader's current package is whatever the
    /// build set up, which a static analysis cannot know.
    pub fn package_at(&self, offset: usize) -> Option<&PackageId> {
        self.regions
            .iter()
            .rev()
            .find(|region| region.span().start().get() <= offset)
            .map(PackageRegion::package)
    }

    pub fn region_count(&self) -> usize {
        self.regions.len()
    }

    pub fn regions(&self) -> &[PackageRegion] {
        &self.regions
    }
}

/// Resolves the `in-package` regions of one file.
///
/// A region runs from its `in-package` form to the next one, or to the end of
/// the file. Only Common Lisp has the form; every other dialect gets no
/// regions, which reads as "no package is known" rather than a guessed one.
pub fn resolve_file_packages(dialect: Dialect, tree: &SyntaxTree) -> FilePackages {
    if dialect != Dialect::CommonLisp {
        return FilePackages::default();
    }

    let document = tree.root_view();
    let switches: Vec<(PackageId, ByteSpan)> = document
        .children
        .iter()
        .filter_map(|form| Some((in_package_designator(form)?, form.span)))
        .collect();

    let end = document.span.end();
    let regions = switches
        .iter()
        .enumerate()
        .map(|(index, (package, span))| PackageRegion {
            package: package.clone(),
            span: ByteSpan::new(
                span.start(),
                switches
                    .get(index + 1)
                    .map_or(end, |(_, next)| next.start()),
            ),
        })
        .collect();

    FilePackages { regions }
}

/// Resolves one symbol occurrence to the symbol it names.
///
/// `offset` is where the occurrence starts, which is what selects the
/// `in-package` region an unqualified name is read in. `None` means the symbol
/// is unresolvable — an unqualified name in a file that never says which
/// package it is in — and is deliberately representable, because guessing
/// `CL-USER` would manufacture the cross-package confusion this layer exists
/// to remove.
pub fn resolve_symbol(
    packages: &FilePackages,
    text: &str,
    offset: usize,
) -> Option<QualifiedSymbol> {
    let (package, name) = if let Some(name) = keyword_name(text) {
        (PackageId::keyword(), name)
    } else if let Some((prefix, name)) = explicit_qualifier(text) {
        (package_id(prefix), name)
    } else {
        (packages.package_at(offset)?.clone(), text)
    };

    // The name is folded the way the reader folds it, so `app:RUN` and
    // `app:run` are one symbol while `app:|run|` stays its own.
    SymbolName::new(common_lisp_symbol_reference_needle(name))
        .ok()
        .map(|name| QualifiedSymbol::new(package, name))
}

/// The package a designator names, with the two standard nicknames folded in.
///
/// Without the fold `cl:list` and `common-lisp:list` would be two symbols.
/// `domain::common_lisp` already makes this identification in
/// `canonical_common_lisp_package_identity`, which is private to that module.
fn package_id(designator: &str) -> PackageId {
    let id = PackageId::new(normalize_common_lisp_package_designator(designator));
    match id.as_str() {
        "CL" => PackageId::common_lisp(),
        "CL-USER" => PackageId::new("COMMON-LISP-USER"),
        _ => id,
    }
}

/// The package an `(in-package DESIGNATOR)` form switches to.
fn in_package_designator(view: &ExpressionView) -> Option<PackageId> {
    (view.kind == ExpressionKind::List && view.reader_prefixes.is_empty()).then_some(())?;
    let head = head_text(view.children.first()?)?;
    common_lisp_operator_head_eq(head, "in-package").then_some(())?;
    // The `:app` / `#:app` / `"app"` spellings all reach `PackageId` stripped.
    Some(package_id(atom_text(view.children.get(1)?)?))
}

/// The name part of a `:foo` keyword.
///
/// A leading colon with nothing before it names a symbol in `KEYWORD`, not a
/// qualified reference to a `foo` living elsewhere.
fn keyword_name(symbol: &str) -> Option<&str> {
    let rest = symbol.strip_prefix(':')?;
    // `::foo` is not keyword syntax, and a bare `:` names nothing.
    (!rest.is_empty() && !rest.starts_with(':')).then_some(rest)
}

/// The package prefix and bare name of `pkg:sym` or `pkg::sym`.
///
/// Duplicates the private `common_lisp_explicit_package_and_name` in
/// `domain::common_lisp::operator::normalize`; exporting that and deleting
/// this would be a pure simplification.
fn explicit_qualifier(symbol: &str) -> Option<(&str, &str)> {
    // `#:foo` is an uninterned symbol whose `#:` is not a package marker, and
    // the shared predicate is what already knows that.
    has_common_lisp_package_qualifier(symbol).then_some(())?;

    let marker = last_package_marker(symbol)?;
    if marker == 0 || marker + 1 >= symbol.len() {
        return None;
    }
    // `pkg::sym` marks an internal symbol with two colons, so the package name
    // ends before the first of them.
    let package_end = if symbol.as_bytes().get(marker - 1) == Some(&b':') {
        marker - 1
    } else {
        marker
    };
    (package_end > 0).then_some((&symbol[..package_end], &symbol[marker + 1..]))
}

/// The last package marker, skipping colons the reader escapes.
fn last_package_marker(symbol: &str) -> Option<usize> {
    let mut escaped = false;
    let mut in_multiple_escape = false;
    let mut marker = None;

    for (index, character) in symbol.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match character {
            '\\' => escaped = true,
            '|' => in_multiple_escape = !in_multiple_escape,
            ':' if !in_multiple_escape => marker = Some(index),
            _ => {}
        }
    }

    marker
}

/// An atom's text, or `None` when the node is not a bare atom.
///
/// A prefixed atom is never an operator head: reading `#'(setf x)` as the head
/// `setf` would turn a function designator into a binding form.
fn head_text(view: &ExpressionView) -> Option<&str> {
    (view.reader_prefixes.is_empty())
        .then_some(())
        .and_then(|()| atom_text(view))
}

pub(super) fn atom_text(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom)
        .then_some(view.text.as_deref())
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packages(input: &str) -> FilePackages {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        resolve_file_packages(Dialect::CommonLisp, &tree)
    }

    fn qualified(input: &str, symbol: &str, offset: usize) -> Option<String> {
        resolve_symbol(&packages(input), symbol, offset).map(|resolved| resolved.to_string())
    }

    #[test]
    fn a_file_that_never_says_its_package_resolves_nothing() {
        // Guessing `CL-USER` here would manufacture exactly the cross-package
        // confusion this layer exists to remove.
        let input = "(defun run () 1)";
        assert_eq!(packages(input).region_count(), 0);
        assert_eq!(qualified(input, "run", 7), None);
    }

    #[test]
    fn every_designator_spelling_names_the_same_package() {
        for spelling in ["app", ":app", "#:app", "\"app\""] {
            let input = format!("(in-package {spelling})");
            let resolved = packages(&input);
            assert_eq!(
                resolved.package_at(input.len() - 1).map(PackageId::as_str),
                Some("APP"),
                "{spelling}"
            );
        }
    }

    #[test]
    fn a_second_in_package_switches_at_its_own_offset() {
        let input = "(in-package :app)\n(defun a () 1)\n(in-package :test)\n(defun b () 2)";
        let resolved = packages(input);
        assert_eq!(resolved.region_count(), 2);
        let boundary = input.find("(in-package :test)").expect("second switch");
        assert_eq!(
            resolved.package_at(boundary - 1).map(PackageId::as_str),
            Some("APP")
        );
        assert_eq!(
            resolved.package_at(boundary).map(PackageId::as_str),
            Some("TEST")
        );
    }

    #[test]
    fn an_explicit_qualifier_wins_over_the_current_package() {
        let input = "(in-package :app)";
        assert_eq!(
            qualified(input, "other:run", input.len()),
            Some("OTHER:RUN".to_owned())
        );
        assert_eq!(
            qualified(input, "other::run", input.len()),
            Some("OTHER:RUN".to_owned())
        );
    }

    #[test]
    fn a_keyword_lives_in_the_keyword_package() {
        let input = "(in-package :app)";
        assert_eq!(
            qualified(input, ":test", input.len()),
            Some("KEYWORD:TEST".to_owned())
        );
    }

    #[test]
    fn the_standard_nicknames_fold_onto_their_full_names() {
        let input = "(in-package :app)";
        assert_eq!(
            qualified(input, "cl:list", input.len()),
            qualified(input, "common-lisp:list", input.len())
        );
    }

    #[test]
    fn the_same_name_in_two_packages_stays_two_symbols() {
        // The motivating case: a name-only comparison calls these equal and
        // reports a change to one as affecting callers of the other.
        let input = "(in-package :app)";
        let left = resolve_symbol(&packages(input), "app:run", input.len()).expect("app:run");
        let right = resolve_symbol(&packages(input), "test:run", input.len()).expect("test:run");
        assert_ne!(left, right);
        assert_eq!(left.name(), right.name());
    }

    #[test]
    fn a_non_common_lisp_file_has_no_packages() {
        let tree =
            SyntaxTree::parse_with_dialect("(ns app.core)", Dialect::Clojure).expect("parse");
        assert_eq!(
            resolve_file_packages(Dialect::Clojure, &tree).region_count(),
            0
        );
    }
}
