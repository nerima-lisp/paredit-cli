//! Literal-place modify-macro (`(incf 5)`, `(push x 3)`, … — a literal where a
//! place belongs) detection across explicit files.

pub use crate::literal_place::domain::{
    LiteralPlaceItem, LiteralPlacePolicy, LiteralPlacePolicyOptions, LiteralPlaceSummary,
    collect_literal_places, evaluate_literal_place_policy, summarize_literal_places,
};
