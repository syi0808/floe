pub use floe_knowledge::{
    LoadedPlaybook, MAX_LOADED_PLAYBOOK_BYTES, MAX_LOADED_PLAYBOOKS, MAX_PLAYBOOK_DEPTH,
    MAX_VISIBLE_PLAYBOOKS, Playbook, PlaybookAudience, PlaybookBody, PlaybookChild,
    PlaybookIndexEntry, PlaybookRef, PlaybookRegistry, PlaybookSession,
};

impl From<crate::PromptRole> for floe_knowledge::PlaybookAudience {
    fn from(role: crate::PromptRole) -> Self {
        Self(
            match role {
                crate::PromptRole::Manager => "manager",
                crate::PromptRole::ScheduleExpert => "schedule_expert",
                crate::PromptRole::CommitmentsExpert => "commitments_expert",
                crate::PromptRole::CommunicationExpert => "communication_expert",
                crate::PromptRole::RelationshipsExpert => "relationships_expert",
                crate::PromptRole::FocusAttentionExpert => "focus_attention_expert",
                crate::PromptRole::WellbeingExpert => "wellbeing_expert",
                crate::PromptRole::WorkContextExpert => "work_context_expert",
                crate::PromptRole::LifeLogisticsExpert => "life_logistics_expert",
                crate::PromptRole::Learner => "learner",
            }
            .into(),
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::PromptRole;

    use super::*;

    #[test]
    fn playbook_audience_preserves_prompt_role_wire_names() {
        let roles = [
            (PromptRole::Manager, "manager"),
            (PromptRole::ScheduleExpert, "schedule_expert"),
            (PromptRole::CommitmentsExpert, "commitments_expert"),
            (PromptRole::CommunicationExpert, "communication_expert"),
            (PromptRole::RelationshipsExpert, "relationships_expert"),
            (PromptRole::FocusAttentionExpert, "focus_attention_expert"),
            (PromptRole::WellbeingExpert, "wellbeing_expert"),
            (PromptRole::WorkContextExpert, "work_context_expert"),
            (PromptRole::LifeLogisticsExpert, "life_logistics_expert"),
            (PromptRole::Learner, "learner"),
        ];
        for (role, expected) in roles {
            assert_eq!(
                serde_json::to_value(role).unwrap(),
                serde_json::to_value(PlaybookAudience::from(role)).unwrap()
            );
            assert_eq!(
                serde_json::to_value(PlaybookAudience::from(role)).unwrap(),
                expected
            );
        }
    }
}
