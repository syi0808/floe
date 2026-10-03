import 'package:floe_client/app/runtime/agent_vault_gateway.dart';
import 'package:floe_client/features/experts/domain/agent_registry.dart';
import 'package:floe_client/features/knowledge/domain/agent_memory.dart';
import 'package:floe_client/features/knowledge/application/memory_gateway.dart';
import 'package:floe_client/features/actions/application/calendar_action_gateway.dart';

final class LocalOwnerGateways {
  const LocalOwnerGateways({
    this.vault,
    this.registry,
    this.memory,
    this.memoryReview,
    this.actions,
  });
  final AgentVaultGateway? vault;
  final AgentRegistryGateway? registry;
  final AgentMemoryGateway? memory;
  final AgentMemoryReviewGateway? memoryReview;
  final CalendarActionGateway? actions;
}
