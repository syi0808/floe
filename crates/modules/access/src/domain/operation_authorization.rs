//! Access-owned authorization values for immutable external operations.
//!
//! Calendar Operations normalizes an effect and assigns its identity. Access
//! binds that identity to the exact effect, actor, source fence, policy
//! revision and expiry before it can be reviewed or dispatched.

use chrono::{DateTime, Utc};
use floe_kernel::{AgentFailure, PersonId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub type OperationDigest = [u8; 32];

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationPolicyMode {
    Allow,
    #[default]
    Ask,
    Deny,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OperationAuthorizationPolicy {
    pub person_id: PersonId,
    pub revision: u64,
    pub calendar_create: OperationPolicyMode,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OperationPolicyChange {
    pub command_id: Uuid,
    pub person_id: PersonId,
    pub expected_revision: u64,
    pub mode: OperationPolicyMode,
}

impl OperationAuthorizationPolicy {
    pub fn default_for(person_id: PersonId) -> Self {
        Self {
            person_id,
            revision: 1,
            calendar_create: OperationPolicyMode::Ask,
        }
    }

    pub fn validate(&self) -> Result<(), AgentFailure> {
        if !self.person_id.is_valid() || self.revision == 0 {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OperationSubject {
    pub operation_id: Uuid,
    pub effect_digest: OperationDigest,
    pub person_id: PersonId,
    pub device_id: String,
    pub source_digest: OperationDigest,
    /// Manual Day operations have no agent-policy revision.
    pub policy_revision: Option<u64>,
    pub expires_at: DateTime<Utc>,
}

impl OperationSubject {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.operation_id.is_nil()
            || self.effect_digest == [0; 32]
            || !self.person_id.is_valid()
            || self.device_id.is_empty()
            || self.device_id.len() > 256
            || self.device_id.trim() != self.device_id
            || self.device_id.chars().any(char::is_control)
            || self.source_digest == [0; 32]
            || self.policy_revision.is_some_and(|revision| revision == 0)
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }

    pub fn digest(&self) -> Result<OperationDigest, AgentFailure> {
        self.validate()?;
        let bytes = serde_json::to_vec(self).map_err(|_| AgentFailure::InvalidInput)?;
        if bytes.len() > 4096 {
            return Err(AgentFailure::BudgetExceeded);
        }
        let mut digest = Sha256::new();
        digest.update(b"floe.access.operation-subject.v1\0");
        digest.update(bytes);
        Ok(digest.finalize().into())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OperationApprovalRef {
    pub id: Uuid,
    pub operation_id: Uuid,
    pub effect_digest: OperationDigest,
    pub source_digest: OperationDigest,
    pub person_id: PersonId,
    pub device_id: String,
    pub policy_revision: Option<u64>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

impl OperationApprovalRef {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.subject().validate()?;
        if self.id.is_nil() || self.created_at >= self.expires_at {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }

    pub fn subject(&self) -> OperationSubject {
        OperationSubject {
            operation_id: self.operation_id,
            effect_digest: self.effect_digest,
            person_id: self.person_id,
            device_id: self.device_id.clone(),
            source_digest: self.source_digest,
            policy_revision: self.policy_revision,
            expires_at: self.expires_at,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationDecisionKind {
    Approve,
    Reject,
    Cancel,
}

/// An Access receipt that is consumed by the Calendar Operations dispatch CAS.
/// Its digest pins all subject fields, while the variant explains which owner
/// authority admitted the decision.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum OperationDecisionReceipt {
    DirectInstruction {
        command_id: Uuid,
        subject_digest: OperationDigest,
    },
    StandingPolicy {
        subject_digest: OperationDigest,
        policy_revision: u64,
    },
    ReviewedDecision {
        command_id: Uuid,
        approval_id: Uuid,
        subject_digest: OperationDigest,
        policy_revision: u64,
        person_id: PersonId,
        device_id: String,
        decision: OperationDecisionKind,
        decided_at: DateTime<Utc>,
    },
}

impl OperationDecisionReceipt {
    pub fn policy_revision(&self) -> Option<u64> {
        match self {
            Self::DirectInstruction { .. } => None,
            Self::StandingPolicy {
                policy_revision, ..
            }
            | Self::ReviewedDecision {
                policy_revision, ..
            } => Some(*policy_revision),
        }
    }

    /// Authenticate a Conversation resolution receipt against the exact
    /// immutable Access review. This is a projection check only; dispatch
    /// still consumes the receipt in the owner transaction.
    pub fn reviewed_decision_command_for(
        &self,
        review: &OperationApprovalRef,
    ) -> Result<Uuid, AgentFailure> {
        review.validate()?;
        let Self::ReviewedDecision {
            command_id,
            approval_id,
            subject_digest,
            policy_revision,
            person_id,
            device_id,
            decided_at,
            ..
        } = self
        else {
            return Err(AgentFailure::Conflict);
        };
        if command_id.is_nil()
            || *approval_id != review.id
            || *subject_digest != review.subject().digest()?
            || Some(*policy_revision) != review.policy_revision
            || *person_id != review.person_id
            || *device_id != review.device_id
            || *decided_at < review.created_at
            || *decided_at >= review.expires_at
        {
            return Err(AgentFailure::Conflict);
        }
        Ok(*command_id)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationPolicyDecision {
    Allow,
    Ask,
    Deny,
}

pub fn change_operation_policy(
    current: &OperationAuthorizationPolicy,
    change: &OperationPolicyChange,
) -> Result<OperationAuthorizationPolicy, AgentFailure> {
    current.validate()?;
    if change.command_id.is_nil()
        || change.person_id != current.person_id
        || change.expected_revision != current.revision
    {
        return Err(AgentFailure::Conflict);
    }
    if change.mode == current.calendar_create {
        return Ok(current.clone());
    }
    Ok(OperationAuthorizationPolicy {
        person_id: current.person_id,
        revision: current
            .revision
            .checked_add(1)
            .ok_or(AgentFailure::Conflict)?,
        calendar_create: change.mode,
    })
}

pub fn evaluate_operation_policy(
    subject: &OperationSubject,
    policy: &OperationAuthorizationPolicy,
) -> Result<OperationPolicyDecision, AgentFailure> {
    subject.validate()?;
    policy.validate()?;
    if subject.person_id != policy.person_id || subject.policy_revision != Some(policy.revision) {
        return Err(AgentFailure::Conflict);
    }
    Ok(match policy.calendar_create {
        OperationPolicyMode::Allow => OperationPolicyDecision::Allow,
        OperationPolicyMode::Ask => OperationPolicyDecision::Ask,
        OperationPolicyMode::Deny => OperationPolicyDecision::Deny,
    })
}

pub fn direct_operation_receipt(
    command_id: Uuid,
    subject: &OperationSubject,
) -> Result<OperationDecisionReceipt, AgentFailure> {
    subject.validate()?;
    if command_id.is_nil() || subject.policy_revision.is_some() {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(OperationDecisionReceipt::DirectInstruction {
        command_id,
        subject_digest: subject.digest()?,
    })
}

pub fn standing_policy_receipt(
    subject: &OperationSubject,
    policy: &OperationAuthorizationPolicy,
) -> Result<OperationDecisionReceipt, AgentFailure> {
    if evaluate_operation_policy(subject, policy)? != OperationPolicyDecision::Allow {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(OperationDecisionReceipt::StandingPolicy {
        subject_digest: subject.digest()?,
        policy_revision: policy.revision,
    })
}

pub fn reviewed_operation_receipt(
    command_id: Uuid,
    approval: &OperationApprovalRef,
    policy: &OperationAuthorizationPolicy,
    decision: OperationDecisionKind,
    decided_at: DateTime<Utc>,
) -> Result<OperationDecisionReceipt, AgentFailure> {
    approval.validate()?;
    let subject = approval.subject();
    if command_id.is_nil()
        || evaluate_operation_policy(&subject, policy)? == OperationPolicyDecision::Deny
        || decided_at < approval.created_at
        || decided_at >= subject.expires_at
    {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(OperationDecisionReceipt::ReviewedDecision {
        command_id,
        approval_id: approval.id,
        subject_digest: subject.digest()?,
        policy_revision: policy.revision,
        person_id: subject.person_id,
        device_id: subject.device_id.clone(),
        decision,
        decided_at,
    })
}

pub fn validate_operation_receipt(
    subject: &OperationSubject,
    approval: Option<&OperationApprovalRef>,
    receipt: &OperationDecisionReceipt,
    expected_direct_command: Option<Uuid>,
    current_policy: Option<&OperationAuthorizationPolicy>,
    now: DateTime<Utc>,
) -> Result<(), AgentFailure> {
    subject.validate()?;
    if now >= subject.expires_at {
        return Err(AgentFailure::PolicyDenied);
    }
    validate_operation_receipt_binding(subject, approval, receipt, expected_direct_command)?;
    match receipt {
        OperationDecisionReceipt::DirectInstruction { .. } => Ok(()),
        OperationDecisionReceipt::StandingPolicy {
            policy_revision, ..
        } => {
            let policy = current_policy.ok_or(AgentFailure::PolicyDenied)?;
            if Some(*policy_revision) != subject.policy_revision
                || policy.person_id != subject.person_id
                || policy.revision != *policy_revision
                || policy.calendar_create != OperationPolicyMode::Allow
            {
                return Err(AgentFailure::PolicyDenied);
            }
            Ok(())
        }
        OperationDecisionReceipt::ReviewedDecision { .. } => {
            let policy = current_policy.ok_or(AgentFailure::PolicyDenied)?;
            if policy.person_id != subject.person_id
                || Some(policy.revision) != subject.policy_revision
                || policy.calendar_create == OperationPolicyMode::Deny
            {
                return Err(AgentFailure::PolicyDenied);
            }
            Ok(())
        }
    }
}

pub fn validate_operation_receipt_binding(
    subject: &OperationSubject,
    approval: Option<&OperationApprovalRef>,
    receipt: &OperationDecisionReceipt,
    expected_direct_command: Option<Uuid>,
) -> Result<(), AgentFailure> {
    subject.validate()?;
    let subject_digest = subject.digest()?;
    match receipt {
        OperationDecisionReceipt::DirectInstruction {
            command_id,
            subject_digest: receipt_digest,
        } if subject.policy_revision.is_none()
            && Some(*command_id) == expected_direct_command
            && *receipt_digest == subject_digest =>
        {
            Ok(())
        }
        OperationDecisionReceipt::StandingPolicy {
            subject_digest: receipt_digest,
            policy_revision,
        } => {
            if *receipt_digest != subject_digest
                || Some(*policy_revision) != subject.policy_revision
            {
                return Err(AgentFailure::PolicyDenied);
            }
            Ok(())
        }
        OperationDecisionReceipt::ReviewedDecision {
            command_id,
            approval_id,
            subject_digest: receipt_digest,
            person_id,
            device_id,
            policy_revision,
            decision: _,
            decided_at,
        } => {
            let approval = approval.ok_or(AgentFailure::PolicyDenied)?;
            approval.validate()?;
            let approval_subject = approval.subject();
            if approval.id != *approval_id
                || approval_subject != *subject
                || *receipt_digest != subject_digest
                || Some(*policy_revision) != subject.policy_revision
                || command_id.is_nil()
                || *person_id != subject.person_id
                || device_id != &subject.device_id
                || *decided_at < approval.created_at
                || *decided_at >= subject.expires_at
            {
                return Err(AgentFailure::PolicyDenied);
            }
            Ok(())
        }
        _ => Err(AgentFailure::PolicyDenied),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn approval(policy_revision: Option<u64>) -> OperationApprovalRef {
        let now = Utc::now();
        OperationApprovalRef {
            id: Uuid::new_v4(),
            operation_id: Uuid::new_v4(),
            effect_digest: [7; 32],
            source_digest: [8; 32],
            person_id: PersonId::new(),
            device_id: "device-a".into(),
            policy_revision,
            created_at: now,
            expires_at: now + Duration::minutes(5),
        }
    }

    fn policy(
        subject: &OperationSubject,
        mode: OperationPolicyMode,
    ) -> OperationAuthorizationPolicy {
        OperationAuthorizationPolicy {
            person_id: subject.person_id,
            revision: subject.policy_revision.unwrap_or(1),
            calendar_create: mode,
        }
    }

    #[test]
    fn operation_policy_distinguishes_allow_ask_deny_and_rejects_stale_revision() {
        let review = approval(Some(4));
        let subject = review.subject();
        for (mode, expected) in [
            (OperationPolicyMode::Allow, OperationPolicyDecision::Allow),
            (OperationPolicyMode::Ask, OperationPolicyDecision::Ask),
            (OperationPolicyMode::Deny, OperationPolicyDecision::Deny),
        ] {
            assert_eq!(
                evaluate_operation_policy(&subject, &policy(&subject, mode)).unwrap(),
                expected
            );
        }
        let stale = OperationAuthorizationPolicy {
            person_id: subject.person_id,
            revision: 3,
            calendar_create: OperationPolicyMode::Allow,
        };
        assert_eq!(
            evaluate_operation_policy(&subject, &stale),
            Err(AgentFailure::Conflict)
        );
    }

    #[test]
    fn manual_operation_receipt_is_independent_of_agent_policy() {
        let review = approval(None);
        let receipt = direct_operation_receipt(Uuid::new_v4(), &review.subject()).unwrap();
        assert_eq!(receipt.policy_revision(), None);
        let expected_direct_command = match &receipt {
            OperationDecisionReceipt::DirectInstruction { command_id, .. } => Some(*command_id),
            _ => None,
        };
        assert!(
            validate_operation_receipt(
                &review.subject(),
                Some(&review),
                &receipt,
                expected_direct_command,
                None,
                Utc::now(),
            )
            .is_ok()
        );
    }

    #[test]
    fn reviewed_receipt_is_bound_to_exact_operation_actor_policy_and_expiry() {
        let review = approval(Some(4));
        let policy = policy(&review.subject(), OperationPolicyMode::Ask);
        let command_id = Uuid::new_v4();
        let receipt = reviewed_operation_receipt(
            command_id,
            &review,
            &policy,
            OperationDecisionKind::Approve,
            Utc::now(),
        )
        .unwrap();
        assert!(
            validate_operation_receipt_binding(&review.subject(), Some(&review), &receipt, None,)
                .is_ok()
        );

        let mut changed_effect = review.subject();
        changed_effect.effect_digest = [9; 32];
        assert_eq!(
            validate_operation_receipt_binding(&changed_effect, Some(&review), &receipt, None,),
            Err(AgentFailure::PolicyDenied)
        );

        let mut changed_actor = review.clone();
        changed_actor.device_id = "other-device".into();
        assert_eq!(
            validate_operation_receipt_binding(
                &changed_actor.subject(),
                Some(&changed_actor),
                &receipt,
                None,
            ),
            Err(AgentFailure::PolicyDenied)
        );

        let changed_policy = OperationAuthorizationPolicy {
            person_id: review.person_id,
            revision: 5,
            calendar_create: OperationPolicyMode::Ask,
        };
        assert_eq!(
            validate_operation_receipt(
                &review.subject(),
                Some(&review),
                &receipt,
                None,
                Some(&changed_policy),
                Utc::now(),
            ),
            Err(AgentFailure::PolicyDenied)
        );
    }
}
