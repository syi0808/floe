use floe_access::{GrantAuthority, GrantId, GrantState};
use floe_context_contract::SourceAuthority;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConnectionObserveReviewedMember {
    pub view_id: String,
    pub policy_digest: String,
    pub resource: String,
    pub expected_grant_id: Option<GrantId>,
    pub expected_grant_authority: Option<GrantAuthority>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConnectionObserveExpectation {
    pub connector_id: String,
    pub connection_id: String,
    pub source_authority: SourceAuthority,
    pub connection_revision: Option<u64>,
    pub native_subject: Option<String>,
    pub producer_fingerprint: Option<String>,
    pub members: Vec<ConnectionObserveReviewedMember>,
}

impl ConnectionObserveExpectation {
    pub fn validate(&self) -> Result<(), crate::AgentFailure> {
        use crate::AgentFailure;

        floe_context_contract::ConnectorId::try_new(&self.connector_id)
            .map_err(|_| AgentFailure::InvalidInput)?;
        let connection = floe_context_contract::ConnectionId::try_new(&self.connection_id)
            .map_err(|_| AgentFailure::InvalidInput)?;
        if !self.source_authority.is_valid()
            || self
                .connection_revision
                .is_some_and(|revision| revision == 0)
            || self.native_subject.is_some() == self.producer_fingerprint.is_some()
            || self.members.is_empty()
            || self.members.len() > 8
        {
            return Err(AgentFailure::InvalidInput);
        }
        for value in [
            self.native_subject.as_deref(),
            self.producer_fingerprint.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            if value.is_empty()
                || value.len() > 256
                || value.trim() != value
                || value.chars().any(char::is_control)
            {
                return Err(AgentFailure::InvalidInput);
            }
        }
        let mut previous_view = None;
        for member in &self.members {
            if member.view_id.is_empty()
                || member.view_id.len() > 128
                || member.view_id.trim() != member.view_id
                || member.view_id.chars().any(char::is_control)
                || previous_view.is_some_and(|previous| previous >= member.view_id.as_str())
                || member.policy_digest.len() != 64
                || !member
                    .policy_digest
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                || floe_context_contract::connection_view_resource(&member.view_id, &connection)
                    .map_or(true, |resource| resource.as_str() != member.resource)
            {
                return Err(AgentFailure::InvalidInput);
            }
            match (member.expected_grant_id, member.expected_grant_authority) {
                (None, None) => {}
                (Some(id), Some(authority)) if id.is_valid() && authority.is_valid() => {}
                _ => return Err(AgentFailure::InvalidInput),
            }
            previous_view = Some(member.view_id.as_str());
        }
        Ok(())
    }
}

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
    pub state: GrantState,
    pub review_required: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConnectionObserveOverview {
    pub connector_id: String,
    pub connection_id: String,
    pub status: ConnectionObserveStatus,
    pub enabled: bool,
    pub source_resources: Vec<String>,
    pub members: Vec<ConnectionObserveMember>,
}

impl ConnectionObserveOverview {
    pub(crate) fn from_members(
        connector_id: impl Into<String>,
        connection_id: impl Into<String>,
        mut source_resources: Vec<String>,
        expected_views: &[&str],
        mut members: Vec<ConnectionObserveMember>,
    ) -> Self {
        source_resources.sort();
        source_resources.dedup();
        members.sort_by(|left, right| left.view_id.cmp(&right.view_id));
        let exact_members = members.len() == expected_views.len()
            && expected_views.iter().all(|expected| {
                members
                    .iter()
                    .filter(|member| member.view_id == *expected)
                    .count()
                    == 1
            });
        let all_active = exact_members
            && members
                .iter()
                .all(|member| member.state == GrantState::Active && !member.review_required);
        let all_paused = exact_members
            && members
                .iter()
                .all(|member| member.state == GrantState::Paused && !member.review_required);
        let status = if all_active {
            ConnectionObserveStatus::Active
        } else if all_paused {
            ConnectionObserveStatus::Paused
        } else {
            ConnectionObserveStatus::NeedsReview
        };
        Self {
            connector_id: connector_id.into(),
            connection_id: connection_id.into(),
            status,
            enabled: all_active,
            source_resources,
            members,
        }
    }

    pub(crate) fn from_calendar(value: crate::CalendarAccessOverview) -> Self {
        let members = match (value.grant_id, value.grant_authority) {
            (Some(_), Some(_)) => {
                vec![ConnectionObserveMember {
                    view_id: "calendar.timeline".into(),
                    state: match value.state {
                        crate::CalendarAccessState::Active => GrantState::Active,
                        crate::CalendarAccessState::Paused => GrantState::Paused,
                        crate::CalendarAccessState::Revoked => GrantState::Revoked,
                        crate::CalendarAccessState::NeedsReview => GrantState::Paused,
                    },
                    review_required: value.review_required
                        || value.state == crate::CalendarAccessState::NeedsReview,
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

    fn expectation() -> ConnectionObserveExpectation {
        let connection_id = Uuid::new_v4().to_string();
        let connection = floe_context_contract::ConnectionId::try_new(&connection_id).unwrap();
        ConnectionObserveExpectation {
            connector_id: "calendar.google".into(),
            connection_id,
            source_authority: SourceAuthority::new(),
            connection_revision: Some(1),
            native_subject: None,
            producer_fingerprint: Some("producer".into()),
            members: vec![ConnectionObserveReviewedMember {
                view_id: "calendar.timeline".into(),
                policy_digest: "a".repeat(64),
                resource: floe_context_contract::connection_view_resource(
                    "calendar.timeline",
                    &connection,
                )
                .unwrap()
                .as_str()
                .into(),
                expected_grant_id: None,
                expected_grant_authority: None,
            }],
        }
    }

    #[test]
    fn reviewed_expectation_requires_canonical_logical_members() {
        let reviewed = expectation();
        assert_eq!(reviewed.validate(), Ok(()));

        let mut duplicate = reviewed.clone();
        duplicate.members.push(duplicate.members[0].clone());
        assert_eq!(duplicate.validate(), Err(crate::AgentFailure::InvalidInput));

        let mut leaf = reviewed.clone();
        leaf.members[0].resource = "calendar-id".into();
        assert_eq!(leaf.validate(), Err(crate::AgentFailure::InvalidInput));

        let mut incomplete_grant = reviewed.clone();
        incomplete_grant.members[0].expected_grant_id = Some(GrantId::new());
        assert_eq!(
            incomplete_grant.validate(),
            Err(crate::AgentFailure::InvalidInput)
        );

        let mut mixed_source = reviewed;
        mixed_source.native_subject = Some("native-subject".into());
        assert_eq!(
            mixed_source.validate(),
            Err(crate::AgentFailure::InvalidInput)
        );
    }

    fn member(view: &str, state: GrantState) -> ConnectionObserveMember {
        ConnectionObserveMember {
            view_id: view.into(),
            state,
            review_required: false,
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
    fn source_resources_do_not_change_permission_members() {
        let connection_id = Uuid::new_v4().to_string();
        let members = vec![member("calendar.timeline", GrantState::Active)];
        let first = ConnectionObserveOverview::from_members(
            "calendar.google",
            &connection_id,
            vec!["calendar-a".into()],
            &["calendar.timeline"],
            members.clone(),
        );
        let changed = ConnectionObserveOverview::from_members(
            "calendar.google",
            connection_id,
            vec!["calendar-b".into(), "calendar-a".into()],
            &["calendar.timeline"],
            members,
        );
        assert_eq!(first.members, changed.members);
        assert_eq!(first.status, changed.status);
        assert_ne!(first.source_resources, changed.source_resources);
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
