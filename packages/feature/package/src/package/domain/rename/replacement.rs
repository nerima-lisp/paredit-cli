use crate::domain::sexpr::SymbolName;

use super::super::syntax::normalize_package_atom;

pub(super) fn package_designator_replacement(text: &str, to: &SymbolName) -> String {
    let target = normalize_package_atom(to.as_str());
    if text.starts_with("#:") {
        format!("#:{target}")
    } else if text.starts_with(':') {
        format!(":{target}")
    } else {
        target.to_owned()
    }
}

pub(super) fn package_qualified_replacement(
    text: &str,
    from: &SymbolName,
    to: &SymbolName,
) -> Option<String> {
    if text.starts_with(':') {
        return None;
    }
    let separator = text.find(':')?;
    let package = &text[..separator];
    if package.is_empty() || !package.eq_ignore_ascii_case(normalize_package_atom(from.as_str())) {
        return None;
    }

    Some(format!(
        "{}{}",
        normalize_package_atom(to.as_str()),
        &text[separator..]
    ))
}
