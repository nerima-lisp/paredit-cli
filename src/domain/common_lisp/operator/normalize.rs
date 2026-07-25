fn strip_common_lisp_package_prefix<'a>(head: &'a str, prefix: &str) -> Option<&'a str> {
    head.get(..prefix.len())
        .filter(|candidate| candidate.eq_ignore_ascii_case(prefix))?;
    Some(&head[prefix.len()..])
}

pub(crate) fn normalize_common_lisp_operator_head(head: &str) -> &str {
    strip_common_lisp_package_prefix(head, "cl:")
        .or_else(|| strip_common_lisp_package_prefix(head, "cl-user:"))
        .or_else(|| strip_common_lisp_package_prefix(head, "common-lisp:"))
        .or_else(|| strip_common_lisp_package_prefix(head, "common-lisp-user:"))
        .unwrap_or(head)
}

pub(crate) fn common_lisp_operator_head_eq(head: &str, expected: &str) -> bool {
    common_lisp_symbol_name_eq(head, expected)
}

pub(crate) fn common_lisp_symbol_name_eq(head: &str, expected: &str) -> bool {
    canonical_common_lisp_symbol_name(normalize_common_lisp_operator_head(head))
        == canonical_common_lisp_symbol_name(normalize_common_lisp_operator_head(expected))
}

fn last_common_lisp_package_marker(symbol: &str) -> Option<usize> {
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

fn canonical_common_lisp_symbol_name(symbol: &str) -> String {
    let mut canonical = String::with_capacity(symbol.len());
    let mut escaped = false;
    let mut in_multiple_escape = false;

    for character in symbol.chars() {
        if escaped {
            canonical.push(character);
            escaped = false;
            continue;
        }

        match character {
            '\\' => escaped = true,
            '|' => in_multiple_escape = !in_multiple_escape,
            _ if in_multiple_escape => canonical.push(character),
            _ => canonical.push(character.to_ascii_uppercase()),
        }
    }

    if escaped {
        canonical.push('\\');
    }

    canonical
}

/// Strips a Common Lisp package qualifier (`pkg:sym` or `pkg::sym`) and the
/// `#:` uninterned-symbol reader prefix used in `defpackage` `:export`
/// clauses, returning the bare symbol name.
///
/// A leading colon with nothing before it (`:keyword`) is left untouched:
/// that names a distinct symbol in the `KEYWORD` package, not a qualified
/// reference to a same-named symbol elsewhere.
fn strip_common_lisp_symbol_qualifiers(head: &str) -> &str {
    let head = head.strip_prefix("#:").unwrap_or(head);
    match last_common_lisp_package_marker(head) {
        Some(index) if index > 0 => &head[index + 1..],
        _ => head,
    }
}

pub(crate) fn has_common_lisp_package_qualifier(symbol: &str) -> bool {
    let symbol = symbol.strip_prefix("#:").unwrap_or(symbol);
    last_common_lisp_package_marker(symbol)
        .is_some_and(|index| index > 0 && index + 1 < symbol.len())
}

/// General-purpose Common Lisp symbol-name equality for occurrence matching,
/// rename, and unused-definition detection.
///
/// Unlike [`common_lisp_symbol_name_eq`], which only recognizes the four
/// standard CL home-package aliases so an unrelated dialect or package is
/// never misclassified as a builtin special form, this strips *any* package
/// qualifier or `#:` prefix: `nshell.application:execute-command-line` and
/// `#:execute-command-line` both denote the same symbol as bare
/// `execute-command-line` for the purpose of asking "is this symbol
/// referenced anywhere?" Comparison is case-insensitive per the CLHS reader.
pub(crate) fn common_lisp_symbol_reference_eq(candidate: &str, expected: &str) -> bool {
    canonical_common_lisp_symbol_name(strip_common_lisp_symbol_qualifiers(candidate))
        == canonical_common_lisp_symbol_name(strip_common_lisp_symbol_qualifiers(expected))
}

fn common_lisp_explicit_package_and_name(symbol: &str) -> Option<(&str, &str)> {
    let marker = last_common_lisp_package_marker(symbol)?;
    if marker == 0 || marker + 1 >= symbol.len() {
        return None;
    }

    let package_end = if symbol.as_bytes().get(marker - 1) == Some(&b':') {
        marker - 1
    } else {
        marker
    };
    (package_end > 0).then_some((&symbol[..package_end], &symbol[marker + 1..]))
}

fn canonical_common_lisp_package_identity(package: &str) -> String {
    let canonical = canonical_common_lisp_symbol_name(package);
    if canonical == "CL" {
        "COMMON-LISP".to_owned()
    } else {
        canonical
    }
}

/// Compares user-defined symbols without assuming package visibility.
///
/// An unqualified symbol can only match another unqualified symbol. Explicitly
/// qualified symbols must name the same package, while uninterned symbols are
/// conservatively treated as distinct.
pub(crate) fn common_lisp_symbol_identity_eq(candidate: &str, expected: &str) -> bool {
    if candidate.starts_with("#:") || expected.starts_with("#:") {
        return false;
    }

    match (
        common_lisp_explicit_package_and_name(candidate),
        common_lisp_explicit_package_and_name(expected),
    ) {
        (Some((candidate_package, candidate_name)), Some((expected_package, expected_name))) => {
            canonical_common_lisp_package_identity(candidate_package)
                == canonical_common_lisp_package_identity(expected_package)
                && canonical_common_lisp_symbol_name(candidate_name)
                    == canonical_common_lisp_symbol_name(expected_name)
        }
        (None, None) => {
            canonical_common_lisp_symbol_name(candidate)
                == canonical_common_lisp_symbol_name(expected)
        }
        _ => false,
    }
}

/// Canonical unqualified name of `symbol`, for reference indexes and
/// prefiltering parsed atoms.
///
/// Common Lisp escape syntax affects case: `foo` and `|FOO|` denote the same
/// symbol, while `foo` and `|foo|` do not. Removing the reader escapes and
/// folding only unescaped characters therefore produces a stable key without
/// collapsing distinct escaped names.
pub(crate) fn common_lisp_symbol_reference_needle(symbol: &str) -> String {
    canonical_common_lisp_symbol_name(strip_common_lisp_symbol_qualifiers(symbol))
}

/// Strips the reader-syntax wrapper from a package or ASDF system
/// designator — a leading `:` (keyword, `:app`), a leading `#:` (uninterned
/// symbol, `#:app`, common in `.asd` files), or surrounding `"..."` (string,
/// `"app"`) — so the result compares equal regardless of which spelling a
/// particular form used. A designator already written as a plain symbol is
/// returned as-is.
pub(crate) fn normalize_common_lisp_package_designator(designator: &str) -> &str {
    let unprefixed = designator
        .strip_prefix("#:")
        .or_else(|| designator.strip_prefix(':'))
        .unwrap_or(designator);
    unprefixed
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(unprefixed)
}

pub(crate) fn is_common_lisp_declaration_form(head: &str) -> bool {
    common_lisp_operator_head_eq(head, "declare")
        || common_lisp_operator_head_eq(head, "declaim")
        || common_lisp_operator_head_eq(head, "proclaim")
}

/// Returns true for the "earmuffed" naming convention (`*name*`) Common Lisp
/// programmers use, near-universally, to mark a symbol as a special
/// (dynamically scoped) variable declared elsewhere via `defvar`/
/// `defparameter`/`declaim special`.
///
/// This matters for `let`-binding analysis: rebinding a special variable
/// (`(let ((*read-eval* nil)) (read stream))`) is meaningful purely through
/// its dynamic-scope side effect for the body's dynamic extent — every
/// nested call that reads the special variable sees the rebound value, with
/// no textual reference to the binding name required anywhere in the
/// lexical body. A lexical-scope-only "is this name referenced in the body"
/// check is the wrong question for such a binding and must not flag it as
/// dead.
pub(crate) fn is_common_lisp_earmuffed_special_variable_name(name: &str) -> bool {
    let name = strip_common_lisp_symbol_qualifiers(name);
    let bytes = name.as_bytes();
    bytes.len() > 2 && bytes[0] == b'*' && bytes[bytes.len() - 1] == b'*'
}
