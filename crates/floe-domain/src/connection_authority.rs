use std::num::NonZeroU64;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceAuthority {
    incarnation: Uuid,
    epoch: NonZeroU64,
}

impl SourceAuthority {
    pub fn new() -> Self {
        Self {
            incarnation: Uuid::new_v4(),
            epoch: NonZeroU64::MIN,
        }
    }

    pub fn is_valid(self) -> bool {
        !self.incarnation.is_nil()
    }

    pub fn advance(self) -> Option<Self> {
        Some(Self {
            incarnation: self.incarnation,
            epoch: self.epoch.checked_add(1)?,
        })
    }
}

impl Default for SourceAuthority {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authority_changes_without_reusing_an_incarnation() {
        let authority = SourceAuthority::new();
        let advanced = authority.advance().unwrap();
        assert!(authority.is_valid());
        assert_eq!(authority.incarnation, advanced.incarnation);
        assert_eq!(advanced.epoch.get(), 2);
        assert_ne!(SourceAuthority::new().incarnation, authority.incarnation);
    }

    #[test]
    fn exhaustion_never_wraps() {
        let exhausted = SourceAuthority {
            incarnation: Uuid::new_v4(),
            epoch: NonZeroU64::MAX,
        };
        assert_eq!(exhausted.advance(), None);
        assert!(
            !SourceAuthority {
                incarnation: Uuid::nil(),
                ..exhausted
            }
            .is_valid()
        );
    }
}
