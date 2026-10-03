import 'package:floe_client/app/runtime/owner_failure.dart';

import 'dart:convert';

sealed class ConnectionRef {
  ConnectionRef(this.value) {
    if (!RegExp(
          r'^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$',
        ).hasMatch(value) ||
        value == '00000000-0000-0000-0000-000000000000') {
      throw const FormatException('Invalid owner reference.');
    }
  }
  final String value;
  @override
  bool operator ==(Object other) =>
      other.runtimeType == runtimeType &&
      other is ConnectionRef &&
      other.value == value;
  @override
  int get hashCode => Object.hash(runtimeType, value);
}

final class GatewayRef extends ConnectionRef {
  GatewayRef(super.value);
}

final class GatewayTargetRef extends ConnectionRef {
  GatewayTargetRef(super.value);
}

final class ConnectionOperationRef extends ConnectionRef {
  ConnectionOperationRef(super.value);
}

final class SourceRef extends ConnectionRef {
  SourceRef(super.value);
}

final class IntegrationRef extends ConnectionRef {
  IntegrationRef(super.value);
}

final class ResourceRef extends ConnectionRef {
  ResourceRef(super.value);
}

final class LaunchActionRef extends ConnectionRef {
  LaunchActionRef(super.value);
}

sealed class ConnectionReviewRef {
  ConnectionReviewRef._(Map<String, dynamic> value)
    : id = _uuid(value['id']),
      revision = _revision(value['revision']),
      digest = _digest(value['digest']);
  final String id;
  final int revision;
  final String digest;
  Map<String, Object?> toJson() => {
    'id': id,
    'revision': revision,
    'digest': digest,
  };
}

final class SourceReviewRef extends ConnectionReviewRef {
  SourceReviewRef.fromJson(Object? value)
    : super._(connectionObject(value, {'id', 'revision', 'digest'}));
}

final class ObserveReviewRef extends ConnectionReviewRef {
  ObserveReviewRef.fromJson(Object? value)
    : super._(connectionObject(value, {'id', 'revision', 'digest'}));
}

final class IntegrationReviewRef extends ConnectionReviewRef {
  IntegrationReviewRef.fromJson(Object? value)
    : super._(connectionObject(value, {'id', 'revision', 'digest'}));
}

enum SourceProcessing {
  deviceOnly('device_only'),
  gatewayAllowed('gateway_allowed');

  const SourceProcessing(this.wire);
  final String wire;
  static SourceProcessing parse(Object? value) => values.firstWhere(
    (entry) => entry.wire == value,
    orElse: () => throw const FormatException('Invalid source processing.'),
  );
}

Map<String, dynamic> connectionObject(
  Object? value,
  Set<String> required, [
  Set<String> optional = const {},
]) {
  if (value is! Map || value.keys.any((key) => key is! String)) {
    throw const FormatException('Invalid Connections object.');
  }
  final result = Map<String, dynamic>.from(value);
  if (!result.keys.toSet().containsAll(required) ||
      result.keys.toSet().difference({...required, ...optional}).isNotEmpty ||
      optional.any((key) => result.containsKey(key) && result[key] == null)) {
    throw const FormatException('Invalid Connections fields.');
  }
  return result;
}

String _text(Object? value, [int maximumBytes = 256]) {
  if (value is! String ||
      value.isEmpty ||
      utf8.encode(value).length > maximumBytes ||
      value.runes.any((rune) => rune < 32 || rune >= 127 && rune <= 159)) {
    throw const FormatException('Invalid Connections text.');
  }
  return value;
}

String _uuid(Object? value) => GatewayRef(_text(value, 36)).value;
String _digest(Object? value) {
  final text = _text(value, 64);
  if (!RegExp(r'^[0-9a-f]{64}$').hasMatch(text))
    throw const FormatException('Invalid review digest.');
  return text;
}

int _revision(Object? value) {
  if (value is! int || value < 1)
    throw const FormatException('Invalid owner revision.');
  return value;
}

bool _boolean(Object? value) {
  if (value is! bool) throw const FormatException('Invalid owner flag.');
  return value;
}

DateTime _time(Object? value) {
  final text = _text(value, 64);
  final time = DateTime.tryParse(text);
  if (time == null || !text.endsWith('Z'))
    throw const FormatException('Invalid owner timestamp.');
  return time.toUtc();
}

String _choice(Object? value, Set<String> choices) {
  final text = _text(value);
  if (!choices.contains(text))
    throw const FormatException('Unknown owner state.');
  return text;
}

List<T> _list<T>(
  Object? value,
  T Function(Object?) decode, [
  int maximum = 256,
]) {
  if (value is! List || value.length > maximum)
    throw const FormatException('Invalid owner list.');
  return List<T>.unmodifiable(value.map(decode));
}

Set<String> _actions(Object? value, [Set<String>? permitted]) {
  final values = _list(value, (value) => _text(value, 64));
  if (values.toSet().length != values.length ||
      permitted != null &&
          values.any((action) => !permitted.contains(action))) {
    throw const FormatException('Unknown owner action.');
  }
  return Set<String>.unmodifiable(values);
}

int _delay(Object? value) {
  if (value is! int || value < 0 || value > 60000)
    throw const FormatException('Invalid observation delay.');
  return value;
}

Uri _launchUrl(Object? value) {
  final uri = Uri.tryParse(_text(value, 2048));
  if (uri == null ||
      uri.scheme != 'http' ||
      uri.host != '127.0.0.1' ||
      uri.userInfo.isNotEmpty ||
      uri.hasQuery ||
      uri.hasFragment ||
      !(uri.path == '/manage' ||
          RegExp(r'^/manage/setup/[0-9a-f-]{36}$').hasMatch(uri.path))) {
    throw const FormatException('Invalid management launch.');
  }
  return uri;
}

final class GatewaySetup {
  const GatewaySetup({
    required this.targetRef,
    required this.displayAddress,
    required this.expiresAt,
  });
  factory GatewaySetup.fromJson(Object? value) {
    final j = connectionObject(value, {
      'target_ref',
      'display_address',
      'expires_at',
    }, {});
    return GatewaySetup(
      targetRef: GatewayTargetRef(_uuid(j['target_ref'])),
      displayAddress: _text(j['display_address'], 2048),
      expiresAt: _time(j['expires_at']),
    );
  }
  final GatewayTargetRef targetRef;
  final String displayAddress;
  final DateTime expiresAt;
}

final class GatewaySummary {
  const GatewaySummary({
    required this.gatewayRef,
    required this.revision,
    required this.displayName,
    required this.state,
    required this.allowedActions,
    required this.remoteRevocationPending,
    this.failure,
  });
  factory GatewaySummary.fromJson(Object? value) {
    final j = connectionObject(
      value,
      {
        'gateway_ref',
        'revision',
        'display_name',
        'state',
        'allowed_actions',
        'remote_revocation_pending',
      },
      {'failure'},
    );
    return GatewaySummary(
      gatewayRef: GatewayRef(_uuid(j['gateway_ref'])),
      revision: _revision(j['revision']),
      displayName: _text(j['display_name']),
      state: _choice(j['state'], {
        'unpaired',
        'paired',
        'repair_required',
        'forgotten',
      }),
      allowedActions: _actions(j['allowed_actions'], {
        'pair',
        'forget',
        'manage',
      }),
      remoteRevocationPending: _boolean(j['remote_revocation_pending']),
      failure: j['failure'] == null
          ? null
          : OwnerFailure.fromJson(j['failure']),
    );
  }
  final GatewayRef gatewayRef;
  final int revision;
  final String displayName;
  final String state;
  final Set<String> allowedActions;
  final bool remoteRevocationPending;
  final OwnerFailure? failure;
}

final class ResourceSummary {
  const ResourceSummary({required this.resourceRef, required this.label});
  factory ResourceSummary.fromJson(Object? value) {
    final j = connectionObject(value, {'resource_ref', 'label'}, {});
    return ResourceSummary(
      resourceRef: ResourceRef(_uuid(j['resource_ref'])),
      label: _text(j['label']),
    );
  }
  final ResourceRef resourceRef;
  final String label;
}

final class ResourceChoice {
  const ResourceChoice({
    required this.resourceRef,
    required this.label,
    required this.selected,
  });
  factory ResourceChoice.fromJson(Object? value) {
    final j = connectionObject(value, {
      'resource_ref',
      'label',
      'selected',
    }, {});
    return ResourceChoice(
      resourceRef: ResourceRef(_uuid(j['resource_ref'])),
      label: _text(j['label']),
      selected: _boolean(j['selected']),
    );
  }
  final ResourceRef resourceRef;
  final String label;
  final bool selected;
}

final class SourceSummary {
  const SourceSummary({
    required this.sourceRef,
    required this.revision,
    required this.displayLabels,
    required this.availability,
    this.lastObservedAt,
    required this.selectedResources,
    required this.observeState,
    required this.allowedActions,
  });
  factory SourceSummary.fromJson(Object? value) {
    final j = connectionObject(
      value,
      {
        'source_ref',
        'revision',
        'display_labels',
        'availability',
        'selected_resources',
        'observe_state',
        'allowed_actions',
      },
      {'last_observed_at'},
    );
    return SourceSummary(
      sourceRef: SourceRef(_uuid(j['source_ref'])),
      revision: _revision(j['revision']),
      displayLabels: _list(j['display_labels'], _text),
      availability: _choice(j['availability'], {
        'available',
        'unavailable',
        'permission_required',
        'identity_changed',
        'disconnected',
      }),
      lastObservedAt: j['last_observed_at'] == null
          ? null
          : _time(j['last_observed_at']),
      selectedResources: _list(
        j['selected_resources'],
        ResourceSummary.fromJson,
      ),
      observeState: _choice(j['observe_state'], {
        'disabled',
        'enabled',
        'paused',
        'review_required',
      }),
      allowedActions: _actions(j['allowed_actions'], {
        'configure',
        'disconnect',
        'prepare_observe_review',
        'pause_observe',
      }),
    );
  }
  final SourceRef sourceRef;
  final int revision;
  final List<String> displayLabels;
  final String availability;
  final DateTime? lastObservedAt;
  final List<ResourceSummary> selectedResources;
  final String observeState;
  final Set<String> allowedActions;
}

final class IntegrationSummary {
  const IntegrationSummary({
    required this.integrationRef,
    required this.revision,
    required this.displayName,
    required this.category,
    required this.state,
    required this.capabilities,
    this.source,
  });
  factory IntegrationSummary.fromJson(Object? value) {
    final j = connectionObject(
      value,
      {
        'integration_ref',
        'revision',
        'display_name',
        'category',
        'state',
        'capabilities',
      },
      {'source'},
    );
    return IntegrationSummary(
      integrationRef: IntegrationRef(_uuid(j['integration_ref'])),
      revision: _revision(j['revision']),
      displayName: _text(j['display_name']),
      category: _text(j['category'], 64),
      state: _choice(j['state'], {
        'available',
        'unavailable',
        'connected',
        'connecting',
        'error',
      }),
      capabilities: _actions(j['capabilities'], {
        'prepare_review',
        'configure',
        'disconnect',
      }),
      source: j['source'] == null ? null : SourceSummary.fromJson(j['source']),
    );
  }
  final IntegrationRef integrationRef;
  final int revision;
  final String displayName;
  final String category;
  final String state;
  final Set<String> capabilities;
  final SourceSummary? source;
}

final class ConnectionsOverview {
  const ConnectionsOverview({
    required this.revision,
    required this.gateways,
    required this.integrations,
    required this.sources,
  });
  factory ConnectionsOverview.fromJson(Object? value) {
    final j = connectionObject(value, {
      'revision',
      'gateways',
      'integrations',
      'sources',
    }, {});
    return ConnectionsOverview(
      revision: _revision(j['revision']),
      gateways: _list(j['gateways'], GatewaySummary.fromJson),
      integrations: _list(j['integrations'], IntegrationSummary.fromJson),
      sources: _list(j['sources'], SourceSummary.fromJson),
    );
  }
  final int revision;
  final List<GatewaySummary> gateways;
  final List<IntegrationSummary> integrations;
  final List<SourceSummary> sources;
}

final class PairingSnapshot {
  const PairingSnapshot({
    required this.operationRef,
    required this.revision,
    required this.state,
    this.displayCode,
    this.expiresAt,
    this.gateway,
    required this.allowedActions,
    this.failure,
    this.nextObservationAfterMs,
  });
  factory PairingSnapshot.fromJson(Object? value) {
    final j = connectionObject(
      value,
      {'operation_ref', 'revision', 'state', 'allowed_actions'},
      {
        'display_code',
        'expires_at',
        'gateway',
        'failure',
        'next_observation_after_ms',
      },
    );
    if (j['display_code'] is String &&
        (j['display_code'] as String).codeUnits.any((value) => value > 127)) {
      throw const FormatException('Invalid pairing display code.');
    }
    if (j['state'] == 'connected' &&
        (j['gateway'] is! Map || (j['gateway'] as Map)['state'] != 'paired')) {
      throw const FormatException('Pairing is not committed.');
    }
    return PairingSnapshot(
      operationRef: ConnectionOperationRef(_uuid(j['operation_ref'])),
      revision: _revision(j['revision']),
      state: _choice(j['state'], {
        'starting',
        'awaiting_local_confirmation',
        'awaiting_gateway_approval',
        'cancelling',
        'connected',
        'rejected',
        'expired',
        'cancelled',
        'repair_required',
      }),
      displayCode: j['display_code'] == null
          ? null
          : _text(j['display_code'], 32),
      expiresAt: j['expires_at'] == null ? null : _time(j['expires_at']),
      gateway: j['gateway'] == null
          ? null
          : GatewaySummary.fromJson(j['gateway']),
      allowedActions: _actions(j['allowed_actions'], {
        'confirm',
        'cancel',
        'reobserve',
      }),
      failure: j['failure'] == null
          ? null
          : OwnerFailure.fromJson(j['failure']),
      nextObservationAfterMs: j['next_observation_after_ms'] == null
          ? null
          : _delay(j['next_observation_after_ms']),
    );
  }
  final ConnectionOperationRef operationRef;
  final int revision;
  final String state;
  final String? displayCode;
  final DateTime? expiresAt;
  final GatewaySummary? gateway;
  final Set<String> allowedActions;
  final OwnerFailure? failure;
  final int? nextObservationAfterMs;
}

final class ProcessingScope {
  const ProcessingScope({required this.processing, required this.categories});
  factory ProcessingScope.fromJson(Object? value) {
    final tagged = connectionObject(value, {'kind'}, {'categories'});
    final processing = SourceProcessing.parse(tagged['kind']);
    if (processing == SourceProcessing.deviceOnly) {
      connectionObject(value, {'kind'});
      return const ProcessingScope(
        processing: SourceProcessing.deviceOnly,
        categories: [],
      );
    }
    final j = connectionObject(value, {'kind', 'categories'});
    final categories = _list(
      j['categories'],
      (v) => _choice(v, {'metadata', 'content', 'derived'}),
      3,
    );
    if (categories.isEmpty || categories.toSet().length != categories.length) {
      throw const FormatException('Invalid processing categories.');
    }
    return ProcessingScope(processing: processing, categories: categories);
  }
  final SourceProcessing processing;
  final List<String> categories;
  String get label => processing == SourceProcessing.deviceOnly
      ? 'This device only'
      : 'This device and verified Gateway (${categories.join(', ')})';
}

final class ViewProcessingDisclosure {
  const ViewProcessingDisclosure({
    required this.viewId,
    required this.dataClass,
    required this.dataCategories,
    required this.current,
    required this.requested,
  });
  factory ViewProcessingDisclosure.fromJson(Object? value) {
    final j = connectionObject(value, {
      'view_id',
      'data_class',
      'data_categories',
      'current',
      'requested',
    });
    final categories = _list(
      j['data_categories'],
      (v) => _choice(v, {'metadata', 'content', 'derived'}),
      3,
    );
    if (categories.toSet().length != categories.length ||
        categories.isEmpty !=
            (j['current'] == null && j['requested'] == null)) {
      throw const FormatException('Invalid reviewed data categories.');
    }
    return ViewProcessingDisclosure(
      viewId: _text(j['view_id']),
      dataClass: _choice(j['data_class'], {'personal', 'highly_sensitive'}),
      dataCategories: categories,
      current: j['current'] == null
          ? null
          : ProcessingScope.fromJson(j['current']),
      requested: j['requested'] == null
          ? null
          : ProcessingScope.fromJson(j['requested']),
    );
  }
  final String viewId;
  final String dataClass;
  final List<String> dataCategories;
  final ProcessingScope? current;
  final ProcessingScope? requested;
  String get dataClassLabel =>
      dataClass == 'highly_sensitive' ? 'Highly sensitive' : 'Personal';
  bool get isDerivedHealth =>
      viewId == 'wellbeing.derived' &&
      dataClass == 'highly_sensitive' &&
      dataCategories.length == 1 &&
      dataCategories.single == 'derived';
  bool get expandsGateway =>
      requested?.processing == SourceProcessing.gatewayAllowed &&
      (current?.processing != SourceProcessing.gatewayAllowed ||
          requested!.categories.any(
            (category) => !current!.categories.contains(category),
          ));
}

final class ProcessingDisclosure {
  const ProcessingDisclosure({required this.views});
  factory ProcessingDisclosure.fromJson(
    Object? value, {
    required bool observe,
  }) {
    final j = connectionObject(value, {'views'});
    final views = _list(j['views'], ViewProcessingDisclosure.fromJson, 64);
    if (views.map((view) => view.viewId).toSet().length != views.length ||
        (observe && views.isEmpty) ||
        views.any((view) => (view.requested != null) != observe)) {
      throw const FormatException('Invalid per-view processing disclosure.');
    }
    return ProcessingDisclosure(views: views);
  }
  final List<ViewProcessingDisclosure> views;
}

final class SourceReview {
  const SourceReview({
    required this.reviewRef,
    required this.sourceRef,
    required this.sourceRevision,
    required this.labels,
    required this.permittedChoices,
    required this.processingDisclosure,
    required this.expiresAt,
    required this.allowedActions,
  });
  factory SourceReview.fromJson(Object? value) {
    final j = connectionObject(value, {
      'review_ref',
      'source_ref',
      'source_revision',
      'labels',
      'permitted_choices',
      'processing_disclosure',
      'expires_at',
      'allowed_actions',
    }, {});
    return SourceReview(
      reviewRef: SourceReviewRef.fromJson(j['review_ref']),
      sourceRef: SourceRef(_uuid(j['source_ref'])),
      sourceRevision: _revision(j['source_revision']),
      labels: _list(j['labels'], _text),
      permittedChoices: _list(j['permitted_choices'], ResourceChoice.fromJson),
      processingDisclosure: ProcessingDisclosure.fromJson(
        j['processing_disclosure'],
        observe: false,
      ),
      expiresAt: _time(j['expires_at']),
      allowedActions: _actions(j['allowed_actions'], {'configure'}),
    );
  }
  final SourceReviewRef reviewRef;
  final SourceRef sourceRef;
  final int sourceRevision;
  final List<String> labels;
  final List<ResourceChoice> permittedChoices;
  final ProcessingDisclosure processingDisclosure;
  final DateTime expiresAt;
  final Set<String> allowedActions;
}

final class ObserveReview {
  const ObserveReview({
    required this.reviewRef,
    required this.sourceRef,
    required this.sourceRevision,
    required this.displayMembers,
    required this.processingDisclosure,
    required this.expiresAt,
    required this.allowedActions,
  });
  factory ObserveReview.fromJson(Object? value) {
    final j = connectionObject(value, {
      'review_ref',
      'source_ref',
      'source_revision',
      'display_members',
      'processing_disclosure',
      'expires_at',
      'allowed_actions',
    }, {});
    final members = _list(j['display_members'], _text, 64);
    final disclosure = ProcessingDisclosure.fromJson(
      j['processing_disclosure'],
      observe: true,
    );
    if (members.length != disclosure.views.length ||
        members.toSet().length != members.length ||
        disclosure.views.any((view) => !members.contains(view.viewId))) {
      throw const FormatException('Reviewed View scope mismatch.');
    }
    return ObserveReview(
      reviewRef: ObserveReviewRef.fromJson(j['review_ref']),
      sourceRef: SourceRef(_uuid(j['source_ref'])),
      sourceRevision: _revision(j['source_revision']),
      displayMembers: members,
      processingDisclosure: disclosure,
      expiresAt: _time(j['expires_at']),
      allowedActions: _actions(j['allowed_actions'], {'allow', 'decline'}),
    );
  }
  final ObserveReviewRef reviewRef;
  final SourceRef sourceRef;
  final int sourceRevision;
  final List<String> displayMembers;
  final ProcessingDisclosure processingDisclosure;
  final DateTime expiresAt;
  final Set<String> allowedActions;
}

sealed class IntegrationTarget {
  const IntegrationTarget();
  factory IntegrationTarget.fromJson(Object? value) {
    if (value is! Map)
      throw const FormatException('Invalid integration target.');
    return switch (value['kind']) {
      'device' => DeviceIntegrationTarget(
        _text(connectionObject(value, {'kind', 'device_id'})['device_id'], 128),
      ),
      'gateway' => GatewayIntegrationTarget.fromJson(
        connectionObject(value, {'kind', 'gateway_ref', 'gateway_revision'}),
      ),
      _ => throw const FormatException('Unknown integration target.'),
    };
  }
}

final class DeviceIntegrationTarget extends IntegrationTarget {
  const DeviceIntegrationTarget(this.deviceId);
  final String deviceId;
}

final class GatewayIntegrationTarget extends IntegrationTarget {
  GatewayIntegrationTarget.fromJson(Map<String, dynamic> value)
    : gatewayRef = GatewayRef(_uuid(value['gateway_ref'])),
      gatewayRevision = _revision(value['gateway_revision']);
  final GatewayRef gatewayRef;
  final int gatewayRevision;
}

final class IntegrationReview {
  const IntegrationReview({
    required this.reviewRef,
    required this.integrationRef,
    required this.catalogRevision,
    required this.target,
    required this.displayName,
    required this.setupKind,
    required this.expiresAt,
    required this.allowedActions,
  });
  factory IntegrationReview.fromJson(Object? value) {
    final j = connectionObject(value, {
      'review_ref',
      'integration_ref',
      'catalog_revision',
      'target',
      'display_name',
      'setup_kind',
      'expires_at',
      'allowed_actions',
    });
    final target = IntegrationTarget.fromJson(j['target']);
    final setup = _choice(j['setup_kind'], {
      'browser_authorization',
      'device_code',
      'gateway_managed_secret',
      'native_permission',
    });
    if ((setup == 'native_permission') != (target is DeviceIntegrationTarget)) {
      throw const FormatException(
        'Integration setup target does not match its kind.',
      );
    }
    return IntegrationReview(
      reviewRef: IntegrationReviewRef.fromJson(j['review_ref']),
      integrationRef: IntegrationRef(_uuid(j['integration_ref'])),
      catalogRevision: _revision(j['catalog_revision']),
      target: target,
      displayName: _text(j['display_name']),
      setupKind: setup,
      expiresAt: _time(j['expires_at']),
      allowedActions: _actions(j['allowed_actions'], {'start'}),
    );
  }
  final IntegrationReviewRef reviewRef;
  final IntegrationRef integrationRef;
  final int catalogRevision;
  final IntegrationTarget target;
  final String displayName;
  final String setupKind;
  final DateTime expiresAt;
  final Set<String> allowedActions;
}

final class LaunchAction {
  const LaunchAction({
    required this.actionRef,
    required this.purpose,
    required this.validatedUrl,
    required this.expiresAt,
  });
  factory LaunchAction.fromJson(Object? value) {
    final j = connectionObject(value, {
      'action_ref',
      'purpose',
      'validated_url',
      'expires_at',
    }, {});
    return LaunchAction(
      actionRef: LaunchActionRef(_uuid(j['action_ref'])),
      purpose: _choice(j['purpose'], {
        'manage_gateway',
        'authorize_integration',
      }),
      validatedUrl: _launchUrl(j['validated_url']),
      expiresAt: _time(j['expires_at']),
    );
  }
  final LaunchActionRef actionRef;
  final String purpose;
  final Uri validatedUrl;
  final DateTime expiresAt;
}

final class ConnectionOperationSnapshot {
  const ConnectionOperationSnapshot({
    required this.operationRef,
    required this.revision,
    required this.state,
    this.launchAction,
    this.displayCode,
    this.source,
    required this.allowedActions,
    this.failure,
    this.nextObservationAfterMs,
  });
  factory ConnectionOperationSnapshot.fromJson(Object? value) {
    final j = connectionObject(
      value,
      {'operation_ref', 'revision', 'state', 'allowed_actions'},
      {
        'launch_action',
        'display_code',
        'source',
        'failure',
        'next_observation_after_ms',
      },
    );
    if ({'failed', 'repair_required'}.contains(j['state']) &&
        j['failure'] == null) {
      throw const FormatException('Missing owner operation failure.');
    }
    return ConnectionOperationSnapshot(
      operationRef: ConnectionOperationRef(_uuid(j['operation_ref'])),
      revision: _revision(j['revision']),
      state: _choice(j['state'], {
        'pending',
        'running',
        'awaiting_user',
        'completed',
        'failed',
        'cancelled',
        'repair_required',
      }),
      launchAction: j['launch_action'] == null
          ? null
          : LaunchAction.fromJson(j['launch_action']),
      displayCode: j['display_code'] == null
          ? null
          : _text(j['display_code'], 32),
      source: j['source'] == null ? null : SourceSummary.fromJson(j['source']),
      allowedActions: _actions(j['allowed_actions'], {'cancel', 'reobserve'}),
      failure: j['failure'] == null
          ? null
          : OwnerFailure.fromJson(j['failure']),
      nextObservationAfterMs: j['next_observation_after_ms'] == null
          ? null
          : _delay(j['next_observation_after_ms']),
    );
  }
  final ConnectionOperationRef operationRef;
  final int revision;
  final String state;
  final LaunchAction? launchAction;
  final String? displayCode;
  final SourceSummary? source;
  final Set<String> allowedActions;
  final OwnerFailure? failure;
  final int? nextObservationAfterMs;
}
