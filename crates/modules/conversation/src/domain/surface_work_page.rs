//! Bounded product recovery of admitted Work. Execution state remains on Run.
use crate::{MAX_SURFACE_PAGE_ITEMS, SurfaceWorkSnapshot};
use floe_kernel::{AgentFailure, CommandId};
use uuid::Uuid;

/// Stable admission watermark and exclusive older-page boundary. These are
/// canonical command identities, resolved inside the same read transaction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SurfaceWorkCursor {
    pub through: CommandId,
    pub before: CommandId,
}
impl SurfaceWorkCursor {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if !self.through.is_valid() || !self.before.is_valid() {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SurfaceWorkPageRequest {
    pub session_id: Uuid,
    pub cursor: Option<SurfaceWorkCursor>,
    pub limit: usize,
}
impl SurfaceWorkPageRequest {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.session_id.is_nil() || self.limit == 0 || self.limit > MAX_SURFACE_PAGE_ITEMS {
            return Err(AgentFailure::InvalidInput);
        }
        if let Some(cursor) = &self.cursor {
            cursor.validate()?;
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SurfaceWorkPage {
    /// Within each newest-first page, entries retain admission order.
    pub items: Vec<SurfaceWorkSnapshot>,
    pub next: Option<SurfaceWorkCursor>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn page_bound_is_not_a_lifetime_work_limit() {
        let mut request = SurfaceWorkPageRequest {
            session_id: Uuid::new_v4(),
            cursor: None,
            limit: MAX_SURFACE_PAGE_ITEMS,
        };
        request.validate().unwrap();
        request.limit = 0;
        assert_eq!(request.validate(), Err(AgentFailure::InvalidInput));
        request.limit = MAX_SURFACE_PAGE_ITEMS + 1;
        assert_eq!(request.validate(), Err(AgentFailure::InvalidInput));
        request.limit = 1;
        request.cursor = Some(SurfaceWorkCursor {
            through: CommandId::new(),
            before: CommandId::new(),
        });
        request.validate().unwrap();
        request.session_id = Uuid::nil();
        assert!(request.validate().is_err());
    }
}
