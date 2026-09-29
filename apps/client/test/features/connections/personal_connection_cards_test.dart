import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/features/connections/application/connection_observe_gateway.dart';
import 'package:floe_client/features/connections/application/native_personal_source_gateway.dart';
import 'package:floe_client/features/connections/domain/connection_observe.dart';
import 'package:floe_client/features/connections/domain/source_connection.dart';
import 'package:floe_client/features/connections/presentation/personal_connection_cards.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:floe_client/l10n/app_localizations.dart';

void main() {
  testWidgets('Contacts source edit never reviews or re-enables Observe', (
    tester,
  ) async {
    final source = _SourceGateway('contacts.apple', ['A']);
    final observe = _ObserveGateway('active');
    await tester.pumpWidget(
      _host(
        PersonalContactsSourceCard(
          sourceGateway: source,
          observeGateway: observe,
          readContacts: () async => {
            'identities': [
              {'identity_handle': 'A', 'display_name': 'Alice'},
              {'identity_handle': 'B', 'display_name': 'Bob'},
            ],
          },
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(
      find.byKey(const ValueKey('personal-use-with-floe')),
      findsOneWidget,
    );
    expect(observe.calls, ['inspect']);
    await tester.tap(find.text('Bob'));
    await tester.tap(find.text('Save selection'));
    await tester.pumpAndSettle();
    expect(source.savedHandles, ['A', 'B']);
    expect(source.setupCount, 1);
    expect(
      observe.calls.where((call) => call == 'review' || call == 'enable'),
      isEmpty,
    );
    expect(find.text('Floe can use this connection.'), findsOneWidget);
  });

  testWidgets('Attention source setup and Observe review are separate', (
    tester,
  ) async {
    final source = _SourceGateway('attention.macos', const [], exists: false);
    final observe = _ObserveGateway('needs_review');
    await tester.pumpWidget(
      _host(
        PersonalSingletonSourceCard(
          title: 'Attention source',
          description: 'Coarse attention',
          connectorId: 'attention.macos',
          sourceGateway: source,
          observeGateway: observe,
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.byKey(const ValueKey('personal-use-with-floe')), findsNothing);
    await tester.tap(find.text('Set up source'));
    await tester.pumpAndSettle();
    expect(source.setupCount, 1);
    expect(observe.calls, ['inspect']);
    await tester.tap(find.byKey(const ValueKey('personal-use-with-floe')));
    await tester.pumpAndSettle();
    expect(observe.calls, ['inspect', 'review']);
    await tester.tap(find.text('Allow'));
    await tester.pumpAndSettle();
    expect(observe.calls, ['inspect', 'review', 'enable']);
  });
}

Widget _host(Widget child) => MaterialApp(
  theme: FloeTheme.light,
  localizationsDelegates: AppLocalizations.localizationsDelegates,
  supportedLocales: AppLocalizations.supportedLocales,
  home: Scaffold(body: SingleChildScrollView(child: child)),
);

final class _SourceGateway implements NativePersonalSourceGateway {
  _SourceGateway(this.connectorId, this.savedHandles, {this.exists = true});

  final String connectorId;
  List<String> savedHandles;
  bool exists;
  int revision = 1;
  int setupCount = 0;

  @override
  Future<SourceConnection?> inspect(String connectorId) async =>
      exists ? _source() : null;

  @override
  Future<SourceConnection> setup({
    required String connectorId,
    required int? expectedRevision,
    required List<String> selectedHandles,
  }) async {
    setupCount++;
    savedHandles = selectedHandles;
    exists = true;
    revision++;
    return _source();
  }

  SourceConnection _source() => SourceConnection(
    connectorId: connectorId,
    connectionId: '$connectorId.local',
    executionOwnerId: 'apple:device',
    state: 'ready',
    revision: revision,
    sourceAuthority: const SourceAuthority(
      incarnation: '00000000-0000-4000-8000-000000000001',
      epoch: 1,
    ),
    resourceMode: connectorId == 'contacts.apple'
        ? 'selected'
        : 'all_available',
    resources: [
      for (final handle in savedHandles)
        SourceResource(handle: handle, label: handle),
    ],
    nativeSubjectFingerprint: 'a' * 64,
  );
}

final class _ObserveGateway implements ConnectionObserveGateway {
  _ObserveGateway(this.status);

  String status;
  final List<String> calls = [];

  @override
  Future<ConnectionObserveOverview> inspect({
    required String connectorId,
    required String connectionId,
  }) async {
    calls.add('inspect');
    return _overview(connectorId, connectionId);
  }

  @override
  Future<ConnectionObserveReview> review({
    required String connectorId,
    required String connectionId,
  }) async {
    calls.add('review');
    return ConnectionObserveReview.fromJson({
      'connector_id': connectorId,
      'connection_id': connectionId,
      'source_authority': {
        'incarnation': '00000000-0000-4000-8000-000000000001',
        'epoch': 1,
      },
      'connection_revision': 2,
      'native_subject': 'a' * 64,
      'producer_fingerprint': null,
      'members': [
        {
          'view_id': 'attention.coarse',
          'resource': 'connection/$connectionId/view/attention.coarse',
          'policy_digest': 'a' * 64,
          'expected_grant_id': null,
          'expected_grant_authority': null,
        },
      ],
    });
  }

  @override
  Future<ConnectionObserveOverview> setEnabled({
    required String connectorId,
    required String connectionId,
    required bool enabled,
    bool disconnecting = false,
    ConnectionObserveReview? expected,
  }) async {
    calls.add(enabled ? 'enable' : 'disable');
    status = enabled ? 'active' : 'paused';
    return _overview(connectorId, connectionId);
  }

  ConnectionObserveOverview _overview(
    String connectorId,
    String connectionId,
  ) => ConnectionObserveOverview.fromJson({
    'connector_id': connectorId,
    'connection_id': connectionId,
    'status': status,
    'enabled': status == 'active',
    'source_resources': const [],
    'members': [
      {
        'view_id': 'attention.coarse',
        'state': status == 'active' ? 'active' : 'paused',
        'review_required': status == 'needs_review',
      },
    ],
  });
}
