//! Assembling every file's top-level definitions into one project table.

use crate::semantics::value::ValueTable;
use crate::semantics::value::service::constant_key;
use paredit_core_syntax::common_lisp::common_lisp_operator_head_eq;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ExpressionKind, ExpressionView, SymbolName, SyntaxTree};

use super::super::model::{
    GlobalDefinition, GlobalKind, GlobalTable, GlobalTableBuilder, QualifiedSymbol,
};
use super::package_resolver::{FilePackages, atom_text, resolve_symbol};

/// One analysed file's contribution to the project table.
///
/// The value table is per-file on purpose: a `defconstant`'s value is only
/// provable from the file that writes it, and whether it survives to the
/// project table is then a question of how many files define the same symbol.
#[derive(Debug, Clone, Copy)]
pub struct ProjectFile<'a> {
    pub tree: &'a SyntaxTree,
    pub packages: &'a FilePackages,
    pub values: &'a ValueTable,
}

impl<'a> ProjectFile<'a> {
    #[must_use]
    pub const fn new(
        tree: &'a SyntaxTree,
        packages: &'a FilePackages,
        values: &'a ValueTable,
    ) -> Self {
        Self {
            tree,
            packages,
            values,
        }
    }
}

/// Builds the project table from `files`, which must already be in analysis
/// order — each file's index becomes the `file` a definition records.
///
/// Only Common Lisp is analysed; every other dialect gets an empty table
/// rather than one built on guessed definition forms.
#[must_use]
pub fn build_global_table(dialect: Dialect, files: &[ProjectFile<'_>]) -> GlobalTable {
    let mut builder = GlobalTableBuilder::new();
    if dialect != Dialect::CommonLisp {
        return builder.finish();
    }

    for (index, file) in files.iter().enumerate() {
        for form in &file.tree.root_view().children {
            collect_definition(&mut builder, *file, index, form);
        }
    }

    builder.finish()
}

fn collect_definition(
    builder: &mut GlobalTableBuilder,
    file: ProjectFile<'_>,
    index: usize,
    form: &ExpressionView,
) {
    let Some((kind, name)) = definition_head(form).zip(definition_name(form)) else {
        return;
    };
    let Some(symbol) = resolve_symbol(file.packages, name.text, name.offset) else {
        // An unqualified name in a file with no `in-package` has no project
        // identity, so recording it would mean inventing one.
        return;
    };

    builder.define(GlobalDefinition::new(symbol.clone(), kind, index));

    if kind == GlobalKind::Constant {
        record_constant(builder, file, symbol, name.text);
    }
}

/// Records a `defconstant`'s value when the defining file proves it constant.
///
/// `defvar`/`defparameter` never reach here: both can be rebound at run time,
/// so their definitions are recorded while their values are not.
fn record_constant(
    builder: &mut GlobalTableBuilder,
    file: ProjectFile<'_>,
    symbol: QualifiedSymbol,
    text: &str,
) {
    // The value table keys constants by the folded name, the way the reader
    // reads a symbol. Looking one up by the spelling at the definition site
    // would miss a file that writes `+LIMIT+` where the table holds `+limit+`
    // folded — and, worse, would only miss it sometimes.
    let Some(name) = constant_key(text) else {
        return;
    };
    if let Some(value) = file.values.constant_value(&name) {
        builder.define_constant(symbol, value.clone());
    }
}

/// The kind of global a top-level form defines.
///
/// Deliberately narrower than [`paredit_core_syntax::definition::definition_shape`],
/// whose `Variable` category also covers `defglobal` and
/// `define-symbol-macro` — neither of which is a variable this table can make
/// a claim about.
fn definition_head(form: &ExpressionView) -> Option<GlobalKind> {
    (form.kind == ExpressionKind::List && form.reader_prefixes.is_empty()).then_some(())?;
    let head = form
        .children
        .first()
        .filter(|head| head.reader_prefixes.is_empty())?;
    let head = atom_text(head)?;

    [
        ("defun", GlobalKind::Function),
        ("defmacro", GlobalKind::Macro),
        ("defvar", GlobalKind::Variable),
        ("defparameter", GlobalKind::Variable),
        ("defconstant", GlobalKind::Constant),
    ]
    .into_iter()
    .find_map(|(candidate, kind)| common_lisp_operator_head_eq(head, candidate).then_some(kind))
}

/// The defined name and where it starts, which is what selects the
/// `in-package` region it is read in.
struct DefinitionName<'a> {
    text: &'a str,
    offset: usize,
}

fn definition_name(form: &ExpressionView) -> Option<DefinitionName<'_>> {
    // A `(setf foo)` function name is a list, not a symbol, and has no single
    // `QualifiedSymbol` to be; it is skipped rather than approximated.
    let name = form.children.get(1)?;
    Some(DefinitionName {
        text: atom_text(name)?,
        offset: name.span.start().get(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantics::project::model::PackageId;
    use crate::semantics::project::service::resolve_file_packages;

    struct File {
        tree: SyntaxTree,
        packages: FilePackages,
        values: ValueTable,
    }

    fn file(input: &str) -> File {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let packages = resolve_file_packages(Dialect::CommonLisp, &tree);
        File {
            tree,
            packages,
            values: ValueTable::default(),
        }
    }

    fn table(files: &[File]) -> GlobalTable {
        let project: Vec<ProjectFile<'_>> = files
            .iter()
            .map(|file| ProjectFile::new(&file.tree, &file.packages, &file.values))
            .collect();
        build_global_table(Dialect::CommonLisp, &project)
    }

    fn symbol(package: &str, name: &str) -> QualifiedSymbol {
        QualifiedSymbol::new(
            PackageId::new(package),
            SymbolName::new(name).expect("symbol"),
        )
    }

    #[test]
    fn each_definition_form_is_recorded_under_its_own_kind() {
        let files = [file(
            "(in-package :app)\
             (defun run () 1)\
             (defmacro with-it (&body body) body)\
             (defvar *state* nil)\
             (defconstant +limit+ 10)",
        )];
        let table = table(&files);
        for (name, kind) in [
            ("RUN", GlobalKind::Function),
            ("WITH-IT", GlobalKind::Macro),
            ("*STATE*", GlobalKind::Variable),
            ("+LIMIT+", GlobalKind::Constant),
        ] {
            let definitions = table.definitions(&symbol("app", name));
            assert_eq!(definitions.len(), 1, "{name}");
            assert_eq!(definitions[0].kind(), kind, "{name}");
        }
    }

    #[test]
    fn the_same_name_in_two_packages_stays_two_definitions() {
        // The impact-report false positive this layer exists to remove: a
        // name-only comparison would call these one definition.
        let files = [
            file("(in-package :app)(defun run () 1)"),
            file("(in-package :test)(defun run () 2)"),
        ];
        let table = table(&files);
        assert_eq!(table.definitions(&symbol("app", "RUN")).len(), 1);
        assert_eq!(table.definitions(&symbol("test", "RUN")).len(), 1);
        assert_eq!(table.definition_count(), 2);
    }

    #[test]
    fn one_name_defined_in_two_files_is_recorded_as_ambiguous() {
        let files = [
            file("(in-package :app)(defun run () 1)"),
            file("(in-package :app)(defun run () 2)"),
        ];
        let table = table(&files);
        let definitions = table.definitions(&symbol("app", "RUN"));
        assert_eq!(definitions.len(), 2);
        assert_eq!(definitions[0].file(), 0);
        assert_eq!(definitions[1].file(), 1);
    }

    #[test]
    fn a_file_with_no_in_package_contributes_nothing() {
        // Without a package the definition has no identity, and inventing one
        // would collide with a real definition elsewhere.
        let files = [file("(defun run () 1)")];
        assert_eq!(table(&files).definition_count(), 0);
    }

    #[test]
    fn a_non_common_lisp_project_has_no_definitions() {
        let tree =
            SyntaxTree::parse_with_dialect("(defn run [] 1)", Dialect::Clojure).expect("parse");
        let packages = FilePackages::default();
        let values = ValueTable::default();
        let project = [ProjectFile::new(&tree, &packages, &values)];
        assert_eq!(
            build_global_table(Dialect::Clojure, &project).definition_count(),
            0
        );
    }
}
