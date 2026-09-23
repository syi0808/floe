import 'package:floe_client/app/runtime/app_wire_transport.dart';
import 'package:floe_client/app/runtime/owner_operation.dart';
import 'package:floe_client/features/connections/domain/native_calendar_access.dart';

final class AppWireNativeCalendarAccessGateway
    implements NativeCalendarAccessGateway {
  AppWireNativeCalendarAccessGateway(this._transport, {required this.deviceId});

  final AppWireTransport _transport;
  final OwnerOperationObserver _operations = OwnerOperationObserver();
  final String deviceId;

  @override
  Future<NativeCalendarAccessOverview> inspectCalendarAccess(
    String personId,
  ) => _observe(personId, const {'kind': 'access.calendar.inspect'});

  @override
  Future<NativeCalendarSubjectPreview> previewCalendarSubject({
    required String personId,
    required String provider,
    required String connectionId,
    required List<String> calendarIds,
    required String connectionScope,
    required int connectionRevision,
    required Map<String, Object?> sourceAuthority,
  }) {
    if (provider.isEmpty ||
        connectionId.isEmpty ||
        calendarIds.isEmpty ||
        calendarIds.length > 4 ||
        connectionRevision <= 0) {
      throw const FormatException('Invalid Calendar subject request');
    }
    return _operations.observe(
      scope: personId,
      intent: ownerIntent({
        'kind': 'access.calendar.preview',
        'request': {
          'provider': provider,
          'connection_id': connectionId,
          'calendar_ids': List<String>.unmodifiable(calendarIds),
          'connection_scope': connectionScope,
          'connection_revision': connectionRevision,
          'source_authority': sourceAuthority,
        },
      }),
      stage: 'calendar_subject_preview',
      resultKind: 'local_access_operation',
      start: (operationId) => ownerQuery(_transport, operationId, {
        'kind': 'access.calendar.preview',
        'request': {
          'provider': provider,
          'connection_id': connectionId,
          'calendar_ids': List<String>.unmodifiable(calendarIds),
          'connection_scope': connectionScope,
          'connection_revision': connectionRevision,
          'source_authority': sourceAuthority,
        },
      }),
      read: (operationId, release) => ownerResult(
        _transport,
        'access.local.read_result',
        operationId,
        release,
      ),
      decode: (result) {
        if (result['state'] != 'ready' ||
            result['calendar_subject_preview'] is! Map) {
          throw const FormatException('Missing Calendar subject preview');
        }
        return NativeCalendarSubjectPreview.fromJson(
          result['calendar_subject_preview'],
        );
      },
    );
  }

  @override
  Future<NativeCalendarAccessOverview> reviewCalendarAccess(
    String personId, {
    required String connectionId,
    required List<String> calendarIds,
    required Map<String, Object?> expectedSourceAuthority,
    required String expectedNativeSubjectFingerprint,
    required NativeCalendarAccessOverview reviewedOverview,
  }) {
    if (reviewedOverview.personId != personId ||
        reviewedOverview.connectionId != connectionId ||
        calendarIds.isEmpty ||
        calendarIds.length > 4 ||
        !_authorityEquals(
          reviewedOverview.sourceAuthority,
          expectedSourceAuthority,
        )) {
      throw const FormatException('Calendar review scope changed');
    }
    return _observe(personId, {
      'kind': 'access.calendar.configure',
      'change': {
        'kind': 'review',
        'connection_id': connectionId,
        'calendar_ids': List<String>.unmodifiable(calendarIds),
        'expected_source_authority': expectedSourceAuthority,
        'expected_native_subject_fingerprint': expectedNativeSubjectFingerprint,
        'expected_grant_id': reviewedOverview.grantId,
        'expected_grant_authority': reviewedOverview.grantAuthority,
      },
    });
  }

  @override
  Future<NativeCalendarAccessOverview> pauseCalendarAccess(
    String personId, {
    required NativeCalendarAccessOverview reviewedOverview,
  }) => _mutate(personId, reviewedOverview, 'pause');

  @override
  Future<NativeCalendarAccessOverview> removeCalendarAccess(
    String personId, {
    required NativeCalendarAccessOverview reviewedOverview,
  }) => _mutate(personId, reviewedOverview, 'remove');

  Future<NativeCalendarAccessOverview> _mutate(
    String personId,
    NativeCalendarAccessOverview reviewedOverview,
    String kind,
  ) {
    final grantId = reviewedOverview.grantId;
    final grantAuthority = reviewedOverview.grantAuthority;
    if (reviewedOverview.personId != personId ||
        grantId == null ||
        grantAuthority == null) {
      throw const FormatException('Calendar access scope changed');
    }
    return _observe(personId, {
      'kind': 'access.calendar.configure',
      'change': {
        'kind': kind,
        'grant_id': grantId,
        'expected_grant_authority': grantAuthority,
      },
    });
  }

  Future<NativeCalendarAccessOverview> _observe(
    String personId,
    Map<String, Object?> intent,
  ) {
    final command = intent['kind'] == 'access.calendar.configure';
    return _operations.observe(
      scope: personId,
      intent: ownerIntent(intent),
      stage: 'calendar_access',
      resultKind: 'local_access_operation',
      start: (operationId) => command
          ? ownerCommand(_transport, operationId, intent)
          : ownerQuery(_transport, operationId, intent),
      read: (operationId, release) => ownerResult(
        _transport,
        'access.local.read_result',
        operationId,
        release,
      ),
      decode: (result) {
        if (result['state'] != 'ready' || result['calendar_access'] is! Map) {
          throw const FormatException('Missing Calendar access overview');
        }
        final overview = NativeCalendarAccessOverview.fromJson(
          result['calendar_access'],
        );
        if (overview.personId != personId) {
          throw const FormatException('Calendar access scope mismatch');
        }
        return overview;
      },
    );
  }
}

bool _authorityEquals(Map<String, Object?> a, Map<String, Object?> b) =>
    a.length == b.length &&
    a.keys.every((key) => b.containsKey(key) && b[key] == a[key]);
