import 'package:floe_client/app/runtime/app_runtime.dart';
import 'package:floe_client/app/runtime/app_wire_transport.dart';
import 'package:floe_client/app/runtime/native_transport.dart'
    show NativeTransportException;
import 'package:floe_client/app/runtime/owner_operation.dart'
    show ownerCommand, ownerQuery;
import 'package:floe_client/features/actions/application/calendar_action_gateway.dart';
import 'package:floe_client/features/actions/domain/calendar_action.dart';
import 'package:floe_client/features/conversation/application/agent_request_id.dart';

/// Mechanical AppWire adapter for the Actions owner routes.
///
/// Every command is sent once with its caller-owned command ID. Queries inspect
/// the owner snapshot directly; they do not start, poll, or cancel owner work.
final class NativeCalendarActionGateway implements CalendarActionGateway {
  NativeCalendarActionGateway(this._transport);

  final AppWireTransport _transport;

  @override
  Future<List<ActionDestinationChoice>> loadDestinations() =>
      _destinations({'kind': 'actions.destinations'});

  @override
  Future<ActionProposalPreview> loadProposalPreview(
    TaskExecutionReceiptReference receipt,
    String artifactId,
  ) async {
    final result = await _query({
      'kind': 'actions.proposal.preview',
      'receipt': receipt.toJson(),
      'artifact_id': artifactId,
    });
    _expectResult(result, 'actions.proposal.preview', const {'preview'});
    return ActionProposalPreview.fromJson(
      _object(result['preview'], 'proposal preview'),
    );
  }

  Future<List<ActionDestinationChoice>> _destinations(
    Map<String, Object?> query,
  ) async {
    final result = await _query(query);
    _expectResult(result, 'actions.destinations', const {'destinations'});
    final raw = result['destinations'];
    if (raw is! List || raw.length > 256) {
      throw const FormatException('Invalid Actions destinations result.');
    }
    final destinations = raw
        .map(
          (value) => ActionDestinationChoice.fromJson(
            _object(value, 'Actions destination'),
          ),
        )
        .toList(growable: false);
    if (destinations
            .map((destination) => destination.destinationRef)
            .toSet()
            .length !=
        destinations.length) {
      throw const FormatException('Duplicate Actions destination reference.');
    }
    return List.unmodifiable(destinations);
  }

  @override
  Future<ActionAuthority> loadAuthority() async {
    final result = await _query({'kind': 'actions.authority.get'});
    _expectResult(result, 'actions.authority', const {'authority'});
    return ActionAuthority.fromJson(
      _object(result['authority'], 'Actions authority'),
    );
  }

  @override
  Future<ActionAuthority> setAuthority({
    required String commandId,
    required ActionAuthorityMode mode,
    required int expectedRevision,
  }) async {
    if (expectedRevision <= 0) {
      throw const FormatException('Invalid Actions authority revision.');
    }
    final result = await _command(commandId, {
      'kind': 'actions.authority.set_calendar_create',
      'mode': mode.name,
      'expected_revision': expectedRevision,
    });
    _expectResult(result, 'actions.authority', const {'authority'});
    return ActionAuthority.fromJson(
      _object(result['authority'], 'Actions authority'),
    );
  }

  @override
  Future<CalendarAction> submit({
    required String commandId,
    required ActionIntent intent,
  }) async {
    final result = await _command(commandId, {
      'kind': 'actions.submit',
      'intent': intent.toJson(),
    });
    _expectResult(result, 'actions.action', const {'action'});
    return CalendarAction.fromJson(_object(result['action'], 'Actions action'));
  }

  @override
  Future<CalendarAction> decide({
    required String commandId,
    required CalendarAction action,
    required CalendarActionDecision decision,
  }) async {
    final result = await _command(commandId, {
      'kind': 'actions.decide',
      'action_ref': action.actionRef,
      'review_ref': action.reviewRef.toJson(),
      'decision': decision.name,
      'expected_revision': action.revision,
    });
    _expectResult(result, 'actions.action', const {'action'});
    final observed = CalendarAction.fromJson(
      _object(result['action'], 'Actions action'),
    );
    if (observed.actionRef != action.actionRef || !observed.follows(action)) {
      throw const FormatException(
        'Actions response identity or revision mismatch.',
      );
    }
    return observed;
  }

  @override
  Future<CalendarAction> reconcile({
    required String commandId,
    required CalendarAction action,
  }) async {
    final result = await _command(commandId, {
      'kind': 'actions.reconcile',
      'action_ref': action.actionRef,
      'expected_revision': action.revision,
    });
    _expectResult(result, 'actions.action', const {'action'});
    final observed = CalendarAction.fromJson(
      _object(result['action'], 'Actions action'),
    );
    if (observed.actionRef != action.actionRef || !observed.follows(action)) {
      throw const FormatException(
        'Actions response identity or revision mismatch.',
      );
    }
    return observed;
  }

  @override
  Future<CalendarAction> inspect(String actionRef) async {
    final result = await _query({
      'kind': 'actions.inspect',
      'action_ref': actionRef,
    });
    _expectResult(result, 'actions.action', const {'action'});
    final action = CalendarAction.fromJson(
      _object(result['action'], 'Actions action'),
    );
    if (action.actionRef != actionRef) {
      throw const FormatException('Actions inspect reference mismatch.');
    }
    return action;
  }

  @override
  Future<ActionsPage> list({String? cursor, int limit = 100}) async {
    if (limit < 1 || limit > 100) {
      throw const FormatException('Invalid Actions page limit.');
    }
    final result = await _query({
      'kind': 'actions.list',
      'cursor': cursor,
      'limit': limit,
    });
    _expectResult(result, 'actions.page', const {'page'});
    final page = ActionsPage.fromJson(_object(result['page'], 'Actions page'));
    if (page.actions.length > limit ||
        (cursor != null && page.nextCursor == cursor)) {
      throw const FormatException(
        'Actions page exceeded its bound or did not advance.',
      );
    }
    return page;
  }

  Future<Map<String, dynamic>> _command(
    String commandId,
    Map<String, Object?> command,
  ) async {
    return _withOwnerErrors(() => ownerCommand(_transport, commandId, command));
  }

  Future<Map<String, dynamic>> _query(Map<String, Object?> query) async {
    return _withOwnerErrors(
      () => ownerQuery(_transport, newAgentRequestId(), query),
    );
  }

  Future<Map<String, dynamic>> _withOwnerErrors(
    Future<Map<String, dynamic>> Function() request,
  ) async {
    try {
      return await request();
    } on NativeTransportException catch (error) {
      throw AppRuntimeException(
        error.metadata['reason_code'] ??
            error.metadata['owner_code'] ??
            error.code,
        error.message,
        field: error.field,
        metadata: error.metadata,
        ownerFailure: error.ownerFailure,
      );
    }
  }
}

void _expectResult(
  Map<String, dynamic> result,
  String kind,
  Set<String> payloadKeys,
) {
  final keys = {'kind', ...payloadKeys};
  if (result.length != keys.length ||
      !result.keys.toSet().containsAll(keys) ||
      result['kind'] != kind) {
    throw FormatException('Invalid $kind result.');
  }
}

Map<String, dynamic> _object(Object? value, String field) {
  if (value is! Map) throw FormatException('Invalid $field.');
  try {
    return Map<String, dynamic>.from(value);
  } on Object {
    throw FormatException('Invalid $field.');
  }
}
