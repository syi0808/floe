import 'package:floe_client/features/vault/application/vault_controller.dart';
import 'package:floe_client/features/experts/application/agent_registry_controller.dart';
import 'package:floe_client/features/knowledge/application/agent_memory_controller.dart';
import 'package:floe_client/features/actions/application/calendar_action_gateway.dart';

final class LocalOwnerGateways {
  const LocalOwnerGateways({
    this.vault,
    this.registry,
    this.memory,
    this.actions,
  });
  final VaultController? vault;
  final AgentRegistryController? registry;
  final AgentMemoryController? memory;
  final CalendarActionGateway? actions;
}
