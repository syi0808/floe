# ConversationAgent Orchestrator Integration Plan - 2025-06-22 06:39 UTC

Based on the completion of the ConversationAgent basic modules, the next step is to integrate it with the OrchestratorEngine so that general conversation intents are handled by the new agent.

## Objectives
- Wrap `ConversationAgent` so it conforms to the `BaseAgent` interface.
- Register this wrapper with `OrchestrationEngine` for the `general_conversation` intent.
- Update routing logic to use a registered agent if available.
- Provide unit tests verifying this behaviour.

## Planned Work
1. **`conversation_agent/orchestrator_wrapper.py`**
   - Implement `ConversationAgentWrapper` inheriting from `BaseAgent`.
   - Expose `name` and `supported_intents` (`['general_conversation']`).
   - `process()` will call `ConversationAgent.handle_message` and return an `AgentResponse`.
2. **`orchestrator_agent/orchestrator_core.py`**
   - Modify `route_request` so that if an agent is registered for the intent, it is used even for `general_conversation`.
   - Fallback to existing behaviour only when no agent is registered.
3. **Tests**
   - Add fixture creating an engine with the conversation agent wrapper.
   - Verify that a `general_conversation` intent is routed to the wrapper and that the wrapper's response appears in the orchestrator response.
4. **Documentation**
   - Summarise this work and outline next potential steps after implementation.
