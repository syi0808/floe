# Integrate Conversation client with safe owner projections

Own only apps/client/lib/features/conversation/** EXCEPT domain/agent_interaction.dart and presentation/agent_interaction_card.dart (another worker owns their ExpertBinding branches), plus apps/client/lib/app/runtime/local_owner_gateways_scope.dart. Other workers own Actions/Day/Experts/Knowledge and app_runtime/main/settings. Source-only S2: no formatter/analyzer/compiler/build/tests/package commands, native/provider operations, commits or push. Read actual pinned backend protocol dto/conversation.rs, commands/queries and session_projection.rs.

## Safe session/Task decoding

Session usage now has unknown_token_attempts and unknown_cost_attempts alongside model_attempts/estimated counters. Validate nonnegative counts <=model_attempts; do not infer uncertainty from an estimated amount being nonzero. Preserve all previous typed message/outcome/continuation parsing, and accept the exact Blocked Task state. Do not reconstruct removed raw AgentSession payloads.

Each safe delegation Task has execution_receipt: optional TaskExecutionReceiptRefDto, from the actual acknowledged Task. The field is present, possibly null; no reference is valid only for an unadmitted Rejected Task with issue and no artifacts. Validate nonnil canonical IDs, matching task_id, positive execution generation, task_revision>=2, journal_revision0..512 and nonzero lowercase SHA256. Preserve artifact ID/name/media_types only; no artifact data JSON. Expose AgentCapabilityMessage.executionReceipt as TaskExecutionReceiptReference? and retain existing artifacts List<AgentArtifact> with id/name/mediaTypes. Capability messages have executionReceipt=null. The Actions worker defines canonical TaskExecutionReceiptReference in features/actions/domain/calendar_action.dart with fromJson/toJson; import that actual shared boundary value rather than inventing another authority DTO. Report if the worker's type differs so native can align it.

## Remove obsolete Action proposal orchestration

Remove ConversationController's legacy proposal inspection map/methods and calls to actions.proposal.inspect/read_result. Conversation never fabricates Task proof, Actions intent, approval, execution or collection. LocalOwnerGateways replaces proposals:AgentProposalGateway with actions:CalendarActionGateway; preserve vault/registry/memory/memoryReview fields and their existing typed interfaces. Native coordinator owns AppRuntime constructing the one gateway.

Keep AgentPanel's use of the Actions-owned AgentProposalCard(controller,message,onOpenAction) constructor. Actions worker will make that card use controller.owners.actions and message.executionReceipt plus exact artifact ID, select an owner destination and submit typed ExpertProposal intent. ConversationController.owners remains accessible. Do not edit the card or Actions files. Gate display on actual matching media type and receipt availability; no fake placeholder proof. User command and actual owner response drive state, screen disposal stops observers only.

## Preserve Conversation semantics

No regression to read/watch cancellation of active Run, command IDs versus request IDs, source-review actions/refresh, receipt revision/epoch checks, partial result or exact continuation references. Do not reintroduce generic owner result polling for Actions/Experts/Knowledge. Existing native callback lane is out of scope. Keep current UI style and localization strings, report any unowned app constructor changes.

Return full owned files/patch and exact interface notes. No tests or executable validation in this phase.
