import 'package:flutter/foundation.dart';

import '../agent_calendar_experts.dart';
import '../agent_registry.dart';
import '../agent_vault_gateway.dart';
import '../../day_canvas/domain/day_models.dart';
import 'agent_registry_controller.dart';

final class AgentCalendarExpertController extends ChangeNotifier {
  AgentCalendarExpertController({
    required this.gateway,
    required this.registryGateway,
    required this.registryController,
    required this.personId,
    required this.canOperate,
    required this.onFatalFailure,
  });

  final AgentCalendarExpertGateway? gateway;
  final AgentRegistryGateway? registryGateway;
  final AgentRegistryController registryController;
  final String personId;
  final bool Function() canOperate;
  final void Function(String failure) onFatalFailure;

  AgentCalendarExperts? experts;
  String? failure;
  AgentCalendarSetup? _pendingSetup;
  bool busy = false;

  AgentCalendarSetup? get pendingSetup => _pendingSetup;
  bool get available => gateway != null && registryGateway != null;
  bool get canManage =>
      available && !busy && !registryController.busy && canOperate();

  Future<void> load() =>
      _operation(() => gateway!.readCalendarExperts(personId));

  Future<CalendarSubjectPreview?> previewCalendarSubject({
    required String provider,
    required String deviceId,
    required String connectionId,
    required List<String> calendarIds,
    required String connectionScope,
    required int connectionRevision,
    required CalendarSourceAuthority sourceAuthority,
  }) async {
    if (!canManage || !_nativeProvider(provider)) return null;
    try {
      final preview = await gateway!.previewCalendarSubject(
        CalendarSubjectPreviewRequest(
          personId: personId,
          provider: provider,
          deviceId: deviceId,
          connectionId: connectionId,
          calendarIds: calendarIds,
          connectionScope: connectionScope,
          connectionRevision: connectionRevision,
          sourceAuthority: sourceAuthority,
        ),
      );
      if (!_previewMatches(
        preview,
        provider: provider,
        deviceId: deviceId,
        calendarIds: calendarIds,
        connectionScope: connectionScope,
        sourceAuthority: sourceAuthority,
      )) {
        throw const FormatException('Calendar preview scope mismatch');
      }
      return preview;
    } on Object catch (error) {
      failure = error is AgentVaultException
          ? error.failure
          : 'storage_unavailable';
      notifyListeners();
      return null;
    }
  }

  Future<void> install({
    required String setupId,
    required String provider,
    required String deviceId,
    required List<String> calendarIds,
    required String connectionScope,
    required int connectionRevision,
    required CalendarSourceAuthority sourceAuthority,
    CalendarSubjectPreview? reviewedPreview,
  }) async {
    final current = experts;
    if (!canManage || current == null || _pendingSetup != null) {
      return;
    }
    CalendarSubjectPreview? preview = reviewedPreview;
    busy = true;
    failure = null;
    notifyListeners();
    if (_nativeProvider(provider)) {
      try {
        _pendingSetup = AgentCalendarSetup(
          personId: personId,
          instanceId: current.registry.instanceId,
          expectedRevision: current.registry.revision,
          setupId: setupId,
          provider: provider,
          deviceId: deviceId,
          calendarIds: calendarIds,
          connectionScope: connectionScope,
          connectionRevision: connectionRevision,
          sourceAuthority: sourceAuthority,
        );
      } on FormatException {
        busy = false;
        failure = 'invalid_input';
        notifyListeners();
        return;
      }
      if (preview == null ||
          !_previewMatches(
            preview,
            provider: provider,
            deviceId: deviceId,
            calendarIds: calendarIds,
            connectionScope: connectionScope,
            sourceAuthority: sourceAuthority,
          )) {
        _pendingSetup = null;
        busy = false;
        failure = 'access_review_required';
        notifyListeners();
        return;
      }
      if (!canOperate()) {
        _pendingSetup = null;
        busy = false;
        notifyListeners();
        return;
      }
    }
    try {
      _pendingSetup = AgentCalendarSetup(
        personId: personId,
        instanceId: current.registry.instanceId,
        expectedRevision: current.registry.revision,
        setupId: setupId,
        provider: provider,
        deviceId: deviceId,
        calendarIds: calendarIds,
        connectionScope: connectionScope,
        connectionRevision: preview?.connectionRevision ?? connectionRevision,
        sourceAuthority: preview?.sourceAuthority ?? sourceAuthority,
        reviewedNativeSubjectFingerprint: preview?.nativeSubjectFingerprint,
      );
    } on FormatException {
      busy = false;
      failure = 'invalid_input';
      notifyListeners();
      return;
    }
    busy = false;
    await retrySetup();
  }

  Future<void> retrySetup() async {
    final pending = _pendingSetup;
    if (pending == null) return;
    await _operation(
      () => gateway!.installCalendarExpert(pending),
      submitted: pending,
    );
  }

  void discardUncommittedSetup() {
    final current = experts;
    final pending = _pendingSetup;
    if (!canManage ||
        current == null ||
        pending == null ||
        current.registry.instanceId != pending.instanceId ||
        current.setups.any((entry) => entry.setupId == pending.setupId)) {
      return;
    }
    _pendingSetup = null;
    notifyListeners();
  }

  Future<void> configureView(String handle, bool enabled) async {
    final current = experts;
    if (current == null || _pendingSetup != null) return;
    final before = current.views
        .where((entry) => entry.handle == handle)
        .singleOrNull;
    if (before == null) return;
    await _operation(() async {
      final configured = await registryGateway!.configureRegistry(
        current.registry,
        target: AgentRegistryTarget.calendarView,
        id: handle,
        enabled: enabled,
      );
      final next = await gateway!.readCalendarExperts(personId);
      final updated = next.views
          .where((entry) => entry.handle == handle)
          .singleOrNull;
      if (configured.instanceId != current.registry.instanceId ||
          configured.revision != current.registry.revision + 1 ||
          next.registry.instanceId != configured.instanceId ||
          next.registry.revision != configured.revision ||
          updated == null ||
          updated.enabled != enabled ||
          updated.provider != before.provider ||
          updated.deviceId != before.deviceId ||
          updated.sourceAuthority != before.sourceAuthority ||
          !listEquals(updated.calendarIds, before.calendarIds)) {
        throw const FormatException('Calendar configuration mismatch');
      }
      return next;
    });
  }

  Future<void> setCalendarAccessEnabled(String setupId, bool enabled) =>
      _configureCalendarAccess(
        setupId,
        operation: AgentCalendarAccessOperation.setEnabled,
        enabled: enabled,
      );

  Future<void> changeCalendarAccessScope({
    required String setupId,
    required String provider,
    required String deviceId,
    required List<String> calendarIds,
    required String connectionScope,
    required int connectionRevision,
    required CalendarSourceAuthority sourceAuthority,
    CalendarSubjectPreview? reviewedPreview,
  }) async {
    await _configureCalendarAccess(
      setupId,
      operation: AgentCalendarAccessOperation.setScope,
      provider: provider,
      deviceId: deviceId,
      calendarIds: calendarIds,
      connectionScope: connectionScope,
      connectionRevision: connectionRevision,
      sourceAuthority: sourceAuthority,
      reviewedPreview: reviewedPreview,
    );
  }

  Future<void> removeCalendarAccess(String setupId) => _configureCalendarAccess(
    setupId,
    operation: AgentCalendarAccessOperation.remove,
  );

  Future<void> _configureCalendarAccess(
    String setupId, {
    required AgentCalendarAccessOperation operation,
    bool? enabled,
    String? provider,
    String? deviceId,
    List<String>? calendarIds,
    String? connectionScope,
    int? connectionRevision,
    CalendarSourceAuthority? sourceAuthority,
    CalendarSubjectPreview? reviewedPreview,
  }) async {
    final current = experts;
    if (current == null ||
        _pendingSetup != null ||
        !current.setups.any((entry) => entry.setupId == setupId)) {
      return;
    }
    await _operation(() async {
      CalendarSubjectPreview? preview;
      if (operation == AgentCalendarAccessOperation.setScope &&
          _nativeProvider(provider!)) {
        preview = reviewedPreview;
        if (preview == null ||
            !_previewMatches(
              preview,
              provider: provider,
              deviceId: deviceId!,
              calendarIds: calendarIds!,
              connectionScope: connectionScope!,
              sourceAuthority: sourceAuthority!,
            )) {
          throw const AgentVaultException('access_review_required');
        }
      }
      final request = AgentCalendarAccessRequest(
        personId: personId,
        instanceId: current.registry.instanceId,
        expectedRevision: current.registry.revision,
        setupId: setupId,
        operation: operation,
        enabled: enabled,
        provider: provider,
        deviceId: deviceId,
        sourceAuthority: preview?.sourceAuthority ?? sourceAuthority,
        reviewedNativeSubjectFingerprint: preview?.nativeSubjectFingerprint,
        calendarIds: calendarIds,
        connectionScope: connectionScope,
        connectionRevision: preview?.connectionRevision ?? connectionRevision,
      );
      final next = await gateway!.configureCalendarAccess(request);
      if (next.registry.instanceId != current.registry.instanceId ||
          next.registry.revision != current.registry.revision + 1) {
        throw const FormatException('Calendar access configuration mismatch');
      }
      final setup = next.setups
          .where((entry) => entry.setupId == setupId)
          .singleOrNull;
      switch (operation) {
        case AgentCalendarAccessOperation.setEnabled:
          if (setup == null || next.accessEnabled(setup) != enabled) {
            throw const FormatException('Calendar access state mismatch');
          }
        case AgentCalendarAccessOperation.setScope:
          final view = setup == null
              ? null
              : next.views
                    .where((entry) => entry.handle == setup.viewHandle)
                    .singleOrNull;
          if (view == null ||
              view.provider != provider ||
              view.deviceId != deviceId ||
              view.sourceAuthority !=
                  (preview?.sourceAuthority ?? sourceAuthority) ||
              view.connectionScope != connectionScope ||
              view.connectionRevision !=
                  (preview?.connectionRevision ?? connectionRevision) ||
              setup!.connectionScope != connectionScope ||
              setup.connectionRevision !=
                  (preview?.connectionRevision ?? connectionRevision) ||
              setup.sourceAuthority !=
                  (preview?.sourceAuthority ?? sourceAuthority) ||
              setup.reviewedNativeSubjectFingerprint !=
                  preview?.nativeSubjectFingerprint ||
              !listEquals(view.calendarIds, [...calendarIds!]..sort())) {
            throw const FormatException('Calendar access scope mismatch');
          }
        case AgentCalendarAccessOperation.remove:
          if (setup != null) {
            throw const FormatException('Calendar access removal mismatch');
          }
      }
      return next;
    });
  }

  bool _nativeProvider(String? provider) =>
      provider == 'event_kit' || provider == 'android';

  bool _previewMatches(
    CalendarSubjectPreview preview, {
    required String provider,
    required String deviceId,
    required List<String> calendarIds,
    required String connectionScope,
    required CalendarSourceAuthority sourceAuthority,
  }) {
    final expectedIds = [...calendarIds]..sort();
    return preview.provider == provider &&
        preview.deviceId == deviceId &&
        listEquals(preview.calendarIds, expectedIds) &&
        preview.connectionScope == connectionScope &&
        preview.sourceAuthority == sourceAuthority;
  }

  Future<void> _operation(
    Future<AgentCalendarExperts> Function() operation, {
    AgentCalendarSetup? submitted,
  }) async {
    if (!canManage) return;
    busy = true;
    failure = null;
    notifyListeners();
    try {
      final result = await operation();
      if (!canOperate()) return;
      if (result.registry.personId != personId) {
        throw const FormatException('Calendar Person mismatch');
      }
      if (submitted != null && result.receiptFor(submitted) == null) {
        throw const FormatException('Missing Calendar setup receipt');
      }
      if (_pendingSetup case final pending?) {
        if (result.receiptFor(pending) != null) _pendingSetup = null;
      }
      experts = result;
      registryController.replace(result.registry);
    } on Object catch (error) {
      if (!canOperate()) return;
      experts = null;
      registryController.clear();
      failure = error is AgentVaultException
          ? error.failure
          : 'storage_unavailable';
      if (failure == 'vault_unavailable' || failure == 'interrupted') {
        onFatalFailure(failure!);
      }
    } finally {
      busy = false;
      notifyListeners();
    }
  }

  void clear() {
    experts = null;
    failure = null;
    _pendingSetup = null;
    notifyListeners();
  }
}
