//! Domain knowledge for Lisp-family definition forms.

mod classify;
mod lambda_list;
mod name;

use crate::dialect::Dialect;
use crate::sexpr::{ByteSpan, ExpressionView, Path};

use classify::classify_definition_head;
use lambda_list::{
    definition_body_start_child_index, definition_lambda_list_position,
    definition_lambda_parameter_arity, definition_lambda_parameter_count,
};
use name::{definition_name_target, definition_name_text};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DefinitionCategory {
    Function,
    Macro,
    GenericFunction,
    Method,
    Class,
    Struct,
    Condition,
    Variable,
    Constant,
    Parameter,
    Package,
    System,
    Test,
    Customization,
    Mode,
    Other,
    /// A `define-*`-prefixed macro not otherwise recognized by this tool.
    /// Unlike `Other` (a dialect's own known definition forms, such as
    /// Emacs Lisp `defun` or Clojure `defn`), this macro's expansion — and
    /// any symbol names it derives from its argument — is unknown.
    UnknownMacro,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DefinitionShape {
    pub category: DefinitionCategory,
    /// Kept so arity questions can be answered in the right language. A
    /// Common Lisp lambda list marks its optional tail with `&optional`; a
    /// Scheme one marks it with a dotted `. rest` and Racket brackets. Reading
    /// either with the other's rules produces a confident wrong answer.
    dialect: Dialect,
    name_child_index: Option<usize>,
    lambda_list_child_index: Option<usize>,
    /// The first child of the lambda list that is actually a parameter.
    ///
    /// `0` everywhere the lambda list is a list of nothing but parameters,
    /// which is every Common Lisp definition form. Scheme's `define` packs the
    /// name and the parameters into one node -- `(define (f x y) ...)` has
    /// lambda list `(f x y)` -- so there it is `1`. Reading that node's
    /// children directly counts the procedure's own name as a parameter.
    lambda_list_first_parameter_index: usize,
    body_start_child_index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionNameTarget<'a> {
    pub path: Path,
    pub span: ByteSpan,
    pub text: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DefinitionBodyRange {
    start_child_index: usize,
    end_child_index: Option<usize>,
}

impl DefinitionShape {
    pub fn name(self, view: &ExpressionView) -> Option<&str> {
        self.name_child_index
            .and_then(|index| view.children.get(index))
            .and_then(definition_name_text)
    }

    #[must_use]
    pub fn body_form_count(self, view: &ExpressionView) -> usize {
        view.children
            .len()
            .saturating_sub(self.body_start_child_index)
    }

    #[must_use]
    pub fn body_forms(self, view: &ExpressionView) -> &[ExpressionView] {
        let start = self.body_start_child_index.min(view.children.len());
        &view.children[start..]
    }

    #[must_use]
    pub fn lambda_list(self, view: &ExpressionView) -> Option<&ExpressionView> {
        self.lambda_list_child_index
            .and_then(|index| view.children.get(index))
    }

    /// The parameter nodes of the lambda list, with any leading name skipped.
    ///
    /// Prefer this to walking `lambda_list(view).children` directly: the two
    /// agree for every Common Lisp form and disagree for Scheme's `define`,
    /// where child 0 of the lambda list is the procedure's own name.
    #[must_use]
    pub fn lambda_parameters(self, view: &ExpressionView) -> Option<&[ExpressionView]> {
        let lambda_list = self.lambda_list(view)?;
        let start = self
            .lambda_list_first_parameter_index
            .min(lambda_list.children.len());
        Some(&lambda_list.children[start..])
    }

    /// Where the parameters begin inside the lambda list node.
    #[must_use]
    pub const fn lambda_list_first_parameter_index(self) -> usize {
        self.lambda_list_first_parameter_index
    }

    #[must_use]
    pub fn lambda_parameter_count(self, view: &ExpressionView) -> Option<usize> {
        self.lambda_parameters(view)
            .map(|parameters| definition_lambda_parameter_count(self.dialect, parameters))
    }

    /// Return the (minimum, maximum) call-argument arity this definition's
    /// lambda list accepts; MAXIMUM is `None` when unbounded. See
    /// DEFINITION_LAMBDA_PARAMETER_ARITY for why this differs from
    /// LAMBDA_PARAMETER_COUNT's flat total.
    #[must_use]
    pub fn lambda_parameter_arity(self, view: &ExpressionView) -> Option<(usize, Option<usize>)> {
        self.lambda_parameters(view)
            .map(|parameters| definition_lambda_parameter_arity(self.dialect, parameters))
    }

    #[must_use]
    pub fn name_target<'a>(
        self,
        view: &'a ExpressionView,
        parent_path: &Path,
    ) -> Option<DefinitionNameTarget<'a>> {
        let index = self.name_child_index?;
        let name = view.children.get(index)?;
        definition_name_target(name, &parent_path.child(index))
    }

    #[must_use]
    pub const fn body_range(self) -> DefinitionBodyRange {
        DefinitionBodyRange {
            start_child_index: self.body_start_child_index,
            end_child_index: None,
        }
    }

    /// Returns a body range bounded by the children present in `view`.
    #[must_use]
    pub fn body_range_in(self, view: &ExpressionView) -> DefinitionBodyRange {
        DefinitionBodyRange {
            start_child_index: self.body_start_child_index,
            end_child_index: Some(view.children.len()),
        }
    }
}

impl DefinitionBodyRange {
    #[must_use]
    pub fn contains_child(self, child_index: usize) -> bool {
        child_index >= self.start_child_index
            && self.end_child_index.is_none_or(|end| child_index < end)
    }

    #[must_use]
    pub fn child_path(self, parent_path: &Path, child_index: usize) -> Option<Path> {
        self.contains_child(child_index)
            .then(|| parent_path.child(child_index))
    }
}

impl DefinitionCategory {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Macro => "macro",
            Self::GenericFunction => "generic-function",
            Self::Method => "method",
            Self::Class => "class",
            Self::Struct => "struct",
            Self::Condition => "condition",
            Self::Variable => "variable",
            Self::Constant => "constant",
            Self::Parameter => "parameter",
            Self::Package => "package",
            Self::System => "system",
            Self::Test => "test",
            Self::Customization => "customization",
            Self::Mode => "mode",
            Self::Other => "other",
            Self::UnknownMacro => "unknown-macro",
        }
    }

    #[must_use]
    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "function" => Some(Self::Function),
            "macro" => Some(Self::Macro),
            "generic-function" => Some(Self::GenericFunction),
            "method" => Some(Self::Method),
            "class" => Some(Self::Class),
            "struct" => Some(Self::Struct),
            "condition" => Some(Self::Condition),
            "variable" => Some(Self::Variable),
            "constant" => Some(Self::Constant),
            "parameter" => Some(Self::Parameter),
            "package" => Some(Self::Package),
            "system" => Some(Self::System),
            "test" => Some(Self::Test),
            "customization" => Some(Self::Customization),
            "mode" => Some(Self::Mode),
            "other" => Some(Self::Other),
            "unknown-macro" => Some(Self::UnknownMacro),
            _ => None,
        }
    }

    #[must_use]
    pub const fn is_callable(self) -> bool {
        matches!(
            self,
            Self::Function | Self::Macro | Self::GenericFunction | Self::Method
        )
    }

    /// Whether a definition in this category is safe to bulk-remove based
    /// purely on "no direct symbol references elsewhere" evidence.
    ///
    /// `UnknownMacro` covers a `define-*`-prefixed macro this tool does not
    /// recognize, whose expansion is unknown. Such a macro commonly derives
    /// *other* symbol names from its argument via string concatenation (for
    /// example a strategy DSL where `(define-strategy foo ...)` generates and
    /// exports `make-foo-strategy`), so "is the argument symbol referenced
    /// elsewhere" is not a safe proxy for "is this definition unused": the
    /// argument symbol itself may legitimately have zero direct references
    /// while the code it defines is very much in use. This is distinct from
    /// `Other`, which covers a dialect's own recognized definition forms (for
    /// example Emacs Lisp `defun`/`defvar` or Clojure `defn`) that are not
    /// broken out into a more specific category but are still known,
    /// non-generative shapes.
    ///
    /// `Struct` (Common Lisp `defstruct`) has the same derived-symbol
    /// problem even though this tool DOES recognize the form: `defstruct`
    /// implicitly derives a constructor (`make-<name>` by default, or an
    /// explicit `(:constructor other-name)` option), a predicate
    /// (`<name>-p`), a copier, and per-slot accessors from the structure
    /// name, none of which textually contain the structure name symbol
    /// itself.
    ///
    /// `Test` (`deftest`/`ert-deftest`) and `Package` (`provide`/`require`)
    /// are entry points invoked by a test runner or the module loader rather
    /// than referenced by symbol from other code, so "zero direct
    /// references" is their normal, expected state and carries no signal
    /// about whether the code is actually dead. `Customization` and `Mode`
    /// definitions are conventionally discovered by a human through `M-x`
    /// or a customize buffer rather than called by name from other Lisp
    /// forms, so the same reasoning applies to them.
    #[must_use]
    pub const fn is_bulk_removable(self) -> bool {
        matches!(
            self,
            Self::Function
                | Self::Macro
                | Self::GenericFunction
                | Self::Method
                | Self::Class
                | Self::Condition
                | Self::Variable
                | Self::Constant
                | Self::Parameter
                | Self::Other
        )
    }
}

const fn definition_name_child_index(_head: &str) -> Option<usize> {
    Some(1)
}

#[must_use]
pub fn definition_shape(
    dialect: Dialect,
    view: &ExpressionView,
    head: &str,
) -> Option<DefinitionShape> {
    let category = classify_definition_head(dialect, head)?;
    let position = definition_lambda_list_position(dialect, view, head);
    let lambda_list_child_index = position.map(|(index, _)| index);
    let body_start_child_index =
        definition_body_start_child_index(view, head, Some(category), lambda_list_child_index);

    Some(DefinitionShape {
        category,
        dialect,
        name_child_index: definition_name_child_index(head),
        lambda_list_child_index,
        lambda_list_first_parameter_index: position.map_or(0, |(_, first)| first),
        body_start_child_index,
    })
}

/// Whether the definition body returns code to be evaluated by a Lisp macro expander.
#[must_use]
pub fn is_macro_expander_definition(dialect: Dialect, head: &str) -> bool {
    classify::is_macro_expander_definition(dialect, head)
}

/// Returns the child range containing code returned by a macro expander.
pub fn macro_expander_body_range(
    dialect: Dialect,
    view: &ExpressionView,
    head: &str,
) -> Option<DefinitionBodyRange> {
    if !is_macro_expander_definition(dialect, head) {
        return None;
    }

    definition_shape(dialect, view, head).map(DefinitionShape::body_range)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::sexpr::{Path, SyntaxTree};

    #[test]
    fn labels_round_trip_to_categories() {
        let categories = [
            DefinitionCategory::Function,
            DefinitionCategory::Macro,
            DefinitionCategory::GenericFunction,
            DefinitionCategory::Method,
            DefinitionCategory::Class,
            DefinitionCategory::Struct,
            DefinitionCategory::Condition,
            DefinitionCategory::Variable,
            DefinitionCategory::Constant,
            DefinitionCategory::Parameter,
            DefinitionCategory::Package,
            DefinitionCategory::System,
            DefinitionCategory::Test,
            DefinitionCategory::Customization,
            DefinitionCategory::Mode,
            DefinitionCategory::Other,
        ];

        for category in categories {
            assert_eq!(
                DefinitionCategory::from_label(category.label()),
                Some(category)
            );
        }
        assert_eq!(DefinitionCategory::from_label("unknown"), None);
    }

    #[test]
    fn classifies_common_lisp_and_emacs_definition_heads() {
        for (head, category) in [
            ("defun", DefinitionCategory::Function),
            ("DEFUN", DefinitionCategory::Function),
            ("CL:DEFMACRO", DefinitionCategory::Macro),
            ("cl:defgeneric", DefinitionCategory::GenericFunction),
            ("define-setf-expander", DefinitionCategory::Macro),
            ("define-symbol-macro", DefinitionCategory::Variable),
            ("asdf:defsystem", DefinitionCategory::System),
            ("deftest", DefinitionCategory::Test),
        ] {
            assert_eq!(
                classify_definition_head(Dialect::CommonLisp, head),
                Some(category),
                "Common Lisp head {head}"
            );
        }

        for (head, category) in [
            ("defun", DefinitionCategory::Function),
            ("defsubst", DefinitionCategory::Function),
            ("cl-defun", DefinitionCategory::Function),
            ("defmacro", DefinitionCategory::Macro),
            ("cl-defmacro", DefinitionCategory::Macro),
            ("cl-defgeneric", DefinitionCategory::GenericFunction),
            ("cl-defmethod", DefinitionCategory::Method),
            ("defvar", DefinitionCategory::Variable),
            ("defconst", DefinitionCategory::Constant),
            ("defcustom", DefinitionCategory::Customization),
            ("defgroup", DefinitionCategory::Customization),
            ("define-minor-mode", DefinitionCategory::Mode),
            ("define-derived-mode", DefinitionCategory::Mode),
            ("provide", DefinitionCategory::Package),
            ("require", DefinitionCategory::Package),
        ] {
            assert_eq!(
                classify_definition_head(Dialect::EmacsLisp, head),
                Some(category),
                "Emacs Lisp head {head}"
            );
        }
    }

    #[test]
    fn classifies_common_lisp_definition_extensions_without_accepting_arbitrary_heads() {
        for (head, expected) in [
            (
                "define-trading-strategy",
                Some(DefinitionCategory::UnknownMacro),
            ),
            (
                "CL:DEFINE-TRADING-STRATEGY",
                Some(DefinitionCategory::UnknownMacro),
            ),
            ("deftest", Some(DefinitionCategory::Test)),
            ("CL:DEFTEST", Some(DefinitionCategory::Test)),
            ("trading-strategy", None),
        ] {
            assert_eq!(
                classify_definition_head(Dialect::CommonLisp, head),
                expected,
                "Common Lisp extension head {head}"
            );
        }
    }

    #[test]
    fn identifies_macro_expander_definition_forms() {
        assert!(is_macro_expander_definition(
            Dialect::CommonLisp,
            "defmacro"
        ));
        assert!(is_macro_expander_definition(
            Dialect::CommonLisp,
            "cl:define-compiler-macro"
        ));
        assert!(is_macro_expander_definition(
            Dialect::CommonLisp,
            "cl:define-setf-expander"
        ));
        assert!(is_macro_expander_definition(Dialect::CommonLisp, "defsetf"));
        assert!(is_macro_expander_definition(
            Dialect::EmacsLisp,
            "cl-defmacro"
        ));
        assert!(!is_macro_expander_definition(Dialect::CommonLisp, "defun"));
        assert!(!is_macro_expander_definition(
            Dialect::Scheme,
            "define-syntax"
        ));
        assert!(is_macro_expander_definition(Dialect::Clojure, "defmacro"));
        assert!(!is_macro_expander_definition(Dialect::Clojure, "defn"));
        assert!(is_macro_expander_definition(Dialect::Janet, "defmacro"));
        assert!(!is_macro_expander_definition(Dialect::Janet, "defn"));
        assert!(is_macro_expander_definition(Dialect::Hy, "defmacro"));
        assert!(!is_macro_expander_definition(Dialect::Hy, "defn"));
        assert!(is_macro_expander_definition(Dialect::Carp, "defmacro"));
        assert!(!is_macro_expander_definition(Dialect::Carp, "defn"));
        assert!(is_macro_expander_definition(Dialect::Fennel, "macro"));
        assert!(!is_macro_expander_definition(Dialect::Fennel, "fn"));
        assert!(is_macro_expander_definition(Dialect::Lfe, "defmacro"));
        assert!(is_macro_expander_definition(Dialect::Lfe, "defsyntax"));
        assert!(!is_macro_expander_definition(Dialect::Lfe, "defun"));
        assert!(!is_macro_expander_definition(Dialect::Racket, "defmacro"));
    }

    #[test]
    fn identifies_macro_expander_body_range() {
        let tree =
            SyntaxTree::parse("(defmacro render (node) (declare (ignore node)) `(fetch node))")
                .expect("source parses");
        let view = tree
            .select_path(&Path::from_indexes(vec![0]))
            .expect("macro form exists")
            .view();

        let range = macro_expander_body_range(Dialect::CommonLisp, &view, "defmacro")
            .expect("defmacro has a body range");

        assert!(!range.contains_child(2));
        assert!(range.contains_child(3));
        assert!(range.contains_child(4));
    }

    #[test]
    fn identifies_define_setf_expander_body_range() {
        let tree = SyntaxTree::parse(
            "(define-setf-expander slot (place) (values nil nil nil `(writer store) `(reader ,place)))",
        )
        .expect("source parses");
        let view = tree
            .select_path(&Path::from_indexes(vec![0]))
            .expect("setf expander form exists")
            .view();

        let range = macro_expander_body_range(Dialect::CommonLisp, &view, "define-setf-expander")
            .expect("define-setf-expander has a body range");

        assert!(!range.contains_child(2));
        assert!(range.contains_child(3));
    }

    #[test]
    fn identifies_long_defsetf_body_range() {
        let tree = SyntaxTree::parse("(defsetf slot (place) (store) `(writer ,place ,store))")
            .expect("source parses");
        let view = tree
            .select_path(&Path::from_indexes(vec![0]))
            .expect("defsetf form exists")
            .view();

        let range = macro_expander_body_range(Dialect::CommonLisp, &view, "defsetf")
            .expect("long defsetf has a body range");

        assert!(!range.contains_child(3));
        assert!(range.contains_child(4));
    }

    #[test]
    fn classifies_scheme_clojure_janet_and_fennel_definition_heads() {
        for (dialect, head, category) in [
            (Dialect::Scheme, "define", DefinitionCategory::Function),
            (Dialect::Scheme, "define-syntax", DefinitionCategory::Macro),
            (
                Dialect::Scheme,
                "define-library",
                DefinitionCategory::Package,
            ),
            (Dialect::Scheme, "lambda", DefinitionCategory::Function),
            (Dialect::Scheme, "let", DefinitionCategory::Other),
            (Dialect::Scheme, "let*", DefinitionCategory::Other),
            (Dialect::Clojure, "ns", DefinitionCategory::Package),
            (Dialect::Clojure, "def", DefinitionCategory::Variable),
            (Dialect::Clojure, "defn", DefinitionCategory::Function),
            (Dialect::Clojure, "defmacro", DefinitionCategory::Macro),
            (Dialect::Clojure, "defrecord", DefinitionCategory::Struct),
            (Dialect::Clojure, "deftype", DefinitionCategory::Class),
            (Dialect::Clojure, "defprotocol", DefinitionCategory::Class),
            (
                Dialect::Clojure,
                "defmulti",
                DefinitionCategory::GenericFunction,
            ),
            (Dialect::Clojure, "defmethod", DefinitionCategory::Method),
            (Dialect::Janet, "def", DefinitionCategory::Variable),
            (Dialect::Janet, "def-", DefinitionCategory::Variable),
            (Dialect::Janet, "defn", DefinitionCategory::Function),
            (Dialect::Janet, "defn-", DefinitionCategory::Function),
            (Dialect::Janet, "defmacro", DefinitionCategory::Macro),
            (Dialect::Fennel, "fn", DefinitionCategory::Function),
            (Dialect::Fennel, "lambda", DefinitionCategory::Function),
            (Dialect::Fennel, "macro", DefinitionCategory::Macro),
            (Dialect::Fennel, "local", DefinitionCategory::Variable),
            (Dialect::Fennel, "global", DefinitionCategory::Variable),
        ] {
            assert_eq!(
                classify_definition_head(dialect, head),
                Some(category),
                "{dialect:?} head {head}"
            );
        }
    }

    #[test]
    fn rejects_cross_dialect_definition_heads_but_keeps_unknown_compatibility() {
        for (dialect, head) in [
            (Dialect::Scheme, "defn"),
            (Dialect::Fennel, "defconst"),
            (Dialect::EmacsLisp, "ns"),
            // `defn-` is NOT foreign to Clojure: it is `clojure.core/defn-`.
            (Dialect::Clojure, "defun"),
            (Dialect::Clojure, "setv"),
            (Dialect::Janet, "defrecord"),
        ] {
            assert_eq!(
                classify_definition_head(dialect, head),
                None,
                "{dialect:?} must reject foreign head {head}"
            );
        }

        assert_eq!(
            classify_definition_head(Dialect::Unknown, "define-widget"),
            Some(DefinitionCategory::Other)
        );
        assert_eq!(classify_definition_head(Dialect::Unknown, "let"), None);
    }

    #[test]
    fn callable_categories_are_limited_to_invokable_definition_kinds() {
        assert!(DefinitionCategory::Function.is_callable());
        assert!(DefinitionCategory::Macro.is_callable());
        assert!(DefinitionCategory::GenericFunction.is_callable());
        assert!(DefinitionCategory::Method.is_callable());
        assert!(!DefinitionCategory::Class.is_callable());
        assert!(!DefinitionCategory::Variable.is_callable());
    }

    #[test]
    fn finds_the_lambda_list_of_a_named_fennel_callable() {
        // Without `fn` here, every report that reads a lambda list — unused
        // parameters, call arity, impact — silently checked nothing in Fennel
        // while working for every other bracket dialect's `defn`.
        for (source, head) in [
            ("(fn add [a b] (+ a b))", "fn"),
            ("(macro m [x] x)", "macro"),
        ] {
            let tree = SyntaxTree::parse_with_dialect(source, Dialect::Fennel).unwrap();
            let view = tree
                .select_path(&Path::from_indexes(vec![0]))
                .unwrap()
                .view();
            let shape = definition_shape(Dialect::Fennel, &view, head)
                .unwrap_or_else(|| panic!("{source} is a definition"));

            assert!(
                shape.lambda_list(&view).is_some(),
                "{source} has a lambda list"
            );
        }

        // An anonymous `fn` puts its body where the parameter list would be,
        // so it must resolve to no lambda list rather than to the wrong one.
        let tree = SyntaxTree::parse_with_dialect("(fn [x] x)", Dialect::Fennel).unwrap();
        let view = tree
            .select_path(&Path::from_indexes(vec![0]))
            .unwrap()
            .view();
        assert_eq!(
            definition_lambda_list_position(Dialect::Fennel, &view, "fn"),
            None
        );
    }

    #[test]
    fn finds_common_lisp_lambda_list_shapes() {
        let source = "(defmethod render :around ((node widget) stream) (draw node stream))";
        let tree = SyntaxTree::parse(source).unwrap();
        let view = tree
            .select_path(&Path::from_indexes(vec![0]))
            .unwrap()
            .view();

        assert_eq!(
            definition_lambda_list_position(Dialect::CommonLisp, &view, "defmethod"),
            Some((3, 0))
        );
        assert_eq!(
            definition_shape(Dialect::CommonLisp, &view, "defmethod").map(|shape| shape.category),
            Some(DefinitionCategory::Method)
        );
        let shape = definition_shape(Dialect::CommonLisp, &view, "defmethod").unwrap();
        assert_eq!(shape.name(&view), Some("render"));
        assert_eq!(
            shape
                .lambda_list(&view)
                .map(|lambda_list| lambda_list.span.slice(source)),
            Some("((node widget) stream)")
        );
        assert_eq!(shape.body_form_count(&view), 1);
        assert_eq!(
            definition_body_start_child_index(
                &view,
                "defmethod",
                Some(DefinitionCategory::Method),
                definition_lambda_list_position(Dialect::CommonLisp, &view, "defmethod")
                    .map(|(index, _)| index)
            ),
            4
        );

        let qualified_tree =
            SyntaxTree::parse("(cl:defmacro with-panel ((panel) &body body) `(progn ,@body))")
                .unwrap();
        let qualified = qualified_tree
            .select_path(&Path::from_indexes(vec![0]))
            .unwrap()
            .view();

        assert_eq!(
            definition_lambda_list_position(Dialect::CommonLisp, &qualified, "cl:defmacro"),
            Some((2, 0))
        );

        let defsetf_source = "(defsetf accessor (item) (value) (list item value))";
        let defsetf_tree = SyntaxTree::parse(defsetf_source).unwrap();
        let defsetf_view = defsetf_tree
            .select_path(&Path::from_indexes(vec![0]))
            .unwrap()
            .view();
        let defsetf_shape =
            definition_shape(Dialect::CommonLisp, &defsetf_view, "defsetf").unwrap();
        assert_eq!(defsetf_shape.lambda_parameter_count(&defsetf_view), Some(1));
        assert_eq!(defsetf_shape.body_form_count(&defsetf_view), 1);
        assert_eq!(
            defsetf_shape
                .body_forms(&defsetf_view)
                .first()
                .map(|body| body.span.slice(defsetf_source)),
            Some("(list item value)")
        );
    }

    #[test]
    fn exposes_path_aware_definition_name_and_body_range() {
        let tree = SyntaxTree::parse("(defun render (node) (draw node) (finish node))").unwrap();
        let view = tree
            .select_path(&Path::from_indexes(vec![0]))
            .unwrap()
            .view();
        let shape = definition_shape(Dialect::CommonLisp, &view, "defun").unwrap();

        let parent_path = Path::from_indexes(vec![0]);
        let target = shape.name_target(&view, &parent_path).unwrap();
        assert_eq!(target.path, Path::from_indexes(vec![0, 1]));
        assert_eq!(target.text, "render");
        assert_eq!(
            target
                .span
                .slice("(defun render (node) (draw node) (finish node))"),
            "render"
        );

        let body = shape.body_range();
        assert!(!body.contains_child(2));
        assert_eq!(
            body.child_path(&parent_path, 3),
            Some(Path::from_indexes(vec![0, 3]))
        );
        assert_eq!(
            body.child_path(&parent_path, 4),
            Some(Path::from_indexes(vec![0, 4]))
        );

        let bounded_body = shape.body_range_in(&view);
        assert!(bounded_body.contains_child(3));
        assert!(bounded_body.contains_child(4));
        assert!(!bounded_body.contains_child(5));
        assert_eq!(
            bounded_body.child_path(&parent_path, 5),
            None,
            "indexes beyond the view must not be considered body forms"
        );
    }
}

#[cfg(test)]
mod clojure_lambda_list_tests {
    use super::*;
    use crate::sexpr::SyntaxTree;

    fn clojure_form(source: &str) -> ExpressionView {
        SyntaxTree::parse_with_dialect(source, Dialect::Clojure)
            .expect("fixture parses")
            .select_path(&Path::from_indexes(vec![0]))
            .expect("select")
            .view()
    }

    fn lambda_list_text<'a>(source: &'a str, head: &str) -> Option<&'a str> {
        let view = clojure_form(source);
        let shape = definition_shape(Dialect::Clojure, &view, head)?;
        let span = shape.lambda_list(&view)?.span;
        Some(span.slice(source))
    }

    #[test]
    fn defmethod_parameters_are_found_after_the_dispatch_value() {
        // A vector dispatch value used to be mistaken for the parameter
        // vector, because the Common Lisp `defmethod` rule takes the first
        // list at or after child 2. Adding a parameter then rewrote the
        // dispatch value and silently changed which calls the method matched.
        assert_eq!(
            lambda_list_text(
                "(defmethod encode [:json :pretty] [x] (str x))",
                "defmethod"
            ),
            Some("[x]")
        );
        assert_eq!(
            lambda_list_text("(defmethod encode :json [x] (str x))", "defmethod"),
            Some("[x]")
        );
    }

    #[test]
    fn defn_parameters_are_found_past_a_docstring_and_attribute_map() {
        assert_eq!(lambda_list_text("(defn f [x] x)", "defn"), Some("[x]"));
        assert_eq!(
            lambda_list_text(r#"(defn f "doc" [x] x)"#, "defn"),
            Some("[x]")
        );
        assert_eq!(
            lambda_list_text(r#"(defn f "doc" {:added "1.0"} [x y] x)"#, "defn"),
            Some("[x y]")
        );
        assert_eq!(
            lambda_list_text("(defn- f {:added \"1.0\"} [x] x)", "defn-"),
            Some("[x]")
        );
    }

    #[test]
    fn multi_arity_defn_reports_no_single_parameter_vector() {
        // Each clause carries its own, so there is no one index to report.
        assert_eq!(lambda_list_text("(defn f ([x] x) ([x y] y))", "defn"), None);
    }
}
