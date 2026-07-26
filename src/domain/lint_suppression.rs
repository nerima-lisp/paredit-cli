//! Inline lint-suppression directives: source comments that silence
//! [`crate::domain::lint_report`] findings on a specific line, the way
//! `eslint-disable-next-line` or `# noqa` do elsewhere.
//!
//! A directive is any Common Lisp comment containing the token
//! `paredit:ignore`, optionally followed by rule names:
//!
//! ```lisp
//! ;; paredit:ignore single-arg-comparison
//! (< x)                                     ; suppressed (directive on the line above)
//! (= y)  ; paredit:ignore                    ; suppressed (trailing directive, all rules)
//! ```
//!
//! `paredit:ignore` with no rule names suppresses every rule on the target
//! line; with names it suppresses only those (comma- or space-separated). A
//! directive on its own line protects the *next* source line; a trailing
//! directive (code before the `;`) protects its *own* line.
//!
//! Detection scans the whole file tracking string and `#\` character-literal
//! state, so a `;` inside `"a ; b"` or `#\;` is never mistaken for a comment.

use std::collections::{HashMap, HashSet};

/// Which rules a directive silences on one line.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Scope {
    /// Every rule (a bare `paredit:ignore`).
    All,
    /// Only the named rules.
    Rules(Vec<String>),
}

/// One parsed directive, retained for unused-suppression reporting.
#[derive(Debug, Clone)]
struct Directive {
    /// The line the `; paredit:ignore` comment sits on.
    comment_line: usize,
    /// The line it protects (its own, or the next line).
    target_line: usize,
    scope: Scope,
}

/// A directive (or some of its named rules) that silenced no finding — a stale
/// ignore or a typo'd rule name worth removing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnusedSuppression {
    pub comment_line: usize,
    pub target_line: usize,
    /// `None` for a bare `paredit:ignore` that matched nothing; `Some(rules)`
    /// for the named rules that matched nothing.
    pub unused_rules: Option<Vec<String>>,
}

/// The set of per-line lint suppressions parsed from one source file.
#[derive(Debug, Default)]
pub struct LintSuppressions {
    /// 1-based line number -> the rules suppressed on that line.
    by_line: HashMap<usize, Scope>,
    /// Every directive as parsed, for unused-suppression detection.
    directives: Vec<Directive>,
}

impl LintSuppressions {
    /// Parses every `paredit:ignore` directive in `text`.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut by_line = HashMap::new();
        let mut directives = Vec::new();
        let mut in_string = false;
        for (index, line) in text.lines().enumerate() {
            let line_number = index + 1;
            let (comment_start, had_code, next_in_string) = scan_line(line, in_string);
            in_string = next_in_string;
            let Some(start) = comment_start else {
                continue;
            };
            let Some(scope) = directive_scope(&line[start..]) else {
                continue;
            };
            // A trailing directive protects its own line; a directive alone on
            // its line protects the next one.
            let target = if had_code {
                line_number
            } else {
                line_number + 1
            };
            merge(&mut by_line, target, scope.clone());
            directives.push(Directive {
                comment_line: line_number,
                target_line: target,
                scope,
            });
        }
        Self {
            by_line,
            directives,
        }
    }

    /// Whether `rule` is suppressed on 1-based `line`.
    #[must_use]
    pub fn is_suppressed(&self, rule: &str, line: usize) -> bool {
        match self.by_line.get(&line) {
            None => false,
            Some(Scope::All) => true,
            Some(Scope::Rules(rules)) => rules.iter().any(|name| name == rule),
        }
    }

    /// Whether any directive was parsed (for reporting/telemetry).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_line.is_empty()
    }

    /// Returns the directives (or named rules within them) that silenced no
    /// finding, given `present`: for each line, the set of rule names that
    /// actually reported a finding there. A bare `paredit:ignore` is unused
    /// when its target line has no finding at all; a named directive is unused
    /// for each listed rule absent from its target line.
    #[must_use]
    pub fn unused_directives(
        &self,
        present: &HashMap<usize, HashSet<&'static str>>,
    ) -> Vec<UnusedSuppression> {
        let empty = HashSet::new();
        let mut unused = Vec::new();
        for directive in &self.directives {
            let on_line = present.get(&directive.target_line).unwrap_or(&empty);
            match &directive.scope {
                Scope::All => {
                    if on_line.is_empty() {
                        unused.push(UnusedSuppression {
                            comment_line: directive.comment_line,
                            target_line: directive.target_line,
                            unused_rules: None,
                        });
                    }
                }
                Scope::Rules(rules) => {
                    let stale: Vec<String> = rules
                        .iter()
                        .filter(|rule| !on_line.contains(rule.as_str()))
                        .cloned()
                        .collect();
                    if !stale.is_empty() {
                        unused.push(UnusedSuppression {
                            comment_line: directive.comment_line,
                            target_line: directive.target_line,
                            unused_rules: Some(stale),
                        });
                    }
                }
            }
        }
        unused
    }
}

/// Scans one line (given the incoming in-string state) and returns the byte
/// offset where its comment begins (if any), whether non-whitespace code
/// preceded that comment, and the in-string state after the line.
fn scan_line(line: &str, mut in_string: bool) -> (Option<usize>, bool, bool) {
    let chars: Vec<(usize, char)> = line.char_indices().collect();
    let mut had_code = false;
    let mut i = 0;
    while i < chars.len() {
        let (offset, ch) = chars[i];
        if in_string {
            match ch {
                '\\' => i += 2, // skip the escaped character
                '"' => {
                    in_string = false;
                    i += 1;
                }
                _ => i += 1,
            }
            continue;
        }
        match ch {
            '"' => {
                in_string = true;
                had_code = true;
                i += 1;
            }
            // A `#\c` character literal: the char after `#\` is data, not code
            // structure, so skip all three and never read its `;` as a comment.
            '#' if chars.get(i + 1).is_some_and(|(_, c)| *c == '\\') => {
                had_code = true;
                i += 3;
            }
            ';' => return (Some(offset), had_code, in_string),
            _ => {
                if !ch.is_whitespace() {
                    had_code = true;
                }
                i += 1;
            }
        }
    }
    (None, had_code, in_string)
}

/// Parses a comment (`;`-prefixed) into a suppression scope, or `None` if it
/// carries no `paredit:ignore` directive.
fn directive_scope(comment: &str) -> Option<Scope> {
    let body = comment.trim_start_matches(';').trim_start();
    let position = body.find("paredit:ignore")?;
    let rest = &body[position + "paredit:ignore".len()..];
    let rules: Vec<String> = rest
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|token| !token.is_empty())
        .map(str::to_owned)
        .collect();
    Some(if rules.is_empty() {
        Scope::All
    } else {
        Scope::Rules(rules)
    })
}

/// Merges a new scope into whatever a line already carries (`All` dominates;
/// two rule lists union).
fn merge(by_line: &mut HashMap<usize, Scope>, line: usize, scope: Scope) {
    match by_line.get_mut(&line) {
        None => {
            by_line.insert(line, scope);
        }
        Some(Scope::All) => {}
        Some(existing @ Scope::Rules(_)) => match scope {
            Scope::All => *existing = Scope::All,
            Scope::Rules(more) => {
                if let Scope::Rules(rules) = existing {
                    rules.extend(more);
                }
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn own_line_directive_protects_the_next_line() {
        let sup = LintSuppressions::parse(";; paredit:ignore single-arg-comparison\n(< x)\n");
        assert!(sup.is_suppressed("single-arg-comparison", 2));
        assert!(!sup.is_suppressed("single-arg-comparison", 1));
    }

    #[test]
    fn trailing_directive_protects_its_own_line() {
        let sup = LintSuppressions::parse("(< x)  ; paredit:ignore single-arg-comparison\n");
        assert!(sup.is_suppressed("single-arg-comparison", 1));
    }

    #[test]
    fn bare_directive_suppresses_every_rule() {
        let sup = LintSuppressions::parse(";; paredit:ignore\n(< x)\n");
        assert!(sup.is_suppressed("single-arg-comparison", 2));
        assert!(sup.is_suppressed("redundant-quote", 2));
    }

    #[test]
    fn named_directive_only_suppresses_that_rule() {
        let sup = LintSuppressions::parse(";; paredit:ignore redundant-quote\n(< x)\n");
        assert!(sup.is_suppressed("redundant-quote", 2));
        assert!(!sup.is_suppressed("single-arg-comparison", 2));
    }

    #[test]
    fn multiple_rules_comma_or_space_separated() {
        let sup = LintSuppressions::parse(
            ";; paredit:ignore redundant-quote, single-arg-comparison\n(x)\n",
        );
        assert!(sup.is_suppressed("redundant-quote", 2));
        assert!(sup.is_suppressed("single-arg-comparison", 2));
    }

    #[test]
    fn a_semicolon_inside_a_string_is_not_a_directive() {
        let sup = LintSuppressions::parse("(format t \"; paredit:ignore\")\n(< x)\n");
        assert!(sup.is_empty());
        assert!(!sup.is_suppressed("single-arg-comparison", 2));
    }

    #[test]
    fn a_semicolon_char_literal_is_not_a_directive() {
        let sup = LintSuppressions::parse("(princ #\\; ) ; paredit:ignore redundant-quote\n");
        // The `#\;` is data; the real comment after it is the directive, which
        // (being trailing) protects its own line.
        assert!(sup.is_suppressed("redundant-quote", 1));
    }

    #[test]
    fn no_directive_means_no_suppression() {
        let sup = LintSuppressions::parse("(< x) ; just a normal comment\n");
        assert!(sup.is_empty());
    }

    #[test]
    fn multiline_string_does_not_leak_a_comment() {
        // The `;` sits inside a string that opened on the previous line.
        let sup = LintSuppressions::parse(
            "(defparameter *s* \"line one\n ; paredit:ignore\n end\")\n(< x)\n",
        );
        assert!(sup.is_empty());
    }

    fn present(entries: &[(usize, &'static str)]) -> HashMap<usize, HashSet<&'static str>> {
        let mut map: HashMap<usize, HashSet<&'static str>> = HashMap::new();
        for (line, rule) in entries {
            map.entry(*line).or_default().insert(rule);
        }
        map
    }

    #[test]
    fn a_named_directive_matching_a_finding_is_used() {
        let sup = LintSuppressions::parse(";; paredit:ignore single-arg-comparison\n(< x)\n");
        let unused = sup.unused_directives(&present(&[(2, "single-arg-comparison")]));
        assert!(unused.is_empty());
    }

    #[test]
    fn a_named_directive_matching_nothing_is_unused() {
        let sup = LintSuppressions::parse(";; paredit:ignore single-arg-comparison\n(foo)\n");
        let unused = sup.unused_directives(&present(&[]));
        assert_eq!(unused.len(), 1);
        assert_eq!(
            unused[0].unused_rules.as_deref(),
            Some(&["single-arg-comparison".to_owned()][..])
        );
        assert_eq!(unused[0].comment_line, 1);
        assert_eq!(unused[0].target_line, 2);
    }

    #[test]
    fn a_bare_directive_over_a_finding_free_line_is_unused() {
        let sup = LintSuppressions::parse(";; paredit:ignore\n(clean)\n");
        let unused = sup.unused_directives(&present(&[]));
        assert_eq!(unused.len(), 1);
        assert_eq!(unused[0].unused_rules, None);
    }

    #[test]
    fn a_bare_directive_over_any_finding_is_used() {
        let sup = LintSuppressions::parse(";; paredit:ignore\n(< x)\n");
        let unused = sup.unused_directives(&present(&[(2, "single-arg-comparison")]));
        assert!(unused.is_empty());
    }

    #[test]
    fn only_the_stale_rule_of_a_multi_rule_directive_is_reported() {
        let sup = LintSuppressions::parse(
            ";; paredit:ignore redundant-quote single-arg-comparison\n(< x)\n",
        );
        // single-arg-comparison fired here; redundant-quote did not.
        let unused = sup.unused_directives(&present(&[(2, "single-arg-comparison")]));
        assert_eq!(unused.len(), 1);
        assert_eq!(
            unused[0].unused_rules.as_deref(),
            Some(&["redundant-quote".to_owned()][..])
        );
    }

    #[test]
    fn two_directives_on_one_target_line_union() {
        let sup = LintSuppressions::parse(
            ";; paredit:ignore redundant-quote\n(< x)  ; paredit:ignore single-arg-comparison\n",
        );
        // Line 2 gets `redundant-quote` (from the line above) and
        // `single-arg-comparison` (trailing on itself).
        assert!(sup.is_suppressed("redundant-quote", 2));
        assert!(sup.is_suppressed("single-arg-comparison", 2));
    }
}
