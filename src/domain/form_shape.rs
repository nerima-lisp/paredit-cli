use std::fmt::{Display, Formatter};

use crate::domain::sexpr::{Delimiter, ExpressionKind, ExpressionView};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FormShape(String);

impl FormShape {
    #[must_use]
    pub fn new(shape: String) -> Self {
        Self(shape)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for FormShape {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<&str> for FormShape {
    fn from(value: &str) -> Self {
        Self::new(value.to_owned())
    }
}

impl From<FormShape> for String {
    fn from(value: FormShape) -> Self {
        value.0
    }
}

impl AsRef<str> for FormShape {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Display for FormShape {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[must_use]
pub fn duplicate_shape(view: &ExpressionView, preserve_list_head: bool) -> FormShape {
    let mut output = String::new();
    write_duplicate_shape(view, preserve_list_head, &mut output);
    FormShape::new(output)
}

// Writes into one shared buffer instead of allocating a `Vec<String>` and
// joining at every recursion level, which made shape construction cost
// O(subtree size × depth) allocations for deeply nested forms.
fn write_duplicate_shape(view: &ExpressionView, preserve_list_head: bool, output: &mut String) {
    match view.kind {
        ExpressionKind::Root => {
            output.push_str("(root ");
            for (index, child) in view.children.iter().enumerate() {
                if index > 0 {
                    output.push(' ');
                }
                write_duplicate_shape(child, false, output);
            }
            output.push(')');
        }
        ExpressionKind::Atom => output.push_str("_atom"),
        ExpressionKind::List => {
            let delimiter = match view.delimiter {
                Some(Delimiter::Paren) => "paren",
                Some(Delimiter::Bracket) => "bracket",
                Some(Delimiter::Brace) => "brace",
                None => "list",
            };
            output.push('(');
            output.push_str(delimiter);
            output.push(' ');
            for (index, child) in view.children.iter().enumerate() {
                if index > 0 {
                    output.push(' ');
                }
                if preserve_list_head && index == 0 {
                    match atom_text(child) {
                        Some(head) => {
                            output.push_str("head:");
                            output.push_str(head);
                        }
                        None => write_duplicate_shape(child, false, output),
                    }
                } else {
                    write_duplicate_shape(child, false, output);
                }
            }
            output.push(')');
        }
    }
}

fn atom_text(view: &ExpressionView) -> Option<&str> {
    (view.kind == ExpressionKind::Atom)
        .then_some(view.text.as_deref())
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::sexpr::{ExpressionPath, SyntaxTree};

    fn shape(input: &str, path: &str, preserve_list_head: bool) -> FormShape {
        let tree = SyntaxTree::parse(input).expect("parse");
        let selection = tree
            .select_path(&path.parse::<ExpressionPath>().expect("path"))
            .expect("select");
        duplicate_shape(&selection.view(), preserve_list_head)
    }

    #[test]
    fn identical_structure_has_the_same_shape() {
        let left = shape("(let ((value 1)) (+ value 2))", "0", false);
        let right = shape("(defun ((slot 9)) (* slot 3))", "0", false);

        assert_eq!(left, right);
        assert_eq!(
            left.as_str(),
            "(paren _atom (paren (paren _atom _atom)) (paren _atom _atom _atom))"
        );
    }

    #[test]
    fn preserves_the_list_head_when_requested() {
        let shape = shape("(call alpha beta)", "0", true);

        assert_eq!(shape.as_str(), "(paren head:call _atom _atom)");
    }

    #[test]
    fn preserves_the_list_delimiter_kind() {
        let paren = shape("(call alpha beta)", "0", false);
        let bracket = shape("[call alpha beta]", "0", false);
        let brace = shape("{call alpha beta}", "0", false);

        assert_eq!(paren.as_str(), "(paren _atom _atom _atom)");
        assert_eq!(bracket.as_str(), "(bracket _atom _atom _atom)");
        assert_eq!(brace.as_str(), "(brace _atom _atom _atom)");
        assert_ne!(paren, bracket);
        assert_ne!(paren, brace);
        assert_ne!(bracket, brace);
    }
}
