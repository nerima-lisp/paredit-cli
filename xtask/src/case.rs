//! kebab-case in, snake_case and PascalCase derived — the three spellings
//! every generated file needs to agree on.

use anyhow::{Result, bail};

/// A name given as `--rule-name`/`--command-name`, validated once so every
/// generator can trust its three derived spellings.
#[derive(Debug, Clone)]
pub struct Name {
    kebab: String,
}

impl Name {
    pub fn parse(raw: &str) -> Result<Self> {
        let is_valid = !raw.is_empty()
            && raw
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            && !raw.starts_with('-')
            && !raw.ends_with('-')
            && !raw.contains("--");
        if !is_valid {
            bail!(
                "`{raw}` must be lowercase kebab-case (letters, digits, single hyphens), \
                 e.g. `char-case-fold`"
            );
        }
        Ok(Self {
            kebab: raw.to_owned(),
        })
    }

    #[must_use]
    pub fn kebab(&self) -> &str {
        &self.kebab
    }

    #[must_use]
    pub fn snake(&self) -> String {
        self.kebab.replace('-', "_")
    }

    #[must_use]
    pub fn pascal(&self) -> String {
        self.kebab
            .split('-')
            .map(|part| {
                let mut chars = part.chars();
                match chars.next() {
                    Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
                    None => String::new(),
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_snake_and_pascal_from_kebab() {
        let name = Name::parse("char-case-fold").expect("valid name");
        assert_eq!(name.kebab(), "char-case-fold");
        assert_eq!(name.snake(), "char_case_fold");
        assert_eq!(name.pascal(), "CharCaseFold");
    }

    #[test]
    fn rejects_non_kebab_input() {
        assert!(Name::parse("CharCaseFold").is_err());
        assert!(Name::parse("char_case_fold").is_err());
        assert!(Name::parse("-leading").is_err());
        assert!(Name::parse("trailing-").is_err());
        assert!(Name::parse("double--hyphen").is_err());
        assert!(Name::parse("").is_err());
    }
}
