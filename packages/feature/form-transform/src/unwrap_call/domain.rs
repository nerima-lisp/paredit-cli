//! Domain planning for replacing a call with one of its arguments.

use anyhow::{Context, Result};

use crate::domain::common_lisp::common_lisp_symbol_reference_eq;
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{
    ByteSpan, Delimiter, ExpressionKind, ExpressionView, Path, SymbolName, SyntaxTree,
};

#[derive(Debug, Clone)]
pub(crate) struct Request<'a> {
    pub input: &'a str,
    pub dialect: Dialect,
    pub path: Option<Path>,
    pub target: ExpressionView,
    pub expected_function: Option<SymbolName>,
    pub argument_index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Plan {
    pub dialect: Dialect,
    pub path: Option<Path>,
    pub function: SymbolName,
    pub span: ByteSpan,
    pub argument_index: usize,
    pub argument_span: ByteSpan,
    pub call_argument_count: usize,
    pub replacement: String,
    pub rewritten: String,
    pub changed: bool,
}

pub(crate) fn validate_dialect(dialect: Dialect) -> Result<()> {
    match dialect {
        Dialect::CommonLisp
        | Dialect::EmacsLisp
        | Dialect::Lfe
        | Dialect::Scheme
        | Dialect::Racket
        | Dialect::Hy
        | Dialect::Carp
        | Dialect::Clojure
        | Dialect::Janet
        | Dialect::Fennel => Ok(()),
        Dialect::Unknown => anyhow::bail!("unwrap-call requires a known dialect"),
    }
}

pub(crate) fn plan(request: Request<'_>) -> Result<Plan> {
    validate_dialect(request.dialect)?;

    SyntaxTree::parse_with_dialect(request.input, request.dialect)
        .context("unwrap-call input does not parse")?;

    if request.target.kind != ExpressionKind::List
        || request.target.delimiter != Some(Delimiter::Paren)
    {
        anyhow::bail!("unwrap-call target must be a parenthesized call");
    }

    let head = request
        .target
        .children
        .first()
        .and_then(|child| child.text.as_deref())
        .context("unwrap-call target must have an atom function head")?;
    let function = SymbolName::new(head)?;

    if let Some(expected) = &request.expected_function {
        let matches = match request.dialect {
            Dialect::CommonLisp => {
                common_lisp_symbol_reference_eq(expected.as_str(), function.as_str())
            }
            Dialect::EmacsLisp
            | Dialect::Lfe
            | Dialect::Scheme
            | Dialect::Racket
            | Dialect::Hy
            | Dialect::Carp
            | Dialect::Clojure
            | Dialect::Janet
            | Dialect::Fennel => expected.as_str() == function.as_str(),
            Dialect::Unknown => unreachable!("dialect was validated before parsing"),
        };
        if !matches {
            anyhow::bail!(
                "unwrap-call expected function {}, found {}",
                expected.as_str(),
                function.as_str()
            );
        }
    }

    let child_index = request
        .argument_index
        .checked_add(1)
        .context("--argument-index is too large to address any call argument")?;
    let argument = request.target.children.get(child_index).with_context(|| {
        format!(
            "argument index {} is out of range for {} argument(s)",
            request.argument_index,
            request.target.children.len().saturating_sub(1)
        )
    })?;
    let replacement = argument.span.slice(request.input).to_owned();
    SyntaxTree::parse_with_dialect(&replacement, request.dialect)
        .context("unwrap-call replacement is not parseable")?;

    let mut rewritten = request.input.to_owned();
    rewritten.replace_range(request.target.span.as_range(), &replacement);
    SyntaxTree::parse_with_dialect(&rewritten, request.dialect)
        .context("unwrap-call rewritten output is not parseable")?;

    Ok(Plan {
        dialect: request.dialect,
        path: request.path,
        function,
        span: request.target.span,
        argument_index: request.argument_index,
        argument_span: argument.span,
        call_argument_count: request.target.children.len().saturating_sub(1),
        changed: request.target.span.slice(request.input) != replacement,
        replacement,
        rewritten,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(input: &str, dialect: Dialect, path: &str) -> ExpressionView {
        let tree = SyntaxTree::parse_with_dialect(input, dialect)
            .unwrap_or_else(|error| panic!("{}: {error}", dialect.label()));
        tree.select_path(&path.parse::<Path>().expect("valid test path"))
            .expect("test target exists")
            .view()
    }

    fn request<'a>(
        input: &'a str,
        dialect: Dialect,
        path: &str,
        expected_function: Option<&str>,
        argument_index: usize,
    ) -> Request<'a> {
        Request {
            input,
            dialect,
            path: Some(path.parse::<Path>().expect("valid test path")),
            target: target(input, dialect, path),
            expected_function: expected_function
                .map(SymbolName::new)
                .transpose()
                .expect("valid expected function"),
            argument_index,
        }
    }

    #[test]
    fn supports_all_known_dialects_with_their_reader_forms() {
        let cases = [
            (Dialect::CommonLisp, r"(wrap #\))", r"#\)"),
            (Dialect::EmacsLisp, r"(wrap ?\))", r"?\)"),
            (Dialect::Scheme, "(wrap #u8(1 2))", "#u8(1 2)"),
            (
                Dialect::Clojure,
                r#"(wrap #inst "2020-01-01")"#,
                r#"#inst "2020-01-01""#,
            ),
            (Dialect::Janet, "(wrap ;value)", ";value"),
            (Dialect::Fennel, "(wrap #(value))", "#(value)"),
        ];

        for (dialect, input, expected_replacement) in cases {
            let plan = plan(request(input, dialect, "0", Some("wrap"), 0))
                .unwrap_or_else(|error| panic!("{}: {error}", dialect.label()));

            assert_eq!(plan.dialect, dialect);
            assert_eq!(plan.function.as_str(), "wrap");
            assert_eq!(plan.argument_index, 0);
            assert_eq!(plan.call_argument_count, 1);
            assert_eq!(plan.replacement, expected_replacement);
            assert_eq!(plan.rewritten, expected_replacement);
            assert!(plan.changed);
            SyntaxTree::parse_with_dialect(&plan.rewritten, dialect)
                .unwrap_or_else(|error| panic!("{} output: {error}", dialect.label()));
        }
    }

    #[test]
    fn unknown_dialect_fails_before_malformed_input_is_parsed() {
        let error = plan(Request {
            input: ")",
            dialect: Dialect::Unknown,
            path: None,
            target: target("(wrap value)", Dialect::CommonLisp, "0"),
            expected_function: None,
            argument_index: 0,
        })
        .expect_err("unknown dialect must fail closed");

        assert_eq!(error.to_string(), "unwrap-call requires a known dialect");
    }

    #[test]
    fn common_lisp_expected_function_ignores_case_and_package_qualifiers() {
        let input = r"(PKG:WRAP #\))";
        let plan = plan(request(
            input,
            Dialect::CommonLisp,
            "0",
            Some("other-package:wrap"),
            0,
        ))
        .expect("Common Lisp symbol references should match");

        assert_eq!(plan.function.as_str(), "PKG:WRAP");
        assert_eq!(plan.replacement, r"#\)");
        assert_eq!(plan.rewritten, r"#\)");
    }

    #[test]
    fn non_common_lisp_expected_function_comparison_is_exact() {
        for dialect in [
            Dialect::EmacsLisp,
            Dialect::Scheme,
            Dialect::Clojure,
            Dialect::Janet,
            Dialect::Fennel,
        ] {
            for input in ["(WRAP value)", "(pkg:wrap value)"] {
                let error = plan(request(input, dialect, "0", Some("wrap"), 0))
                    .expect_err("non-Common-Lisp head comparison must be exact");
                assert!(
                    error
                        .to_string()
                        .starts_with("unwrap-call expected function wrap, found "),
                    "{}: {error}",
                    dialect.label()
                );
            }
        }
    }

    #[test]
    fn preserves_selected_call_and_argument_spans() {
        let input = "(outer (wrap first second) tail)";
        let plan = plan(request(input, Dialect::Scheme, "0.1", Some("wrap"), 1))
            .expect("nested selected call should unwrap");

        assert_eq!(plan.path, Some("0.1".parse::<Path>().expect("valid path")));
        assert_eq!(plan.span.slice(input), "(wrap first second)");
        assert_eq!(plan.argument_span.slice(input), "second");
        assert_eq!(plan.call_argument_count, 2);
        assert_eq!(plan.replacement, "second");
        assert_eq!(plan.rewritten, "(outer second tail)");
    }
}
