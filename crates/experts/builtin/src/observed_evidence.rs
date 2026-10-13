//! Index exact retained page observations without fabricating a combined view.
use floe_agent_contract::AgentFailure;
use floe_context_contract::{
    CalendarContextItem, CalendarContextView, CommunicationItem, CommunicationView,
    MAX_COMMUNICATION_BYTES, MAX_COMMUNICATION_ITEMS, validate_calendar_context_view,
    validate_communication_view,
};
use floe_experts::ExpertToolObservation;
use std::collections::BTreeMap;

pub(crate) struct ObservedItem<Item> {
    pub item: Item,
    pub source_handle: String,
    observed_at_unix_ms: i64,
}
pub(crate) struct ObservedEvidence<Item> {
    pub items: BTreeMap<String, ObservedItem<Item>>,
    pub source_handles: Vec<String>,
    pub expires_at_unix_ms: i64,
}
pub(crate) type MailEvidence = ObservedEvidence<CommunicationItem>;
pub(crate) type CalendarEvidence = ObservedEvidence<CalendarContextItem>;
impl<Item: Eq> ObservedEvidence<Item> {
    fn new() -> Self {
        Self {
            items: BTreeMap::new(),
            source_handles: vec![],
            expires_at_unix_ms: i64::MAX,
        }
    }
    fn observe_view(&mut self, source_handle: &str, expires_at: i64) {
        self.expires_at_unix_ms = self.expires_at_unix_ms.min(expires_at);
        if !self
            .source_handles
            .iter()
            .any(|source| source == source_handle)
        {
            self.source_handles.push(source_handle.to_owned());
            self.source_handles.sort();
        }
    }
    fn observe_item(
        &mut self,
        handle: String,
        item: Item,
        source_handle: &str,
        observed_at: i64,
    ) -> Result<(), AgentFailure> {
        let replace = match self.items.get(&handle) {
            None => true,
            Some(prior) if prior.observed_at_unix_ms == observed_at => {
                if prior.item != item {
                    return Err(AgentFailure::Conflict);
                }
                false
            }
            Some(prior) => observed_at > prior.observed_at_unix_ms,
        };
        if replace {
            self.items.insert(
                handle,
                ObservedItem {
                    item,
                    source_handle: source_handle.to_owned(),
                    observed_at_unix_ms: observed_at,
                },
            );
        }
        Ok(())
    }
}
pub(crate) fn read_mail(
    observations: &[ExpertToolObservation],
    key: &str,
    now: i64,
) -> Result<Option<MailEvidence>, AgentFailure> {
    let views = crate::program_support::read_pages::<CommunicationView>(observations, key)?;
    if views.is_empty() {
        return Ok(None);
    }
    let mut result = MailEvidence::new();
    for view in views {
        validate_communication_view(&view, now, MAX_COMMUNICATION_ITEMS, MAX_COMMUNICATION_BYTES)?;
        result.observe_view(&view.source_handle, view.expires_at_unix_ms);
        for item in view.items {
            result.observe_item(
                item.evidence_handle.clone(),
                item,
                &view.source_handle,
                view.observed_at_unix_ms,
            )?;
        }
    }
    Ok(Some(result))
}
pub(crate) fn read_calendar(
    observations: &[ExpertToolObservation],
    key: &str,
    now: i64,
) -> Result<Option<CalendarEvidence>, AgentFailure> {
    let pages = crate::program_support::read_pages::<Vec<CalendarContextView>>(observations, key)?;
    if pages.is_empty() {
        return Ok(None);
    }
    let mut result = CalendarEvidence::new();
    for view in pages.into_iter().flatten() {
        validate_calendar_context_view(&view, now)?;
        result.observe_view(&view.source_handle, view.expires_at_unix_ms);
        for item in view.items {
            result.observe_item(
                item.evidence_handle.clone(),
                item,
                &view.source_handle,
                view.observed_at_unix_ms,
            )?;
        }
    }
    Ok(Some(result))
}
