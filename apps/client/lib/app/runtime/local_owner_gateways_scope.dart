import 'package:floe_client/app/runtime/agent_vault_gateway.dart';
import 'package:floe_client/features/experts/domain/agent_registry.dart';
import 'package:floe_client/features/settings/domain/agent_personal_access.dart';
import 'package:floe_client/features/knowledge/domain/agent_memory.dart';
import 'package:floe_client/features/knowledge/presentation/agent_memory_review.dart';
import 'package:floe_client/features/connections/domain/agent_connections.dart';
import 'package:floe_client/features/connections/domain/native_calendar_access.dart';
import 'package:floe_client/features/actions/domain/agent_proposal.dart';

final class LocalOwnerGateways {
  const LocalOwnerGateways({
    this.vault,
    this.registry,
    this.personalAccess,
    this.calendarAccess,
    this.memory,
    this.memoryReview,
    this.connections,
    this.proposals,
  });
  final AgentVaultGateway? vault;
  final AgentRegistryGateway? registry;
  final AgentPersonalAccessGateway? personalAccess;
  final NativeCalendarAccessGateway? calendarAccess;
  final AgentMemoryGateway? memory;
  final AgentMemoryReviewGateway? memoryReview;
  final AgentConnectionsGateway? connections;
  final AgentProposalGateway? proposals;
}
