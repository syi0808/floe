import 'package:floe_client/app/runtime/app_wire_transport.dart';
import 'package:floe_client/app/runtime/owner_operation.dart';
import 'package:floe_client/features/knowledge/domain/memory_review.dart';
import 'package:floe_client/features/knowledge/application/memory_gateway.dart';
import 'package:floe_client/features/knowledge/domain/agent_memory.dart';

final class AppWireMemoryGateway
    implements AgentMemoryGateway, AgentMemoryReviewGateway {
  AppWireMemoryGateway(this._transport);

  final AppWireTransport _transport;
  final OwnerOperationObserver _operations = OwnerOperationObserver();

  @override
  Future<AgentMemoryOverview> readMemory(String personId) async {
    return _observe(
      personId,
      {'kind': 'knowledge.memory.overview'},
      decode: (result) {
        final memory = AgentMemoryOverview.fromJson(
          Map<String, Object?>.from(result['memory'] as Map),
        );
        if (result['state'] != 'ready' || memory.personId != personId) {
          throw const FormatException('Memory overview scope mismatch');
        }
        return memory;
      },
    );
  }

  @override
  Future<AgentMemoryReviewOverview> readMemoryReview(String personId) =>
      _memoryReview(personId, null);

  @override
  Future<AgentMemoryReviewOverview> decideMemoryCandidate({
    required String personId,
    required String candidateId,
    required AgentMemoryDecision decision,
  }) => _memoryReview(personId, {
    'candidate_id': candidateId,
    'decision': decision.name,
  });

  Future<AgentMemoryReviewOverview> _memoryReview(
    String personId,
    Map<String, Object?>? decision,
  ) async {
    return _observe(
      personId,
      {
        'kind': decision == null
            ? 'knowledge.memory.review'
            : 'knowledge.memory.decide',
        ...?decision,
      },
      decode: (result) {
        final review = AgentMemoryReviewOverview.fromJson(
          Map<String, Object?>.from(result['memory_review'] as Map),
        );
        if (result['state'] != 'ready' || review.personId != personId) {
          throw const FormatException('Memory review scope mismatch');
        }
        return review;
      },
    );
  }

  Future<T> _observe<T>(
    String personId,
    Map<String, Object?> intent, {
    required T Function(Map<String, dynamic>) decode,
  }) {
    final command = const <String>{'knowledge.memory.decide'}
        .contains(intent['kind']);
    return _operations.observe(
      scope: personId,
      intent: ownerIntent(intent),
      stage: const {
        "knowledge.memory.overview": "memory",
        "knowledge.memory.review": "memory_review",
        "knowledge.memory.decide": "memory_review",
      }[intent['kind']]!,
      resultKind: 'knowledge_operation',
      start: (operationId) => command
          ? ownerCommand(_transport, operationId, intent)
          : ownerQuery(_transport, operationId, intent),
      read: (operationId, release) => ownerResult(
        _transport,
        'knowledge.read_result',
        operationId,
        release,
      ),
      decode: decode,
    );
  }
}
