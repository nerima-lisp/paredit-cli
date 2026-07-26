use std::collections::{HashMap, HashSet};

use crate::domain::common_lisp::common_lisp_symbol_reference_needle;
use crate::domain::definition::DefinitionCategory;
use crate::domain::package_report::PackageDefinitionReport;
use crate::domain::remove_unused_definition::types::UnusedDefinitionDefinition;

/// Bulk removal requires the explicit `--include-protected` opt-in for any
/// category `DefinitionCategory::is_bulk_removable` excludes; see that
/// method for why each excluded category cannot be verified from direct
/// references alone. `remove-unused-definitions` and `unused-definition-report`
/// share this single definition so the two commands never disagree on which
/// categories "zero direct references" is a trustworthy signal for.
pub(super) const fn definition_is_bulk_removable(category: DefinitionCategory) -> bool {
    category.is_bulk_removable()
}

pub(super) fn collect_exported_symbol_index(
    packages: &[PackageDefinitionReport],
) -> HashMap<String, HashSet<String>> {
    let mut exported = HashMap::new();

    for package in packages {
        let normalized_exports: Vec<String> = package
            .exports
            .iter()
            .map(|symbol| normalize_symbol_key(symbol))
            .collect();

        for package_key in package_export_keys(package) {
            let symbols = exported.entry(package_key).or_insert_with(HashSet::new);
            symbols.extend(normalized_exports.iter().cloned());
        }
    }

    exported
}

pub(super) fn definition_is_exported(
    definition: &UnusedDefinitionDefinition,
    exported_symbols: &HashMap<String, HashSet<String>>,
) -> bool {
    let (Some(package), Some(name)) = (&definition.package, &definition.name) else {
        return false;
    };
    let Some(exports) = exported_symbols.get(&normalize_package_key(package)) else {
        return false;
    };
    exports.contains(&normalize_symbol_key(name))
}

fn normalize_package_key(value: &str) -> String {
    normalize_keyword_prefix(value).to_ascii_lowercase()
}

fn package_export_keys(package: &PackageDefinitionReport) -> Vec<String> {
    let mut keys = Vec::with_capacity(1 + package.nicknames.len());
    keys.push(normalize_package_key(&package.name));
    keys.extend(
        package
            .nicknames
            .iter()
            .map(|nickname| normalize_package_key(nickname)),
    );
    keys.sort();
    keys.dedup();
    keys
}

fn normalize_symbol_key(value: &str) -> String {
    common_lisp_symbol_reference_needle(normalize_keyword_prefix(value))
}

fn normalize_keyword_prefix(value: &str) -> &str {
    value
        .strip_prefix("#:")
        .or_else(|| value.strip_prefix(':'))
        .unwrap_or(value)
}

#[cfg(test)]
mod tests {
    use super::normalize_symbol_key;

    #[test]
    fn normalizes_only_unescaped_package_qualifiers_and_symbol_characters() {
        assert_eq!(normalize_symbol_key("pkg:foo"), "FOO");
        assert_eq!(normalize_symbol_key("|pkg:name|:foo"), "FOO");
        assert_eq!(normalize_symbol_key("pkg:|foo:bar|"), "foo:bar");
        assert_eq!(normalize_symbol_key("|foo:bar|"), "foo:bar");
    }

    #[test]
    fn preserves_distinct_escaped_symbol_names() {
        assert_eq!(normalize_symbol_key("foo"), normalize_symbol_key("|FOO|"));
        assert_ne!(normalize_symbol_key("foo"), normalize_symbol_key("|foo|"));
    }
}
