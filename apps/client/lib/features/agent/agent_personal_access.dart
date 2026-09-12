abstract interface class AgentPersonalAccessGateway {
  Future<PersonalAccessOverview> inspectPersonalAttention(String personId);
  Future<PersonalAccessOverview> reviewPersonalAttention(
    String personId, {
    required PersonalAccessOverview reviewedPreview,
    required List<String> consumers,
  });
  Future<PersonalAccessOverview> setPersonalAttentionEnabled(
    String personId,
    bool enabled,
  );
  Future<PersonalAccessOverview> inspectPersonalContacts(
    String personId,
    List<String> selectedHandles,
  );
  Future<PersonalAccessOverview> reviewPersonalContacts(
    String personId, {
    required List<String> selectedHandles,
    required PersonalAccessOverview reviewedPreview,
    required List<String> consumers,
  });
}

final class PersonalAccessOverview {
  const PersonalAccessOverview({
    required this.schemaVersion,
    required this.personId,
    required this.connector,
    required this.deviceId,
    required this.connectionId,
    required this.grantId,
    required this.grantAuthority,
    required this.state,
    required this.reviewRequired,
    required this.presenceAvailable,
    required this.consumers,
    required this.nativeSubjectFingerprint,
    required this.processIncarnation,
  });

  final int schemaVersion;
  final String personId;
  final String connector;
  final String deviceId;
  final String connectionId;
  final String? grantId;
  final Map<String, Object?>? grantAuthority;
  final String state;
  final bool reviewRequired;
  final bool presenceAvailable;
  final List<String> consumers;
  final String? nativeSubjectFingerprint;
  final String? processIncarnation;

  PersonalAccessOverview withNativeSubjectFingerprint(String fingerprint) =>
      PersonalAccessOverview(
        schemaVersion: schemaVersion,
        personId: personId,
        connector: connector,
        deviceId: deviceId,
        connectionId: connectionId,
        grantId: grantId,
        grantAuthority: grantAuthority,
        state: state,
        reviewRequired: reviewRequired,
        presenceAvailable: presenceAvailable,
        consumers: consumers,
        nativeSubjectFingerprint: fingerprint,
        processIncarnation: processIncarnation,
      );

  factory PersonalAccessOverview.fromJson(Object? raw) {
    if (raw is! Map) throw const FormatException('Invalid personal access');
    final value = Map<String, Object?>.from(raw);
    const fields = {
      'schema_version',
      'person_id',
      'connector',
      'device_id',
      'connection_id',
      'source_authority',
      'grant_id',
      'grant_authority',
      'state',
      'review_required',
      'presence_available',
      'consumers',
      'process_incarnation',
      'native_subject_fingerprint',
    };
    if (value.keys.any((key) => !fields.contains(key)) ||
        value['schema_version'] != 1 ||
        value['person_id'] is! String ||
        value['connector'] is! String ||
        value['device_id'] is! String ||
        value['connection_id'] is! String ||
        (value['grant_id'] != null && value['grant_id'] is! String) ||
        (value['grant_authority'] != null &&
            value['grant_authority'] is! Map) ||
        value['state'] is! String ||
        value['review_required'] is! bool ||
        value['presence_available'] is! bool ||
        value['consumers'] is! List ||
        (value['consumers']! as List).length > 2 ||
        (value['consumers']! as List).any((item) => item is! String) ||
        (value['native_subject_fingerprint'] != null &&
            value['native_subject_fingerprint'] is! String) ||
        (value['process_incarnation'] != null &&
            value['process_incarnation'] is! String)) {
      throw const FormatException('Invalid personal access');
    }
    return PersonalAccessOverview(
      schemaVersion: 1,
      personId: value['person_id']! as String,
      connector: value['connector']! as String,
      deviceId: value['device_id']! as String,
      connectionId: value['connection_id']! as String,
      grantId: value['grant_id'] as String?,
      grantAuthority: value['grant_authority'] == null
          ? null
          : Map<String, Object?>.from(value['grant_authority']! as Map),
      state: value['state']! as String,
      reviewRequired: value['review_required']! as bool,
      presenceAvailable: value['presence_available']! as bool,
      consumers: List.unmodifiable(
        (value['consumers']! as List).cast<String>(),
      ),
      nativeSubjectFingerprint: value['native_subject_fingerprint'] as String?,
      processIncarnation: value['process_incarnation'] as String?,
    );
  }
}
