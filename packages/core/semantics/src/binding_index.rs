//! Validated one-based binding boundaries used by binding composition plans.

use anyhow::{Result, bail};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BindingIndex(usize);

impl BindingIndex {
    pub fn new(value: usize) -> Result<Self> {
        if value == 0 {
            bail!("binding index must be greater than zero");
        }
        Ok(Self(value))
    }

    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_zero_based_boundary() {
        assert!(BindingIndex::new(0).is_err());
        assert_eq!(BindingIndex::new(1).expect("valid index").get(), 1);
    }
}
