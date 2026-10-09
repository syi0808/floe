import 'package:floe_client/app/runtime/runtime_controller.dart';
import 'package:floe_client/features/conversation/assistant_features/application/assistant_feature_controller.dart';
import 'package:floe_client/features/knowledge/application/agent_memory_controller.dart';
import 'package:floe_client/features/actions/application/calendar_action_gateway.dart';

final class LocalOwnerGateways {
  const LocalOwnerGateways({
    this.runtime,
    this.assistantFeatures,
    this.memory,
    this.operationAuthorization,
  });
  final RuntimeController? runtime;
  final AssistantFeatureController? assistantFeatures;
  final AgentMemoryController? memory;
  final OperationAuthorizationGateway? operationAuthorization;
}
