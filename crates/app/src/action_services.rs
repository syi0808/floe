//! Mechanical host forwarding to the admitted Actions owner.
use std::time::Duration;

use floe_execution::Cancellation;
use uuid::Uuid;

use crate::{AgentFailure, AppComposition, CallerContext};
pub use floe_actions::{
    ActionAuthorityMode, ActionDecisionKind, ActionDestinationChoice, ActionIntent,
    ActionReviewRef, ActionSnapshot, ActionsAuthority, ActionsPage,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActionsCommand {
    Submit {
        intent: ActionIntent,
    },
    Decide {
        action_ref: Uuid,
        review_ref: ActionReviewRef,
        decision: ActionDecisionKind,
        expected_revision: u64,
    },
    Reconcile {
        action_ref: Uuid,
        expected_revision: u64,
    },
    SetAuthority {
        mode: ActionAuthorityMode,
        expected_revision: u64,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActionsQuery {
    Destinations,
    Authority,
    Inspect {
        action_ref: Uuid,
    },
    List {
        cursor: Option<Uuid>,
        limit: u16,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActionsCommandResult {
    Action(ActionSnapshot),
    Authority(ActionsAuthority),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActionsQueryResult {
    Destinations(Vec<ActionDestinationChoice>),
    Authority(ActionsAuthority),
    Action(ActionSnapshot),
    Page(ActionsPage),
}

pub trait ActionsCommands {
    fn actions_command(
        &self,
        caller: &CallerContext,
        command_id: Uuid,
        command: ActionsCommand,
    ) -> Result<ActionsCommandResult, AgentFailure>;
}

pub trait ActionsQueries {
    fn actions_query(
        &self,
        caller: &CallerContext,
        request_id: Uuid,
        query: ActionsQuery,
    ) -> Result<ActionsQueryResult, AgentFailure>;
}

impl ActionsCommands for AppComposition {
    fn actions_command(
        &self,
        caller: &CallerContext,
        command_id: Uuid,
        command: ActionsCommand,
    ) -> Result<ActionsCommandResult, AgentFailure> {
        let owners = self.ready_owners(caller)?;
        let actor = caller.owner_actor();
        let scope = crate::host_scope(command_id, Cancellation::new(), Duration::from_secs(35));
        self.execute_owner(async {
            match command {
                ActionsCommand::Submit { intent } => owners.actions
                    .submit(&actor, command_id, intent, &scope)
                    .await
                    .map(ActionsCommandResult::Action),
                ActionsCommand::Decide { action_ref, review_ref, decision, expected_revision } => owners.actions
                    .decide(&actor, command_id, action_ref, review_ref, decision, expected_revision, &scope)
                    .await
                    .map(ActionsCommandResult::Action),
                ActionsCommand::Reconcile { action_ref, expected_revision } => owners.actions
                    .reconcile(&actor, command_id, action_ref, expected_revision, &scope)
                    .await
                    .map(ActionsCommandResult::Action),
                ActionsCommand::SetAuthority { mode, expected_revision } => owners.actions
                    .set_calendar_create_authority(&actor, command_id, mode, expected_revision, &scope)
                    .await
                    .map(ActionsCommandResult::Authority),
            }
        })
    }
}

impl ActionsQueries for AppComposition {
    fn actions_query(
        &self,
        caller: &CallerContext,
        request_id: Uuid,
        query: ActionsQuery,
    ) -> Result<ActionsQueryResult, AgentFailure> {
        let owners = self.ready_owners(caller)?;
        let actor = caller.owner_actor();
        let scope = crate::host_scope(request_id, Cancellation::new(), Duration::from_secs(35));
        self.execute_owner(async {
            match query {
                ActionsQuery::Destinations => owners.actions
                    .destinations(&actor, &scope)
                    .await
                    .map(ActionsQueryResult::Destinations),
                ActionsQuery::Authority => owners.actions
                    .inspect_authority(&actor, &scope)
                    .await
                    .map(ActionsQueryResult::Authority),
                ActionsQuery::Inspect { action_ref } => owners.actions
                    .inspect(&actor, action_ref, &scope)
                    .await
                    .map(ActionsQueryResult::Action),
                ActionsQuery::List { cursor, limit } => owners.actions
                    .list(&actor, cursor, limit, &scope)
                    .await
                    .map(ActionsQueryResult::Page),
            }
        })
    }
}
