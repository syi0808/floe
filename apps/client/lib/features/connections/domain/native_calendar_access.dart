abstract interface class NativeCalendarAccessGateway {
  Future<NativeCalendarAccessOverview> inspectCalendarAccess(String personId);
  Future<NativeCalendarSubjectPreview> previewCalendarSubject({
    required String personId,
    required String provider,
    required String connectionId,
    required List<String> calendarIds,
    required String connectionScope,
    required int connectionRevision,
    required Map<String, Object?> sourceAuthority,
  });
  Future<NativeCalendarAccessOverview> reviewCalendarAccess(
    String personId, {
    required String connectionId,
    required List<String> calendarIds,
    required Map<String, Object?> expectedSourceAuthority,
    required String expectedNativeSubjectFingerprint,
    required NativeCalendarAccessOverview reviewedOverview,
  });
  Future<NativeCalendarAccessOverview> pauseCalendarAccess(
    String personId, {
    required NativeCalendarAccessOverview reviewedOverview,
  });
  Future<NativeCalendarAccessOverview> removeCalendarAccess(
    String personId, {
    required NativeCalendarAccessOverview reviewedOverview,
  });
}

final class NativeCalendarAccessOverview {
  const NativeCalendarAccessOverview({
    required this.personId,
    required this.provider,
    required this.connectionId,
    required this.selectedResources,
    required this.grantedResources,
    required this.sourceAuthority,
    required this.state,
    required this.reviewRequired,
    this.grantId,
    this.grantAuthority,
    this.consumerPolicy,
  });

  final String personId;
  final String provider;
  final String connectionId;
  final List<String> selectedResources;
  final List<String> grantedResources;
  final Map<String, Object?> sourceAuthority;
  final String state;
  final bool reviewRequired;
  final String? grantId;
  final Map<String, Object?>? grantAuthority;
  final Map<String, Object?>? consumerPolicy;

  bool get hasGrant =>
      grantId != null && grantAuthority != null && consumerPolicy != null;

  factory NativeCalendarAccessOverview.fromJson(Object? raw) {
    if (raw is! Map) {
      throw const FormatException('Invalid Calendar access overview');
    }
    final value = Map<String, Object?>.from(raw);
    if (value['schema_version'] != 1 ||
        value['person_id'] is! String ||
        value['provider'] is! String ||
        value['connection_id'] is! String ||
        value['selected_resources'] is! List ||
        value['granted_resources'] is! List ||
        value['source_authority'] is! Map ||
        value['review_required'] is! bool ||
        !const {'needs_review', 'paused', 'active', 'revoked'}.contains(
          value['state'],
        )) {
      throw const FormatException('Invalid Calendar access overview');
    }
    final grantId = value['grant_id'];
    final grantAuthority = value['grant_authority'];
    final consumerPolicy = value['consumer_policy'];
    if ((grantId != null && grantId is! String) ||
        (grantAuthority != null && grantAuthority is! Map) ||
        (consumerPolicy != null && consumerPolicy is! Map)) {
      throw const FormatException('Invalid Calendar access overview');
    }
    final present =
        (grantId != null ? 1 : 0) +
        (grantAuthority != null ? 1 : 0) +
        (consumerPolicy != null ? 1 : 0);
    if (present != 0 && present != 3) {
      throw const FormatException('Invalid Calendar access overview');
    }
    final state = value['state'] as String;
    if ((state == 'needs_review') == (present == 3)) {
      throw const FormatException('Invalid Calendar access overview');
    }
    return NativeCalendarAccessOverview(
      personId: value['person_id'] as String,
      provider: value['provider'] as String,
      connectionId: value['connection_id'] as String,
      selectedResources: List<String>.from(value['selected_resources'] as List),
      grantedResources: List<String>.from(value['granted_resources'] as List),
      sourceAuthority: Map<String, Object?>.from(value['source_authority'] as Map),
      state: state,
      reviewRequired: value['review_required'] as bool,
      grantId: grantId as String?,
      grantAuthority: grantAuthority == null
          ? null
          : Map<String, Object?>.from(grantAuthority as Map),
      consumerPolicy: consumerPolicy == null
          ? null
          : Map<String, Object?>.from(consumerPolicy as Map),
    );
  }
}

final class NativeCalendarSubjectPreview {
  const NativeCalendarSubjectPreview({
    required this.provider,
    required this.deviceId,
    required this.calendarIds,
    required this.connectionScope,
    required this.connectionId,
    required this.connectionRevision,
    required this.sourceAuthority,
    required this.nativeSubjectFingerprint,
  });

  final String provider;
  final String deviceId;
  final List<String> calendarIds;
  final String connectionScope;
  final String connectionId;
  final int connectionRevision;
  final Map<String, Object?> sourceAuthority;
  final String nativeSubjectFingerprint;

  factory NativeCalendarSubjectPreview.fromJson(Object? raw) {
    if (raw is! Map) {
      throw const FormatException('Invalid Calendar subject preview');
    }
    final value = Map<String, Object?>.from(raw);
    if (value['provider'] is! String ||
        value['device_id'] is! String ||
        value['calendar_ids'] is! List ||
        value['connection_scope'] is! String ||
        value['connection_id'] is! String ||
        value['connection_revision'] is! int ||
        value['source_authority'] is! Map ||
        value['native_subject_fingerprint'] is! String) {
      throw const FormatException('Invalid Calendar subject preview');
    }
    return NativeCalendarSubjectPreview(
      provider: value['provider'] as String,
      deviceId: value['device_id'] as String,
      calendarIds: List<String>.from(value['calendar_ids'] as List),
      connectionScope: value['connection_scope'] as String,
      connectionId: value['connection_id'] as String,
      connectionRevision: value['connection_revision'] as int,
      sourceAuthority: Map<String, Object?>.from(value['source_authority'] as Map),
      nativeSubjectFingerprint: value['native_subject_fingerprint'] as String,
    );
  }
}
