use chrono::{DateTime,Utc};
use floe_agent_contract::TaskExecutionReceiptRef;
use floe_kernel::AgentFailure;
use serde::{Deserialize,Serialize};
use uuid::Uuid;

use super::record::*;

#[derive(Clone,Debug,Deserialize,Eq,PartialEq,Serialize)]
#[serde(tag="kind",rename_all="snake_case",deny_unknown_fields)]
pub enum ActionIntent {
    Direct{effect:CalendarEffect},
    ExpertProposal{receipt:TaskExecutionReceiptRef,artifact_id:Uuid,destination:CalendarDestination,timezone:String},
}

#[derive(Clone,Copy,Debug,Deserialize,Eq,PartialEq,Serialize)]
#[serde(rename_all="snake_case")]
pub enum ActionAllowedAction {Approve,Reject,Cancel,Reconcile}

#[derive(Clone,Debug,Deserialize,Eq,PartialEq,Serialize)]
#[serde(tag="state",rename_all="snake_case",deny_unknown_fields)]
pub enum ActionStatus {
    PendingReview,Approved,Rejected,Cancelled,Expired,Executing,
    Blocked{reason:ActionBlockedReason},Failed{reason:ActionNotAppliedReason},Unknown{reason:ActionUnknownReason},
    Succeeded{external_id:String,collection:ActionCollectionStatus},
}

#[derive(Clone,Copy,Debug,Deserialize,Eq,PartialEq,Serialize)]
#[serde(rename_all="snake_case")]
pub enum ActionCollectionStatus {Pending,Collected}

#[derive(Clone,Copy,Debug,Deserialize,Eq,PartialEq,Serialize)]
#[serde(rename_all="snake_case")]
pub enum ActionOriginKind {Direct,Expert}

#[derive(Clone,Debug,Deserialize,Eq,PartialEq,Serialize)]
#[serde(deny_unknown_fields)]
pub struct ActionSnapshot {
    pub action_ref:Uuid,pub revision:u64,pub origin:ActionOriginKind,pub effect:CalendarEffect,
    pub review_ref:ActionReviewRef,pub created_at:DateTime<Utc>,pub expires_at:DateTime<Utc>,
    pub status:ActionStatus,pub allowed_actions:Vec<ActionAllowedAction>,pub next_observation_after_ms:Option<u64>,
}

impl ActionSnapshot {
    pub fn from_record(record:&ActionRecord,now:DateTime<Utc>)->Result<Self,AgentFailure>{
        record.validate()?;
        let mut allowed_actions=Vec::new();
        let status=match &record.state {
            ActionState::PendingReview=>{
                if now>=record.created_at && now<record.expires_at {allowed_actions.extend([ActionAllowedAction::Approve,ActionAllowedAction::Reject,ActionAllowedAction::Cancel]);}
                ActionStatus::PendingReview
            },
            ActionState::Approved=>{if now<record.expires_at {allowed_actions.push(ActionAllowedAction::Cancel);} ActionStatus::Approved},
            ActionState::Rejected=>ActionStatus::Rejected,
            ActionState::Cancelled=>ActionStatus::Cancelled,
            ActionState::Expired=>ActionStatus::Expired,
            ActionState::Executing{..}=>{allowed_actions.push(ActionAllowedAction::Reconcile);ActionStatus::Executing},
            ActionState::Blocked{reason}=>ActionStatus::Blocked{reason:*reason},
            ActionState::Failed{reason,..}=>ActionStatus::Failed{reason:*reason},
            ActionState::Unknown{reason}=>{allowed_actions.push(ActionAllowedAction::Reconcile);ActionStatus::Unknown{reason:*reason}},
            ActionState::Succeeded{receipt,collection}=>{
                let external_id=match &receipt.effect {CommittedCalendarEffect::Created{event}|CommittedCalendarEffect::Updated{event,..}=>event.external_id.clone(),
                    CommittedCalendarEffect::Deleted{target}=>target.source()?.external_id.clone()};
                let collection=match collection {ActionCollectionState::Pending{..}=>{allowed_actions.push(ActionAllowedAction::Reconcile);ActionCollectionStatus::Pending},
                    ActionCollectionState::Collected{..}=>ActionCollectionStatus::Collected};
                ActionStatus::Succeeded{external_id,collection}
            },
        };
        Ok(Self{action_ref:record.id,revision:record.revision,origin:match record.origin {ActionOrigin::Direct{..}=>ActionOriginKind::Direct,ActionOrigin::Expert{..}=>ActionOriginKind::Expert},
            effect:record.effect.clone(),review_ref:record.review.clone(),created_at:record.created_at,expires_at:record.expires_at,status,allowed_actions,
            next_observation_after_ms:matches!(record.state,ActionState::Approved|ActionState::Executing{..}).then_some(250)})
    }
}

#[derive(Clone,Debug,Deserialize,Eq,PartialEq,Serialize)]
#[serde(deny_unknown_fields)]
pub struct ActionsPage {pub actions:Vec<ActionSnapshot>,pub next_cursor:Option<Uuid>}
