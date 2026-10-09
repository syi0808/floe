import 'package:floe_client/app/runtime/runtime_controller.dart';
import 'package:floe_client/features/experts/application/agent_registry_controller.dart';
import 'package:floe_client/features/knowledge/application/agent_memory_controller.dart';
import 'package:floe_client/features/actions/application/calendar_action_gateway.dart';

final class LocalOwnerGateways {
  const LocalOwnerGateways({
    this.runtime,
    this.registry,
    this.memory,
    this.operationAuthorization,
  });
  final RuntimeController? runtime;
  final AgentRegistryController? registry;
  final AgentMemoryController? memory;
  final OperationAuthorizationGateway? operationAuthorization;
}
