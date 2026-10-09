//! Bounded read-only Day evidence needed to construct Context-owned views.

use floe_context_contract::{
    MAX_NATIVE_CONTEXT_BYTES, MAX_NATIVE_CONTEXT_ITEMS, TaskContextPriority,
};
use floe_day::MAX_DAY_SNAPSHOT_BYTES;
use floe_execution::BoxFuture;
use floe_kernel::{AgentFailure, PersonId};
use uuid::Uuid;

pub const MAX_DAY_EVIDENCE_ACQUISITION_PAYLOAD_BYTES: usize = MAX_DAY_SNAPSHOT_BYTES;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DayContextEvidenceSelection {
    OpenTasks,
    CurrentNotes,
}

/// Budget for the serialized, caller-visible NativeContextView.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeContextProjectionBudget {
    max_items: usize,
    max_serialized_view_bytes: usize,
}

impl NativeContextProjectionBudget {
    pub fn try_new(
        max_items: usize,
        max_serialized_view_bytes: usize,
    ) -> Result<Self, AgentFailure> {
        let max_items = max_items.min(MAX_NATIVE_CONTEXT_ITEMS);
        let max_serialized_view_bytes = max_serialized_view_bytes.min(MAX_NATIVE_CONTEXT_BYTES);
        if max_items == 0 || max_serialized_view_bytes == 0 {
            return Err(AgentFailure::BudgetExceeded);
        }
        Ok(Self {
            max_items,
            max_serialized_view_bytes,
        })
    }

    pub fn max_items(self) -> usize {
        self.max_items
    }

    pub fn max_serialized_view_bytes(self) -> usize {
        self.max_serialized_view_bytes
    }
}

/// Finite storage-side bounds for acquiring exact selected Day evidence.
/// Payload bytes count the stored JSON strings before serde decoding. The
/// projected Day item ceiling preserves Day's prior 4 MiB read allowance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DayEvidenceAcquisitionBudget {
    max_candidate_items: usize,
    max_serialized_payload_bytes: usize,
    max_projected_day_item_bytes: usize,
}

impl DayEvidenceAcquisitionBudget {
    fn for_context_view(max_items: usize) -> Self {
        Self {
            max_candidate_items: max_items,
            max_serialized_payload_bytes: MAX_DAY_EVIDENCE_ACQUISITION_PAYLOAD_BYTES,
            max_projected_day_item_bytes: MAX_DAY_SNAPSHOT_BYTES,
        }
    }

    pub fn max_candidate_items(self) -> usize {
        self.max_candidate_items
    }

    pub fn max_serialized_payload_bytes(self) -> usize {
        self.max_serialized_payload_bytes
    }

    pub fn max_projected_day_item_bytes(self) -> usize {
        self.max_projected_day_item_bytes
    }
}

/// The projection and acquisition budgets are distinct because they measure
/// different serialized shapes and are enforced at different owner boundaries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DayContextEvidenceQuery {
    person_id: PersonId,
    selection: DayContextEvidenceSelection,
    projection: NativeContextProjectionBudget,
    acquisition: DayEvidenceAcquisitionBudget,
}

impl DayContextEvidenceQuery {
    pub fn try_new(
        person_id: PersonId,
        selection: DayContextEvidenceSelection,
        max_items: usize,
        max_native_view_bytes: usize,
    ) -> Result<Self, AgentFailure> {
        if !person_id.is_valid() {
            return Err(AgentFailure::BudgetExceeded);
        }
        let projection = NativeContextProjectionBudget::try_new(max_items, max_native_view_bytes)?;
        Ok(Self {
            person_id,
            selection,
            projection,
            acquisition: DayEvidenceAcquisitionBudget::for_context_view(projection.max_items()),
        })
    }

    pub fn person_id(&self) -> PersonId {
        self.person_id
    }

    pub fn selection(&self) -> DayContextEvidenceSelection {
        self.selection
    }

    pub fn projection_budget(&self) -> NativeContextProjectionBudget {
        self.projection
    }

    pub fn acquisition_budget(&self) -> DayEvidenceAcquisitionBudget {
        self.acquisition
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DayTaskEvidence {
    pub id: Uuid,
    pub person_id: PersonId,
    pub title: String,
    pub deadline: Option<chrono::DateTime<chrono::Utc>>,
    pub priority: TaskContextPriority,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub deleted_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DayNoteEvidence {
    pub id: Uuid,
    pub person_id: PersonId,
    pub content: String,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub deleted_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DayContextEvidence {
    Task(DayTaskEvidence),
    Note(DayNoteEvidence),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DayContextEvidenceError {
    BudgetExceeded,
    StorageUnavailable,
}

impl From<DayContextEvidenceError> for AgentFailure {
    fn from(error: DayContextEvidenceError) -> Self {
        match error {
            DayContextEvidenceError::BudgetExceeded => Self::BudgetExceeded,
            DayContextEvidenceError::StorageUnavailable => Self::StorageUnavailable,
        }
    }
}

pub trait DayContextEvidenceReader: Send + Sync {
    fn read<'a>(
        &'a self,
        query: DayContextEvidenceQuery,
    ) -> BoxFuture<'a, Result<Vec<DayContextEvidence>, DayContextEvidenceError>>;
}
