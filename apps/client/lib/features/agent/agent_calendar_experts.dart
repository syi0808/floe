import 'dart:convert';

import 'agent_registry.dart';

abstract interface class AgentCalendarExpertGateway {
  Future<AgentCalendarExperts> readCalendarExperts(String personId);
  Future<AgentCalendarExperts> installCalendarExpert(
    AgentCalendarSetup request,
  );
  Future<AgentCalendarExperts> configureCalendarAccess(
    AgentCalendarAccessRequest request,
  );
}

enum AgentCalendarAccessOperation { setEnabled, setScope, remove }

final class AgentCalendarAccessRequest {
  AgentCalendarAccessRequest({
    required String personId,
    required String instanceId,
    required int expectedRevision,
    required String setupId,
    required this.operation,
    this.enabled,
    this.provider,
    String? replacementSetupId,
    List<String>? calendarIds,
  }) : personId = _identifier(personId),
       instanceId = _identifier(instanceId),
       expectedRevision = _counter(expectedRevision),
       setupId = _identifier(setupId),
       replacementSetupId = replacementSetupId == null
           ? null
           : _identifier(replacementSetupId),
       calendarIds = calendarIds == null
           ? null
           : _scope(calendarIds, canonical: false) {
    if ((operation == AgentCalendarAccessOperation.setEnabled &&
            enabled == null) ||
        (operation == AgentCalendarAccessOperation.setScope &&
            (provider == null ||
                this.calendarIds == null ||
                this.replacementSetupId == null)) ||
        (operation == AgentCalendarAccessOperation.remove &&
            (enabled != null || provider != null || calendarIds != null))) {
      throw const FormatException('Invalid Calendar access change');
    }
    if (provider != null) _provider(provider);
  }

  final String personId;
  final String instanceId;
  final int expectedRevision;
  final String setupId;
  final AgentCalendarAccessOperation operation;
  final bool? enabled;
  final String? provider;
  final String? replacementSetupId;
  final List<String>? calendarIds;

  Map<String, Object> toJson() => {
    'instance_id': instanceId,
    'expected_revision': expectedRevision,
    'setup_id': setupId,
    'change': switch (operation) {
      AgentCalendarAccessOperation.setEnabled => {
        'kind': 'set_enabled',
        'enabled': enabled!,
      },
      AgentCalendarAccessOperation.setScope => {
        'kind': 'set_scope',
        'replacement_setup_id': replacementSetupId!,
        'provider': provider!,
        'calendar_ids': calendarIds!,
      },
      AgentCalendarAccessOperation.remove => {'kind': 'remove'},
    },
  };
}

final class AgentCalendarSetup {
  AgentCalendarSetup({
    required String personId,
    required String instanceId,
    required int expectedRevision,
    required String setupId,
    required String provider,
    required List<String> calendarIds,
  }) : personId = _identifier(personId),
       instanceId = _identifier(instanceId),
       expectedRevision = _counter(expectedRevision),
       setupId = _identifier(setupId),
       provider = _provider(provider),
       calendarIds = _scope(calendarIds, canonical: false);

  final String personId;
  final String instanceId;
  final int expectedRevision;
  final String setupId;
  final String provider;
  final List<String> calendarIds;

  Map<String, Object> toJson() => {
    'instance_id': instanceId,
    'expected_revision': expectedRevision,
    'setup_id': setupId,
    'provider': provider,
    'calendar_ids': calendarIds,
  };
}

final class AgentCalendarExperts {
  AgentCalendarExperts.fromJson(Map<String, dynamic> json)
    : registry = AgentRegistryView.fromJson(_map(json['registry'])),
      views = List.unmodifiable(
        _entries(json['views'], 256).map(AgentCalendarView.fromJson),
      ),
      setups = List.unmodifiable(
        _entries(json['setups'], 64).map(AgentCalendarSetupReceipt.fromJson),
      ) {
    if (views.map((entry) => entry.handle).toSet().length != views.length ||
        setups.map((entry) => entry.setupId).toSet().length != setups.length ||
        setups.map((entry) => entry.viewHandle).toSet().length !=
            setups.length ||
        setups.map((entry) => entry.toolInstallationId).toSet().length !=
            setups.length ||
        setups.map((entry) => entry.expertInstallationId).toSet().length !=
            setups.length ||
        setups.map((entry) => entry.toolAssignmentId).toSet().length !=
            setups.length ||
        setups.map((entry) => entry.expertAssignmentId).toSet().length !=
            setups.length ||
        views.any((entry) => entry.personId != registry.personId)) {
      throw const FormatException(
        'Invalid Calendar Expert ownership or identity',
      );
    }
    for (final setup in setups) {
      final view = views
          .where((entry) => entry.handle == setup.viewHandle)
          .singleOrNull;
      if (setup.personId != registry.personId ||
          setup.expectedRevision >= registry.revision ||
          view == null) {
        throw const FormatException('Invalid Calendar setup receipt');
      }
      for (final (kind, installationId, assignmentId, packageId, tools) in [
        (
          'tool',
          setup.toolInstallationId,
          setup.toolAssignmentId,
          'floe.builtin.schedule.context',
          0,
        ),
        (
          'expert',
          setup.expertInstallationId,
          setup.expertAssignmentId,
          'floe.builtin.schedule',
          1,
        ),
      ]) {
        final installation = registry.installations
            .where((entry) => entry.id == installationId)
            .singleOrNull;
        final assignment = registry.assignments
            .where((entry) => entry.id == assignmentId)
            .singleOrNull;
        if (installation == null ||
            assignment == null ||
            installation.kind != kind ||
            installation.packageId != packageId ||
            installation.version != '1.0.0' ||
            assignment.installationId != installation.id ||
            assignment.grantedViewCount != 1 ||
            assignment.grantedToolCount != tools) {
          throw const FormatException('Invalid Calendar setup links');
        }
      }
    }
  }

  final AgentRegistryView registry;
  final List<AgentCalendarView> views;
  final List<AgentCalendarSetupReceipt> setups;

  bool accessEnabled(AgentCalendarSetupReceipt setup) {
    final view = views.singleWhere((entry) => entry.handle == setup.viewHandle);
    final installationIds = {
      setup.toolInstallationId,
      setup.expertInstallationId,
    };
    final assignmentIds = {setup.toolAssignmentId, setup.expertAssignmentId};
    return view.enabled &&
        registry.installations
            .where((entry) => installationIds.contains(entry.id))
            .every((entry) => entry.enabled) &&
        registry.assignments
            .where((entry) => assignmentIds.contains(entry.id))
            .every((entry) => entry.enabled);
  }

  AgentCalendarSetupReceipt? receiptFor(AgentCalendarSetup request) {
    if (registry.personId != request.personId ||
        registry.instanceId != request.instanceId) {
      throw const FormatException('Calendar setup scope mismatch');
    }
    final receipt = setups
        .where((entry) => entry.setupId == request.setupId)
        .singleOrNull;
    if (receipt == null) return null;
    final view = views.singleWhere(
      (entry) => entry.handle == receipt.viewHandle,
    );
    if (receipt.expectedRevision != request.expectedRevision ||
        view.provider != request.provider ||
        view.calendarIds.length != request.calendarIds.length ||
        !List.generate(
          view.calendarIds.length,
          (index) => view.calendarIds[index] == request.calendarIds[index],
        ).every((equal) => equal)) {
      throw const FormatException('Calendar setup intent mismatch');
    }
    return receipt;
  }
}

final class AgentCalendarView {
  AgentCalendarView.fromJson(Map<String, dynamic> json)
    : handle = _identifier(json['handle']),
      personId = _identifier(json['person_id']),
      provider = _provider(json['provider']),
      deviceId = _deviceIdentifier(json['device_id']),
      calendarIds = _scope(json['calendar_ids'], canonical: true),
      enabled = _flag(json['enabled']);

  final String handle;
  final String personId;
  final String provider;
  final String deviceId;
  final List<String> calendarIds;
  final bool enabled;
}

final class AgentCalendarSetupReceipt {
  AgentCalendarSetupReceipt.fromJson(Map<String, dynamic> json)
    : setupId = _identifier(json['setup_id']),
      personId = _identifier(json['person_id']),
      expectedRevision = _counter(json['expected_revision']),
      viewHandle = _identifier(json['view_handle']),
      toolInstallationId = _identifier(json['tool_installation_id']),
      expertInstallationId = _identifier(json['expert_installation_id']),
      toolAssignmentId = _identifier(json['tool_assignment_id']),
      expertAssignmentId = _identifier(json['expert_assignment_id']);

  final String setupId;
  final String personId;
  final int expectedRevision;
  final String viewHandle;
  final String toolInstallationId;
  final String expertInstallationId;
  final String toolAssignmentId;
  final String expertAssignmentId;
}

String _identifier(Object? value) {
  if (value is! String ||
      value == '00000000-0000-0000-0000-000000000000' ||
      !RegExp(r'^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$')
          .hasMatch(value)) {
    throw const FormatException('Invalid Calendar setup identifier');
  }
  return value;
}

int _counter(Object? value) {
  if (value is! int || value < 0) {
    throw const FormatException('Invalid Calendar setup revision');
  }
  return value;
}

String _provider(Object? value) {
  if (!const {
    'event_kit',
    'google',
    'microsoft',
    'android',
    'fixture',
  }.contains(value)) {
    throw const FormatException('Invalid Calendar provider');
  }
  return value as String;
}

String _deviceIdentifier(Object? value) {
  if (value is! String || value.trim().isEmpty || value.length > 128) {
    throw const FormatException('Invalid Calendar device identifier');
  }
  return value;
}

bool _flag(Object? value) {
  if (value is! bool) {
    throw const FormatException('Invalid Calendar enablement');
  }
  return value;
}

List<String> _scope(Object? value, {required bool canonical}) {
  if (value is! List ||
      value.isEmpty ||
      value.length > 4 ||
      value.any(
        (entry) =>
            entry is! String ||
            entry.trim().isEmpty ||
            utf8.encode(entry).length > 512 ||
            utf8.decode(utf8.encode(entry)) != entry,
      )) {
    throw const FormatException('Invalid Calendar scope');
  }
  final identifiers = List<String>.from(value);
  final sorted = [...identifiers]..sort(_compareIdentifiers);
  if (sorted.toSet().length != sorted.length ||
      (canonical &&
          List.generate(
            sorted.length,
            (index) => sorted[index] != identifiers[index],
          ).any((different) => different))) {
    throw const FormatException('Invalid canonical Calendar scope');
  }
  return List.unmodifiable(sorted);
}

int _compareIdentifiers(String first, String second) {
  final firstBytes = utf8.encode(first);
  final secondBytes = utf8.encode(second);
  for (
    var index = 0;
    index < firstBytes.length && index < secondBytes.length;
    index++
  ) {
    final comparison = firstBytes[index].compareTo(secondBytes[index]);
    if (comparison != 0) return comparison;
  }
  return firstBytes.length.compareTo(secondBytes.length);
}

Map<String, dynamic> _map(Object? value) {
  if (value is! Map) {
    throw const FormatException('Invalid Calendar Expert object');
  }
  return Map<String, dynamic>.from(value);
}

Iterable<Map<String, dynamic>> _entries(Object? value, int maximum) {
  if (value is! List || value.length > maximum) {
    throw const FormatException('Invalid Calendar Expert entries');
  }
  return value.map(_map);
}
