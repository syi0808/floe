import 'agent_fixture_gateway.dart';

import 'dart:convert';
import 'dart:ffi';

import 'package:ffi/ffi.dart';
import 'package:floe_client/app/runtime/app_runtime.dart';
import 'package:floe_client/features/day/infrastructure/floe_native_bindings.dart';

final class NativeAgentFixtureGateway implements AgentFixtureStreamingGateway {
  NativeAgentFixtureGateway(this._request, this._close);

  final void Function() _close;
  bool _closed = false;

  Future<void> close() async {
    if (_closed) return;
    _closed = true;
    _close();
  }

  static Future<NativeAgentFixtureGateway> open({
    required String libraryPath,
    required String databasePath,
    required String deviceId,
  }) async {
    final bindings = FloeNativeBindings(libraryPath);
    final library = DynamicLibrary.open(libraryPath);
    final fixture = library.lookupFunction<FloeCallNative, FloeCallDart>(
      'floe_core_agent_fixture',
    );
    final streaming = library.lookupFunction<FloeCallNative, FloeCallDart>(
      'floe_core_agent_fixture_run',
    );
    final database = databasePath.toNativeUtf8();
    final error = calloc<Pointer<Utf8>>();
    late final Pointer<Void> handle;
    try {
      handle = bindings.open(database, error);
      if (handle == nullptr) {
        throw StateError('Test fixture core could not open.');
      }
    } finally {
      if (error.value != nullptr) bindings.freeString(error.value);
      calloc.free(error);
      calloc.free(database);
    }
    return NativeAgentFixtureGateway((operation, request) async {
      final input = jsonEncode(request).toNativeUtf8();
      Pointer<Utf8> output = nullptr;
      try {
        output = (operation == 'agent_fixture' ? fixture : streaming)(
          handle,
          input,
        );
        final envelope =
            jsonDecode(output.toDartString()) as Map<String, dynamic>;
        if (envelope['status'] != 'ok') {
          final failure = envelope['error'] as Map;
          throw AppRuntimeException(
            failure['code'] as String,
            failure['message'] as String,
            metadata: Map<String, String>.from(
              failure['metadata'] as Map? ?? {},
            ),
          );
        }
        return Map<String, dynamic>.from(envelope['data'] as Map);
      } finally {
        if (output != nullptr) bindings.freeString(output);
        calloc.free(input);
      }
    }, () => bindings.freeCore(handle));
  }

  final Future<Map<String, dynamic>> Function(
    String operation,
    Map<String, dynamic> request,
  )
  _request;

  @override
  Future<AgentFixtureResult> startAgentFixture(String personId) =>
      _session(personId, {'kind': 'start'});

  @override
  Future<AgentFixtureResult> resumeAgentFixture(String personId) =>
      _session(personId, {'kind': 'resume'});

  @override
  Future<AgentRunUpdate> beginAgentFixtureRun(
    AgentSession session,
    AgentFixturePrompt prompt,
  ) => _run(session, {'kind': 'begin', 'prompt': prompt.wireName});

  @override
  Future<AgentRunUpdate> pollAgentFixtureRun(
    AgentSession session,
    int afterSequence,
  ) => _run(session, {'kind': 'poll', 'after_sequence': afterSequence});

  @override
  Future<AgentRunUpdate> stopAgentFixtureRun(AgentSession session) =>
      _run(session, {'kind': 'stop'});

  @override
  Future<AgentRunUpdate> releaseAgentFixtureRun(AgentSession session) =>
      _run(session, {'kind': 'release'});

  Future<AgentRunUpdate> _run(
    AgentSession session,
    Map<String, Object?> operation,
  ) async => AgentRunUpdate.fromJson(
    await _request('agent_fixture_run', {
      'schema_version': agentSchemaVersion,
      'person_id': session.personId,
      'session_id': session.id,
      'expected_revision': session.revision,
      'operation': operation,
    }),
  );

  @override
  Future<AgentFixtureResult> loadAgentFixture(
    String personId,
    String sessionId,
  ) => _session(personId, {'kind': 'get', 'session_id': sessionId});

  @override
  Future<AgentFixtureResult> runAgentFixture(
    AgentSession session,
    AgentFixturePrompt prompt,
  ) => _session(session.personId, {
    'kind': 'turn',
    'session_id': session.id,
    'expected_revision': session.revision,
    'prompt': prompt.wireName,
  });

  @override
  Future<AgentFixtureResult> recoverAgentFixture(AgentSession session) =>
      _session(session.personId, {
        'kind': 'recover',
        'session_id': session.id,
        'expected_revision': session.revision,
      });

  Future<AgentFixtureResult> _session(
    String personId,
    Map<String, Object?> operation,
  ) async => AgentFixtureResult.fromJson(
    await _request('agent_fixture', {
      'schema_version': agentSchemaVersion,
      'person_id': personId,
      'operation': operation,
    }),
  );
}
