use floe_access::{GrantAuthority, GrantId, GrantState};
use floe_context_contract::{ConsumerPolicyAuthority, SourceAuthority};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConnectionObserveStatus {
    Active,
    Paused,
    NeedsReview,
    NeedsSystemAccess,
    ReconnectRequired,
    Unavailable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConnectionObserveMember {
    pub view_id: String,
    pub grant_id: GrantId,
    pub grant_authority: GrantAuthority,
    pub consumer_policy: ConsumerPolicyAuthority,
    pub source_authority: SourceAuthority,
    pub state: GrantState,
    pub review_required: bool,
    pub resources: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConnectionObserveOverview {
    pub connector_id: String,
    pub connection_id: String,
    pub status: ConnectionObserveStatus,
    pub enabled: bool,
    pub selected_resources: Vec<String>,
    pub granted_resources: Vec<String>,
    pub members: Vec<ConnectionObserveMember>,
}

impl ConnectionObserveOverview {
    pub(crate) fn from_members(
        connector_id: impl Into<String>,
        connection_id: impl Into<String>,
        mut selected_resources: Vec<String>,
        expected_views: &[&str],
        mut members: Vec<ConnectionObserveMember>,
    ) -> Self {
        selected_resources.sort();
        selected_resources.dedup();
        members.sort_by(|left, right| {
            left.view_id
                .cmp(&right.view_id)
                .then_with(|| left.grant_id.cmp(&right.grant_id))
        });
        let exact_members = members.len() == expected_views.len()
            && expected_views.iter().all(|expected| {
                members
                    .iter()
                    .filter(|member| member.view_id == *expected)
                    .count()
                    == 1
            });
        let all_active = exact_members
            && members.iter().all(|member| {
                member.state == GrantState::Active && !member.review_required
            });
        let all_paused = exact_members
            && members.iter().all(|member| {
                member.state == GrantState::Paused && !member.review_required
            });
        let status = if all_active {
            ConnectionObserveStatus::Active
        } else if all_paused {
            ConnectionObserveStatus::Paused
        } else {
            ConnectionObserveStatus::NeedsReview
        };
        let mut granted_resources = members
            .iter()
            .flat_map(|member| member.resources.iter().cloned())
            .collect::<Vec<_>>();
        granted_resources.sort();
        granted_resources.dedup();
        Self {
            connector_id: connector_id.into(),
            connection_id: connection_id.into(),
            status,
            enabled: all_active,
            selected_resources,
            granted_resources,
            members,
        }
    }

    pub(crate) fn from_calendar(value: crate::CalendarAccessOverview) -> Self {
        let members = match (
            value.grant_id,
            value.grant_authority,
            value.consumer_policy,
        ) {
            (Some(grant_id), Some(grant_authority), Some(consumer_policy)) => {
                vec![ConnectionObserveMember {
                    view_id: "calendar.timeline".into(),
                    grant_id,
                    grant_authority,
                    consumer_policy,
                    source_authority: value.source_authority,
                    state: match value.state {
                        crate::CalendarAccessState::Active => GrantState::Active,
                        crate::CalendarAccessState::Paused => GrantState::Paused,
                        crate::CalendarAccessState::Revoked => GrantState::Revoked,
                        crate::CalendarAccessState::NeedsReview => GrantState::Paused,
                    },
                    review_required: value.review_required
                        || value.state == crate::CalendarAccessState::NeedsReview,
                    resources: value.granted_resources.clone(),
                }]
            }
            _ => Vec::new(),
        };
        Self::from_members(
            floe_access::native_calendar_connector(value.provider)
                .expect("calendar access overview is native"),
            value.connection_id,
            value.selected_resources,
            &["calendar.timeline"],
            members,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use floe_context_contract::SourceAuthority;
    use uuid::Uuid;

    fn member(view: &str, state: GrantState) -> ConnectionObserveMember {
        ConnectionObserveMember {
            view_id: view.into(),
            grant_id: GrantId::new(),
            grant_authority: GrantAuthority::new(),
            consumer_policy: ConsumerPolicyAuthority::new(),
            source_authority: SourceAuthority::new(),
            state,
            review_required: false,
            resources: vec![format!("resource:{view}")],
        }
    }

    #[test]
    fn exact_bundle_is_active_or_paused() {
        let active = ConnectionObserveOverview::from_members(
            "gmail",
            Uuid::new_v4().to_string(),
            vec!["account".into()],
            &["mail.communication", "life.logistics"],
            vec![
                member("mail.communication", GrantState::Active),
                member("life.logistics", GrantState::Active),
            ],
        );
        assert_eq!(active.status, ConnectionObserveStatus::Active);
        assert!(active.enabled);
        let paused = ConnectionObserveOverview::from_members(
            "gmail",
            Uuid::new_v4().to_string(),
            vec!["account".into()],
            &["mail.communication", "life.logistics"],
            vec![
                member("mail.communication", GrantState::Paused),
                member("life.logistics", GrantState::Paused),
            ],
        );
        assert_eq!(paused.status, ConnectionObserveStatus::Paused);
        assert!(!paused.enabled);
    }

    #[test]
    fn missing_extra_duplicate_or_review_member_needs_review() {
        for members in [
            vec![member("mail.communication", GrantState::Active)],
            vec![
                member("mail.communication", GrantState::Active),
                member("mail.communication", GrantState::Active),
            ],
            vec![
                member("mail.communication", GrantState::Active),
                member("life.logistics", GrantState::Paused),
            ],
        ] {
            let overview = ConnectionObserveOverview::from_members(
                "gmail",
                Uuid::new_v4().to_string(),
                vec![],
                &["mail.communication", "life.logistics"],
                members,
            );
            assert_eq!(overview.status, ConnectionObserveStatus::NeedsReview);
            assert!(!overview.enabled);
        }
    }

    #[test]
    fn inspection_projection_has_no_authorization_state_of_its_own() {
        assert_eq!(std::mem::size_of::<ConnectionObserveStatus>(), 1);
        let overview = ConnectionObserveOverview::from_members(
            "gmail",
            Uuid::new_v4().to_string(),
            vec![],
            &["mail.communication"],
            vec![],
        );
        assert_eq!(overview.status, ConnectionObserveStatus::NeedsReview);
        assert!(overview.members.is_empty());
    }
}
