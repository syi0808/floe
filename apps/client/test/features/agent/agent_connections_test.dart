import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/features/agent/agent_connection_settings.dart';
import 'package:floe_client/features/agent/agent_connections.dart';
import 'package:floe_client/features/agent/agent_vault_gateway.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'native gateway reads strict provider-neutral connection snapshots',
    () async {
      Map<String, Object?>? submittedAction;
      final gateway = NativeAgentVaultGateway((request) async {
        final operation = Map<String, Object?>.from(
          request['operation']! as Map,
        );
        if (operation['kind'] == 'submit') {
          submittedAction = Map<String, Object?>.from(
            operation['action']! as Map,
          );
        }
        return {
          'request_id': request['request_id'],
          'events': <Object?>[],
          'next_sequence': 0,
          'done': true,
          'state': 'ready',
          'connections': [_connection],
          'failure': null,
        };
      }, deviceId: 'test-device');

      final connections = await gateway.readConnections('person-1');

      expect(submittedAction, {'kind': 'connections'});
      expect(connections.single.descriptor.provider, 'apple_event_kit');
      expect(connections.single.state, AgentConnectionState.degraded);
      expect(connections.single.views.single.itemCount, 2);
      expect(connections.single.failure?.kind, 'partial_fetch');
      expect(connections.single.usable, isTrue);
    },
  );

  test('connection parser rejects authority and projection escalation', () {
    final descriptor = Map<String, Object?>.from(
      _connection['descriptor']! as Map,
    );
    final capabilities = List<Object?>.from(
      descriptor['capabilities']! as List,
    );
    expect(
      () => AgentConnection.fromJson({
        ..._connection,
        'descriptor': {
          ...descriptor,
          'capabilities': [
            ...capabilities,
            {
              'schema_version': 1,
              'id': 'calendar.events.delete',
              'version': '1.0.0',
              'authority': 'act',
              'required_scopes': ['calendar.events.read'],
              'output_view_id': 'calendar.timeline',
            },
          ],
        },
      }),
      throwsFormatException,
    );
    final view = Map<String, Object?>.from(
      (_connection['views']! as List).single as Map,
    );
    expect(
      () => AgentConnection.fromJson({
        ..._connection,
        'views': [
          {...view, 'item_count': 129},
        ],
      }),
      throwsFormatException,
    );
  });

  testWidgets(
    'settings explains degraded source without implying action access',
    (tester) async {
      final connections = [
        AgentConnection.fromJson(Map<String, dynamic>.from(_connection)),
      ];

      await tester.pumpWidget(
        MaterialApp(
          theme: FloeTheme.light,
          home: Scaffold(
            body: AgentConnectionSettings(
              connections: connections,
              loading: false,
              failed: false,
              onRefresh: () async {},
            ),
          ),
        ),
      );

      expect(find.text('Apple Calendar'), findsOneWidget);
      expect(find.text('Partial'), findsOneWidget);
      expect(
        find.text('Some source data could not be refreshed.'),
        findsOneWidget,
      );
      expect(
        find.textContaining('Actions require separate approval.'),
        findsOneWidget,
      );
    },
  );
}

const _connection = <String, Object?>{
  'descriptor': {
    'schema_version': 1,
    'id': 'calendar.event_kit',
    'version': '1.0.0',
    'provider': 'apple_event_kit',
    'execution': {'kind': 'device', 'device_id': 'local-macos'},
    'capabilities': [
      {
        'schema_version': 1,
        'id': 'calendar.events.read',
        'version': '1.0.0',
        'authority': 'observe',
        'required_scopes': ['calendar.events.read'],
        'output_view_id': 'calendar.timeline',
      },
      {
        'schema_version': 1,
        'id': 'calendar.events.create',
        'version': '1.0.0',
        'authority': 'act',
        'required_scopes': ['calendar.events.write'],
      },
    ],
    'views': [
      {
        'schema_version': 1,
        'id': 'calendar.timeline',
        'version': '1.0.0',
        'data_class': 'personal',
        'retention': 'mirror',
        'freshness_ttl_ms': 300000,
        'max_items': 128,
        'max_bytes': 65536,
        'provenance_required': true,
      },
    ],
  },
  'connection': {
    'schema_version': 1,
    'connector_id': 'calendar.event_kit',
    'state': 'degraded',
    'granted_scopes': ['calendar.events.read'],
    'observed_at_unix_ms': 2000000,
    'last_success_at_unix_ms': 1999000,
    'last_failure': {'kind': 'partial_fetch', 'observed_at_unix_ms': 2000000},
  },
  'views': [
    {
      'schema_version': 1,
      'view_id': 'calendar.timeline',
      'source_handle': 'calendar.timeline:opaque',
      'observed_at_unix_ms': 1999000,
      'expires_at_unix_ms': 2299000,
      'item_count': 2,
      'byte_count': 512,
      'provenance_count': 2,
    },
  ],
};
