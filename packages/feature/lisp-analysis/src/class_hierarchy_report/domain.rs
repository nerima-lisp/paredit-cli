//! The CLOS inheritance tree, and which slots each class inherits from where.
//!
//! `inspect class-cycles` already reports the pathological case. This is the
//! ordinary one: what the hierarchy *is*, which is a question every CLOS
//! refactor starts with and no existing report answers.
//!
//! Slot provenance is the half that is genuinely hard to read off the source. A
//! class's effective slots are its own plus every superclass's, and a slot
//! redefined in a subclass *shadows* rather than adds — the subclass's
//! `:initform` wins, its `:initarg`s union with the parent's, and its
//! `:allocation` replaces. A `push-down-slot` or `pull-up-slot` edit needs to
//! know which of those it is about to change, and reading it off the file by
//! eye means holding the whole chain in your head.
//!
//! Only same-file superclasses are resolved. A class whose parent is defined
//! elsewhere is reported with the parent named and its inherited slots marked
//! unresolved, rather than silently reported as having none.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use paredit_core_syntax::common_lisp::common_lisp_operator_head_eq;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionKind, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::list_head;
use serde_json::{Value, json};

use paredit_core_cli::report::{FileFindings, Finding};

/// One class, with its place in the hierarchy and where its slots come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassFinding {
    pub name: String,
    /// The defining head: `defclass`, `define-condition`, or `defstruct`.
    pub head: String,
    /// Direct superclasses, in the order written — which is the order that
    /// decides the class precedence list, so it must not be sorted.
    pub superclasses: Vec<String>,
    /// Superclasses no analyzed file defines. Their slots cannot be attributed.
    pub unresolved_superclasses: Vec<String>,
    /// Slots this class declares itself.
    pub own_slots: Vec<String>,
    /// Slots reached through superclasses, as `slot@class`.
    pub inherited_slots: Vec<String>,
    /// Slots this class redeclares that a superclass also declares. The
    /// actionable field: a shadowed slot silently replaces the parent's
    /// `:initform`.
    pub shadowed_slots: Vec<String>,
    /// How many edges from a root. `0` for a class with no known superclass.
    pub depth: usize,
    pub span: ByteSpan,
}

impl Finding for ClassFinding {
    fn kind(&self) -> &'static str {
        if self.shadowed_slots.is_empty() {
            "class"
        } else {
            "shadowing-class"
        }
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            self.name.clone(),
            format!("depth={}", self.depth),
            format!("supers=({})", self.superclasses.join(" ")),
            format!("own=({})", self.own_slots.join(" ")),
            format!("inherited=({})", self.inherited_slots.join(" ")),
            format!("shadows=({})", self.shadowed_slots.join(" ")),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("name", json!(self.name)),
            ("head", json!(self.head)),
            ("superclasses", json!(self.superclasses)),
            (
                "unresolved_superclasses",
                json!(self.unresolved_superclasses),
            ),
            ("own_slots", json!(self.own_slots)),
            ("inherited_slots", json!(self.inherited_slots)),
            ("shadowed_slots", json!(self.shadowed_slots)),
            ("depth", json!(self.depth)),
        ]
    }
}

/// One class as read off its defining form, before the hierarchy is resolved.
struct RawClass {
    name: String,
    head: String,
    superclasses: Vec<String>,
    slots: Vec<String>,
    span: ByteSpan,
}

#[must_use]
pub fn build_class_hierarchy_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> FileFindings<ClassFinding> {
    let modelled = dialect == Dialect::CommonLisp;

    let mut raw = Vec::new();
    if modelled {
        for form in &tree.root_view().children {
            if let Some(class) = read_class(form) {
                raw.push(class);
            }
        }
    }

    let by_name: BTreeMap<String, usize> = raw
        .iter()
        .enumerate()
        .map(|(index, class)| (class.name.clone(), index))
        .collect();

    let findings = raw
        .iter()
        .map(|class| {
            let (inherited, unresolved, depth) = resolve(class, &raw, &by_name);
            let own: BTreeSet<&String> = class.slots.iter().collect();
            let shadowed = inherited
                .iter()
                .filter_map(|entry| entry.split('@').next())
                .filter(|slot| own.iter().any(|declared| declared.as_str() == *slot))
                .map(ToOwned::to_owned)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();

            ClassFinding {
                name: class.name.clone(),
                head: class.head.clone(),
                superclasses: class.superclasses.clone(),
                unresolved_superclasses: unresolved,
                own_slots: class.slots.clone(),
                inherited_slots: inherited,
                shadowed_slots: shadowed,
                depth,
                span: class.span,
            }
        })
        .collect::<Vec<_>>();

    let roots = findings.iter().filter(|class| class.depth == 0).count();
    let shadowing = findings
        .iter()
        .filter(|class| !class.shadowed_slots.is_empty())
        .count();

    FileFindings::new(
        path.to_path_buf(),
        dialect,
        modelled,
        tree.source(),
        findings,
        vec![
            ("class_count", json!(raw.len())),
            ("root_count", json!(roots)),
            ("shadowing_class_count", json!(shadowing)),
        ],
    )
}

/// Walks a class's superclasses, collecting inherited slots and depth.
///
/// The `seen` set is not defensive programming: `class-cycles` reports
/// inheritance cycles as a finding rather than rejecting the file, so this
/// report is expected to run on input that has one, and must terminate on it.
fn resolve(
    class: &RawClass,
    all: &[RawClass],
    by_name: &BTreeMap<String, usize>,
) -> (Vec<String>, Vec<String>, usize) {
    let mut inherited = BTreeSet::new();
    let mut unresolved = BTreeSet::new();
    let mut seen = BTreeSet::from([class.name.clone()]);
    let mut frontier = vec![(class.superclasses.clone(), 1usize)];
    let mut depth = 0;

    while let Some((supers, distance)) = frontier.pop() {
        for name in supers {
            let Some(index) = by_name.get(&name) else {
                unresolved.insert(name);
                depth = depth.max(distance);
                continue;
            };
            if !seen.insert(name.clone()) {
                continue;
            }
            depth = depth.max(distance);
            let parent = &all[*index];
            for slot in &parent.slots {
                inherited.insert(format!("{slot}@{}", parent.name));
            }
            frontier.push((parent.superclasses.clone(), distance + 1));
        }
    }

    (
        inherited.into_iter().collect(),
        unresolved.into_iter().collect(),
        depth,
    )
}

/// Reads a class-defining form, or `None`.
fn read_class(form: &ExpressionView) -> Option<RawClass> {
    let head = list_head(form)?;
    let is_class = ["defclass", "define-condition"]
        .iter()
        .any(|name| common_lisp_operator_head_eq(head, name));
    let is_struct = common_lisp_operator_head_eq(head, "defstruct");
    if !is_class && !is_struct {
        return None;
    }

    if is_struct {
        return read_struct(form, head);
    }

    let name = fold(atom_symbol_text(form.children.get(1)?)?);
    let superclasses = form
        .children
        .get(2)?
        .children
        .iter()
        .filter_map(atom_symbol_text)
        .map(fold)
        .collect();
    let slots = form
        .children
        .get(3)
        .map(|list| list.children.iter().filter_map(slot_name).collect())
        .unwrap_or_default();

    Some(RawClass {
        name,
        head: head.to_owned(),
        superclasses,
        slots,
        span: form.span,
    })
}

/// `defstruct` spells inheritance as an `(:include parent)` option rather than
/// as a superclass list, and its name may itself be `(name option…)`.
fn read_struct(form: &ExpressionView, head: &str) -> Option<RawClass> {
    let name_form = form.children.get(1)?;
    let (name, options): (String, &[ExpressionView]) = match name_form.kind {
        ExpressionKind::Atom => (fold(atom_symbol_text(name_form)?), &[]),
        ExpressionKind::List => (
            fold(atom_symbol_text(name_form.children.first()?)?),
            &name_form.children[1..],
        ),
        ExpressionKind::Root => return None,
    };

    let superclasses = options
        .iter()
        .filter(|option| {
            option
                .children
                .first()
                .and_then(atom_symbol_text)
                .is_some_and(|head| head.eq_ignore_ascii_case(":include"))
        })
        .filter_map(|option| option.children.get(1).and_then(atom_symbol_text))
        .map(fold)
        .collect();

    // A `defstruct` slot may be a bare symbol or `(name default …)`, and the
    // docstring — a string in the first body position — is not a slot.
    let slots = form
        .children
        .iter()
        .skip(2)
        .filter(|child| !is_string_literal(child))
        .filter_map(slot_name)
        .collect();

    Some(RawClass {
        name,
        head: head.to_owned(),
        superclasses,
        slots,
        span: form.span,
    })
}

fn slot_name(slot: &ExpressionView) -> Option<String> {
    match slot.kind {
        ExpressionKind::Atom => atom_symbol_text(slot).map(fold),
        ExpressionKind::List => slot.children.first().and_then(atom_symbol_text).map(fold),
        ExpressionKind::Root => None,
    }
}

fn is_string_literal(view: &ExpressionView) -> bool {
    view.kind == ExpressionKind::Atom
        && view
            .text
            .as_deref()
            .is_some_and(|text| text.starts_with('"'))
}

fn fold(name: &str) -> String {
    name.to_ascii_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(source: &str) -> FileFindings<ClassFinding> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        build_class_hierarchy_report(Path::new("t.lisp"), Dialect::CommonLisp, &tree)
    }

    fn class<'a>(report: &'a FileFindings<ClassFinding>, name: &str) -> &'a ClassFinding {
        report
            .findings
            .iter()
            .find(|class| class.name.eq_ignore_ascii_case(name))
            .unwrap_or_else(|| panic!("{name} is reported: {report:?}"))
    }

    #[test]
    fn a_root_class_reports_its_own_slots_and_depth_zero() {
        let report = report("(defclass animal () ((name :initarg :name)))");
        let animal = class(&report, "animal");
        assert_eq!(animal.own_slots, vec!["NAME".to_owned()]);
        assert_eq!(animal.depth, 0);
        assert!(animal.inherited_slots.is_empty());
    }

    #[test]
    fn a_subclass_inherits_its_parents_slots_with_provenance() {
        let report = report("(defclass animal () ((name)))\n(defclass fish (animal) ((fins)))");
        let fish = class(&report, "fish");
        assert_eq!(fish.inherited_slots, vec!["NAME@ANIMAL".to_owned()]);
        assert_eq!(fish.depth, 1);
    }

    #[test]
    fn inheritance_is_transitive() {
        let report = report("(defclass a () ((x)))\n(defclass b (a) ())\n(defclass c (b) ())");
        assert_eq!(class(&report, "c").inherited_slots, vec!["X@A".to_owned()]);
        assert_eq!(class(&report, "c").depth, 2);
    }

    #[test]
    fn a_redeclared_slot_is_reported_as_shadowing() {
        let report =
            report("(defclass a () ((x :initform 1)))\n(defclass b (a) ((x :initform 2)))");
        assert_eq!(class(&report, "b").shadowed_slots, vec!["X".to_owned()]);
        assert_eq!(class(&report, "b").kind(), "shadowing-class");
    }

    #[test]
    fn a_superclass_defined_elsewhere_is_named_rather_than_ignored() {
        let report = report("(defclass fish (some-library:animal) ((fins)))");
        let fish = class(&report, "fish");
        assert_eq!(fish.unresolved_superclasses.len(), 1);
        assert!(fish.inherited_slots.is_empty());
    }

    #[test]
    fn a_defstruct_include_is_read_as_inheritance() {
        let report = report("(defstruct animal name)\n(defstruct (fish (:include animal)) fins)");
        let fish = class(&report, "fish");
        assert_eq!(fish.superclasses, vec!["ANIMAL".to_owned()]);
        assert_eq!(fish.inherited_slots, vec!["NAME@ANIMAL".to_owned()]);
    }

    #[test]
    fn a_defstruct_docstring_is_not_read_as_a_slot() {
        let report = report("(defstruct animal \"A beast.\" name)");
        assert_eq!(class(&report, "animal").own_slots, vec!["NAME".to_owned()]);
    }

    #[test]
    fn a_define_condition_is_part_of_the_hierarchy() {
        let report = report("(define-condition my-error (error) ((code)))");
        assert_eq!(
            class(&report, "my-error").superclasses,
            vec!["ERROR".to_owned()]
        );
    }

    #[test]
    fn an_inheritance_cycle_terminates_instead_of_looping() {
        // `class-cycles` reports this rather than rejecting it, so this report
        // is expected to run on it.
        let report = report("(defclass a (b) ((x)))\n(defclass b (a) ((y)))");
        assert_eq!(report.findings.len(), 2);
    }

    #[test]
    fn an_unmodelled_dialect_says_so_rather_than_reporting_nothing() {
        let tree =
            SyntaxTree::parse_with_dialect("(defrecord A [x])", Dialect::Clojure).expect("parse");
        let report = build_class_hierarchy_report(Path::new("t.clj"), Dialect::Clojure, &tree);
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
    }
}
