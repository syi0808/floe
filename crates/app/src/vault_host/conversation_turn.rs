//! Concrete supplied Expert endpoint construction. Conversation admission,
//! model planning, drive scheduling and interaction lifecycle live in the owner.
use std::{future::Future, pin::Pin};
use floe_agent_contract::{AgentFailure, DataClass};
use floe_context::{AgentContext, InferencePolicyDecision, NativeContextView};
use floe_experts::{A2AMessageRole, A2APart, A2ASendMessageRequest, A2ATask, AgentCard, InProcessAgent};
use floe_kernel::{AGENT_VERSION, PersonId};
use floe_vault::{EncryptedAgentVault, VaultKeyProvider};
use uuid::Uuid;
use crate::FloeCore;
use crate::local_context::LocalContextHost;
use floe_provider_adapters::sources::ServerSourceClient;
use super::{personal_grants, remote_views};
pub(super) mod expert_dispatch;
pub(super) mod expert_host;
