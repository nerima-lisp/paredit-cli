use paredit_core_syntax::clojure::ClojureOperator;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{Delimiter, ExpressionKind, ExpressionView, Path};

use crate::dependency_report::domain::syntax::{atom_text, list_head};
use crate::dependency_report::domain::types::{DependencyKind, DependencyReportItem};

/// Collects the dependency edges declared by a Clojure `ns` form.
///
/// A Clojure `ns` form carries the whole of what Common Lisp splits between
/// `defpackage` and a run of `require` calls, so it is the only place most
/// Clojure files state their dependencies at all. The clause grammar this
/// walks is:
///
/// ```clojure
/// (ns my.app
///   "docstring"                       ; skipped
///   {:author "someone"}               ; skipped
///   (:require lib
///             [lib :as alias]
///             [lib :refer [a b]]
///             [prefix [sub :as s] other])
///   (:import java.util.Date
///            (java.io File InputStream))
///   (:gen-class))                     ; no dependency
/// ```
pub fn collect_ns_dependency_items(
    view: &ExpressionView,
    dialect: Dialect,
    path: &Path,
    dependencies: &mut Vec<DependencyReportItem>,
) {
    if dialect != Dialect::Clojure {
        return;
    }
    let Some(head) = list_head(view) else {
        return;
    };
    if !ClojureOperator::from_head(head).is_some_and(ClojureOperator::is_namespace_declaration) {
        return;
    }

    let declaring_namespace = view
        .children
        .get(1)
        .and_then(atom_text)
        .map(ToOwned::to_owned);

    for (clause_index, clause) in view.children.iter().enumerate().skip(2) {
        // A docstring or attribute map sits between the name and the clauses;
        // neither is a paren list, so both fall out here.
        if clause.kind != ExpressionKind::List || clause.delimiter != Some(Delimiter::Paren) {
            continue;
        }
        let Some(kind) = clause
            .children
            .first()
            .and_then(atom_text)
            .and_then(clause_kind)
        else {
            continue;
        };
        let clause_path = path.child(clause_index);

        for (spec_index, spec) in clause.children.iter().enumerate().skip(1) {
            let spec_path = clause_path.child(spec_index);
            match kind {
                DependencyKind::NsImport => {
                    collect_import_spec(spec, &spec_path, &declaring_namespace, dependencies);
                }
                _ => collect_lib_spec(spec, kind, &spec_path, &declaring_namespace, dependencies),
            }
        }
    }
}

/// Maps an `ns` clause keyword onto the dependency kind it declares.
///
/// `:refer-clojure` and `:gen-class` are deliberately absent: neither names a
/// namespace this file depends on. `:refer-clojure` only filters the implicit
/// `clojure.core` referral, and `:gen-class` emits a Java class.
fn clause_kind(keyword: &str) -> Option<DependencyKind> {
    Some(match keyword {
        ":require" => DependencyKind::NsRequire,
        ":require-macros" => DependencyKind::NsRequireMacros,
        ":use" | ":use-macros" => DependencyKind::NsUse,
        ":import" => DependencyKind::NsImport,
        ":load" => DependencyKind::NsLoad,
        _ => return None,
    })
}

/// Collects one `:require`/`:use` libspec, which is either a bare namespace
/// symbol, a `[namespace :as alias ...]` vector, or a prefix list such as
/// `[clojure [string :as s] set]` that expands to `clojure.string` and
/// `clojure.set`.
fn collect_lib_spec(
    spec: &ExpressionView,
    kind: DependencyKind,
    path: &Path,
    source: &Option<String>,
    dependencies: &mut Vec<DependencyReportItem>,
) {
    if let Some(name) = atom_text(spec) {
        push(kind, name, path, spec, source, dependencies);
        return;
    }
    if !is_vector(spec) {
        return;
    }
    let Some(first) = spec.children.first().and_then(atom_text) else {
        return;
    };

    // In a plain libspec the child after the namespace is always an option
    // keyword (`:as`, `:refer`, `:only`, ...). Anything else means this is a
    // prefix list whose remaining children are sub-libspecs.
    let is_prefix_list = spec
        .children
        .get(1)
        .is_some_and(|next| !atom_text(next).is_some_and(|text| text.starts_with(':')));

    if !is_prefix_list {
        push(kind, first, path, spec, source, dependencies);
        return;
    }

    for (index, sub) in spec.children.iter().enumerate().skip(1) {
        let sub_path = path.child(index);
        let sub_name = if let Some(name) = atom_text(sub) {
            Some(name)
        } else if is_vector(sub) {
            sub.children.first().and_then(atom_text)
        } else {
            None
        };
        let Some(sub_name) = sub_name else {
            continue;
        };
        push(
            kind,
            &format!("{first}.{sub_name}"),
            &sub_path,
            sub,
            source,
            dependencies,
        );
    }
}

/// Collects one `:import` spec, which is either a fully qualified class name
/// or a `(package Class Class)` grouping that expands to one entry per class.
fn collect_import_spec(
    spec: &ExpressionView,
    path: &Path,
    source: &Option<String>,
    dependencies: &mut Vec<DependencyReportItem>,
) {
    if let Some(class) = atom_text(spec) {
        push(
            DependencyKind::NsImport,
            class,
            path,
            spec,
            source,
            dependencies,
        );
        return;
    }
    if spec.kind != ExpressionKind::List {
        return;
    }
    let Some(package) = spec.children.first().and_then(atom_text) else {
        return;
    };

    for (index, class) in spec.children.iter().enumerate().skip(1) {
        let Some(class_name) = atom_text(class) else {
            continue;
        };
        push(
            DependencyKind::NsImport,
            &format!("{package}.{class_name}"),
            &path.child(index),
            class,
            source,
            dependencies,
        );
    }
}

fn is_vector(view: &ExpressionView) -> bool {
    view.kind == ExpressionKind::List && view.delimiter == Some(Delimiter::Bracket)
}

fn push(
    kind: DependencyKind,
    target: &str,
    path: &Path,
    view: &ExpressionView,
    source: &Option<String>,
    dependencies: &mut Vec<DependencyReportItem>,
) {
    if target.is_empty() {
        return;
    }
    dependencies.push(DependencyReportItem::new(
        kind,
        target,
        path.to_string(),
        view.span,
        source.clone(),
    ));
}
