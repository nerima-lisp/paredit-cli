use crate::domain::sexpr::{Delimiter, ExpressionKind, ExpressionView, SymbolName};

use super::super::selection::atom_text;

pub(super) fn rewrite_clojure_keys_map_pattern(
    input: &str,
    map_pattern: &ExpressionView,
    renamed_name: &str,
    to: &SymbolName,
) -> String {
    let mut items = vec![format!("{} :{}", to.as_str(), renamed_name)];
    let mut index = 0usize;

    while index < map_pattern.children.len() {
        let child = &map_pattern.children[index];
        if atom_text(child) == Some(":keys") {
            if let Some(keys_form) = map_pattern.children.get(index + 1) {
                let remaining = clojure_keys_shorthand_remaining_names(keys_form, renamed_name);
                if !remaining.is_empty() {
                    items.push(":keys".to_owned());
                    items.push(format!("[{}]", remaining.join(" ")));
                }
                index += 2;
                continue;
            }
        }

        items.push(child.span.slice(input).to_owned());
        index += 1;
    }

    format!("{{{}}}", items.join(" "))
}

fn clojure_keys_shorthand_remaining_names(
    keys_form: &ExpressionView,
    renamed_name: &str,
) -> Vec<String> {
    if keys_form.kind != ExpressionKind::List || keys_form.delimiter != Some(Delimiter::Bracket) {
        return Vec::new();
    }

    keys_form
        .children
        .iter()
        .filter_map(atom_text)
        .filter(|name| *name != renamed_name)
        .map(str::to_owned)
        .collect()
}
