use super::*;

mod edit;
mod formatter;
mod parser;
mod property;
mod tree;

fn parse_path(path: &str) -> ExpressionPath {
    path.parse().expect("valid path")
}
