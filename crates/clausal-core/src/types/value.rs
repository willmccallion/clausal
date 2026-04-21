//! [`Value`]: the three-valued truth assignment of a literal.

/// The three-valued truth assignment of a literal in a partial model.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
#[repr(u8)]
pub enum Value {
    /// The literal has not been assigned a truth value yet.
    #[default]
    Unassigned = 0,
    /// The literal evaluates to true under the current assignment.
    True = 1,
    /// The literal evaluates to false under the current assignment.
    False = 2,
}

impl Value {
    /// Returns `true` if this value is either `True` or `False`.
    #[inline]
    #[must_use]
    pub const fn is_assigned(self) -> bool {
        !matches!(self, Self::Unassigned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_unassigned() {
        assert_eq!(Value::default(), Value::Unassigned);
    }

    #[test]
    fn is_assigned_matches() {
        assert!(!Value::Unassigned.is_assigned());
        assert!(Value::True.is_assigned());
        assert!(Value::False.is_assigned());
    }
}
