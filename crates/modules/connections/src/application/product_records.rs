//! Owner-private bounded observation traversal. No durable state or side effects.
use crate::{ConnectionsProductRepository, ConnectionsRecord};
use floe_execution::ExecutionScope;
use floe_kernel::{AgentFailure, OwnerActor};
use std::collections::VecDeque;
use uuid::Uuid;

pub(super) struct ProductRecordScan<'a> {
    repository: &'a dyn ConnectionsProductRepository,
    actor: &'a OwnerActor,
    scope: &'a ExecutionScope,
    after: Option<Uuid>,
    page: VecDeque<ConnectionsRecord>,
    complete: bool,
}
impl<'a> ProductRecordScan<'a> {
    pub(super) fn new(
        repository: &'a dyn ConnectionsProductRepository,
        actor: &'a OwnerActor,
        scope: &'a ExecutionScope,
    ) -> Self {
        Self {
            repository,
            actor,
            scope,
            after: None,
            page: VecDeque::new(),
            complete: false,
        }
    }
    pub(super) async fn next(&mut self) -> Result<Option<ConnectionsRecord>, AgentFailure> {
        super::source_operation::check(self.actor, self.scope)?;
        if let Some(record) = self.page.pop_front() {
            return Ok(Some(record));
        }
        if self.complete {
            return Ok(None);
        }
        const PAGE_SIZE: usize = 64;
        let page = self
            .repository
            .list_page(self.actor.person_id, self.after, PAGE_SIZE)
            .await?;
        super::source_operation::check(self.actor, self.scope)?;
        if page.records.len() > PAGE_SIZE {
            return Err(AgentFailure::StorageUnavailable);
        }
        let mut previous = self.after;
        for record in &page.records {
            record.validate()?;
            if record.person_id != self.actor.person_id
                || previous.is_some_and(|id| record.record_ref <= id)
            {
                return Err(AgentFailure::PolicyDenied);
            }
            previous = Some(record.record_ref);
        }
        if page.next_after.is_some() && (page.records.is_empty() || page.next_after != previous) {
            return Err(AgentFailure::StorageUnavailable);
        }
        self.after = page.next_after;
        self.complete = page.next_after.is_none();
        self.page = page.records.into();
        Ok(self.page.pop_front())
    }
}
