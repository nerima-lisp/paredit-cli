mod core;
mod lists;
mod styles;

const MAX_INLINE_WIDTH: usize = 80;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Formatter {
    indent: usize,
}
