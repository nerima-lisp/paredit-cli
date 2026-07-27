use std::path::PathBuf;

use proptest::prelude::*;

use crate::domain::definition::DefinitionCategory;
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::SyntaxTree;

use super::*;

mod inventory;
mod property;
mod unused;
