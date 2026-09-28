final class CalendarSourceAuthority {
  const CalendarSourceAuthority({
    required this.incarnation,
    required this.epoch,
  });

  factory CalendarSourceAuthority.fromJson(Object? value) {
    if (value is! Map || value.keys.any((key) => key is! String)) {
      throw const FormatException('Invalid Calendar source authority');
    }
    final json = value.cast<String, Object?>();
    final incarnation = json['incarnation'];
    final epoch = json['epoch'];
    if (json.length != 2 ||
        !json.keys.toSet().containsAll(const {'incarnation', 'epoch'}) ||
        incarnation is! String ||
        !_uuidPattern.hasMatch(incarnation) ||
        incarnation.toLowerCase() == _nilUuid ||
        epoch is! int ||
        epoch <= 0) {
      throw const FormatException('Invalid Calendar source authority');
    }
    return CalendarSourceAuthority(incarnation: incarnation, epoch: epoch);
  }

  final String incarnation;
  final int epoch;

  bool get isValid =>
      _uuidPattern.hasMatch(incarnation) &&
      incarnation.toLowerCase() != _nilUuid &&
      epoch > 0;

  Map<String, Object> toJson() => {'incarnation': incarnation, 'epoch': epoch};

  @override
  bool operator ==(Object other) =>
      other is CalendarSourceAuthority &&
      other.incarnation == incarnation &&
      other.epoch == epoch;

  @override
  int get hashCode => Object.hash(incarnation, epoch);
}

final _uuidPattern = RegExp(
  r'^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$',
);
const _nilUuid = '00000000-0000-0000-0000-000000000000';

final class CalendarSourceResource {
  const CalendarSourceResource({required this.handle, required this.label});

  factory CalendarSourceResource.fromJson(Map<String, dynamic> json) {
    final handle = json['handle'];
    final label = json['label'];
    if (handle is! String ||
        handle.isEmpty ||
        label is! String ||
        label.isEmpty) {
      throw const FormatException('Invalid Calendar source resource');
    }
    return CalendarSourceResource(handle: handle, label: label);
  }

  final String handle;
  final String label;

  Map<String, Object> toJson() => {'handle': handle, 'label': label};
}

final class CalendarSourceConnection {
  const CalendarSourceConnection({
    required this.connectorId,
    required this.connectionId,
    required this.executionOwnerId,
    required this.state,
    required this.revision,
    required this.sourceAuthority,
    required this.resourceMode,
    required this.resources,
    this.nativeSubjectFingerprint,
  });

  factory CalendarSourceConnection.fromJson(Map<String, dynamic> json) {
    final connectorId = json['connector_id'];
    final connectionId = json['connection_id'];
    final executionOwnerId = json['execution_owner_id'];
    final state = json['state'];
    final revision = json['revision'];
    final resourceMode = json['resource_mode'];
    final rawResources = json['resources'];
    final fingerprint = json['native_subject_fingerprint'];
    if (connectorId is! String ||
        !_providers.containsKey(connectorId) ||
        connectionId is! String ||
        connectionId.isEmpty ||
        executionOwnerId is! String ||
        executionOwnerId.isEmpty ||
        state is! String ||
        !const {'pending', 'ready', 'disconnected'}.contains(state) ||
        revision is! int ||
        revision <= 0 ||
        resourceMode is! String ||
        !const {'selected', 'all_available'}.contains(resourceMode) ||
        rawResources is! List ||
        rawResources.length > 4096 ||
        fingerprint != null &&
            (fingerprint is! String ||
                !_fingerprintPattern.hasMatch(fingerprint))) {
      throw const FormatException('Invalid Calendar source connection');
    }
    final resources = rawResources
        .map((resource) {
          if (resource is! Map || resource.keys.any((key) => key is! String)) {
            throw const FormatException('Invalid Calendar source resource');
          }
          return CalendarSourceResource.fromJson(
            Map<String, dynamic>.from(resource),
          );
        })
        .toList(growable: false);
    for (var index = 1; index < resources.length; index++) {
      if (resources[index - 1].handle.compareTo(resources[index].handle) >= 0) {
        throw const FormatException('Unordered Calendar source resources');
      }
    }
    final native =
        connectorId == 'calendar.event_kit' ||
        connectorId == 'calendar.android';
    if (native && state == 'ready' && fingerprint == null ||
        !native && fingerprint != null) {
      throw const FormatException('Invalid Calendar source subject');
    }
    return CalendarSourceConnection(
      connectorId: connectorId,
      connectionId: connectionId,
      executionOwnerId: executionOwnerId,
      state: state,
      revision: revision,
      sourceAuthority: CalendarSourceAuthority.fromJson(
        json['source_authority'],
      ),
      resourceMode: resourceMode,
      resources: resources,
      nativeSubjectFingerprint: fingerprint as String?,
    );
  }

  final String connectorId;
  final String connectionId;
  final String executionOwnerId;
  final String state;
  final int revision;
  final CalendarSourceAuthority sourceAuthority;
  final String resourceMode;
  final List<CalendarSourceResource> resources;
  final String? nativeSubjectFingerprint;

  String get provider => _providers[connectorId]!;
  bool get isServing => state == 'ready';
  bool get includeAll => resourceMode == 'all_available';
  List<String> get selectedCalendarIds =>
      resources.map((resource) => resource.handle).toList(growable: false);
}

const _providers = {
  'calendar.event_kit': 'event_kit',
  'calendar.google': 'google_calendar',
  'calendar.microsoft': 'microsoft_calendar',
  'calendar.fixture': 'fixture',
  'calendar.android': 'android',
};

final _fingerprintPattern = RegExp(r'^[0-9a-fA-F]{64}$');
