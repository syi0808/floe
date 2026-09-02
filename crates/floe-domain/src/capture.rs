use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{CaptureId, DomainError, EventId, NoteId, PersonId, Revision, TaskId};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum CaptureSource {
    Typed,
    Voice,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum DomainRef {
    Event(EventId),
    Task(TaskId),
    Note(NoteId),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum CaptureProcessing {
    Pending,
    Classified {
        target: DomainRef,
        classified_at: DateTime<Utc>,
    },
    Dismissed {
        dismissed_at: DateTime<Utc>,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Capture {
    pub id: CaptureId,
    pub person_id: PersonId,
    pub original_input: String,
    pub captured_at: DateTime<Utc>,
    pub source: CaptureSource,
    pub processing: CaptureProcessing,
    pub revision: Revision,
}

impl Capture {
    pub fn new(
        person_id: PersonId,
        original_input: impl Into<String>,
        captured_at: DateTime<Utc>,
        source: CaptureSource,
    ) -> Result<Self, DomainError> {
        let original_input = original_input.into().trim().to_owned();
        if original_input.is_empty() {
            return Err(DomainError::EmptyText {
                field: "original_input",
            });
        }
        Ok(Self {
            id: CaptureId::new(),
            person_id,
            original_input,
            captured_at,
            source,
            processing: CaptureProcessing::Pending,
            revision: Revision::default(),
        })
    }
    pub fn classify(&mut self, target: DomainRef, classified_at: DateTime<Utc>) {
        self.processing = CaptureProcessing::Classified {
            target,
            classified_at,
        };
        self.revision = self.revision.next();
    }
}
