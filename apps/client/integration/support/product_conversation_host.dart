import 'dart:convert';
import 'dart:io';

import 'package:floe_client/app/runtime/app_read_model.dart';
import 'package:floe_client/app/runtime/app_runtime.dart';
import 'package:floe_client/app/runtime/floe_client.dart';
import 'package:floe_client/features/conversation/application/agent_conversation_gateway.dart';
import 'package:floe_client/features/conversation/application/agent_request_id.dart';
import 'package:floe_client/features/conversation/domain/agent_session.dart';
import 'package:flutter/widgets.dart';

import 'disposable_product_profile.dart';

void require(bool condition, String label) {
  if (!condition) throw StateError(label);
}

void requireSecretFree(Object? value) {
  if (value is Map) {
    for (final entry in value.entries) {
      require(
        !RegExp(
          r'bearer|token|base_url|route',
          caseSensitive: false,
        ).hasMatch(entry.key.toString()),
        'Secret/topology field in product result.',
      );
      requireSecretFree(entry.value);
    }
  } else if (value is List) {
    for (final entry in value) {
      requireSecretFree(entry);
    }
  }
}

Future<Map<String, dynamic>> rawRun(AppRuntime runtime, String runId) async {
  final response = await runtime.wireTransport.queryV2({
    'schema_version': 2,
    'request_id': newAgentRequestId(),
    'query': {'kind': 'conversation.get_run', 'run_id': runId},
  });
  requireSecretFree(response);
  return response;
}

Future<void> exercise() async {
  final profile = await DisposableProductProfile.create();
  try {
    final library = Platform.environment['FLOE_VALIDATION_FFI']!;
    var runtime = await profile.open(library);
    await runtime.vault.createVault(profile.personId);
    await profile.recordVault();
    var session = await runtime.conversation.startConversation(
      profile.personId,
    );
    final successfulRuns = <String, Map<String, dynamic>>{};
    final finalMessages = <String, AppMessage>{};
    for (final profileId in <String?>[null, 'foundation-device']) {
      final gateway = runtime.conversation.conversationRuntime!;
      await gateway.synchronizeConversation(session);
      require(
        runtime.readModel.conversation.syncState ==
            AppReadSyncState.synchronized,
        'Read model must synchronize before admission.',
      );
      final observed = <AppRunSnapshot>[];
      final completion = await gateway.runConversationTurn(
        AgentConversationTurnRequest(
          session: session,
          text: 'Say a short friendly greeting. Do not use tools or delegate.',
          profileId: profileId,
        ),
        onRun: observed.add,
      );
      final run = completion.run;
      require(
        run.state == AppRunState.finished && run.sessionId == session.id,
        'Terminal Run scope/state mismatch.',
      );
      final report = run.report;
      require(
        report != null &&
            report.execution == 'completed' &&
            report.reply == 'generated' &&
            report.finalMessageRef != null,
        'Expected generated completion: ${report?.execution}/${report?.reply}; '
        'issues=${report?.issues.map((issue) => issue.code).toList()}',
      );
      final snapshot = await rawRun(runtime, run.runId);
      require(
        (snapshot['attempt_refs'] as List).isNotEmpty,
        'No model attempt.',
      );
      require(
        (snapshot['task_refs'] as List).isEmpty,
        'Ordinary turn manufactured a Task.',
      );
      final message = await runtime.client.getMessage(report!.finalMessageRef!);
      require(
        message.role == 'assistant' && message.text.trim().isNotEmpty,
        'Missing generated assistant message.',
      );
      final receipt = runtime.readModel.conversation.commands.values
          .singleWhere((receipt) => receipt.runId == run.runId);
      final command = await runtime.client.getCommand(receipt.commandId);
      require(
        command?.runId == run.runId &&
            command?.runtimeEpoch == run.runtimeEpoch,
        'Admission/read identity mismatch.',
      );
      session = completion.session;
      require(
        session.personId == profile.personId &&
            session.activeTurn == null &&
            session.revision >= receipt.sessionRevision,
        'Terminal session mismatch.',
      );
      require(
        observed.isNotEmpty && observed.last.runId == run.runId,
        'No product Run observation.',
      );
      successfulRuns[run.runId] = snapshot;
      finalMessages[message.messageId] = message;
      stdout.writeln(
        jsonEncode({
          'profile': profileId ?? 'auto',
          'state': run.state.name,
          'execution': report.execution,
          'reply': report.reply,
          'attempt_count': (snapshot['attempt_refs'] as List).length,
          'task_count': (snapshot['task_refs'] as List).length,
        }),
      );
    }
    final history = session.messages
        .map(
          (message) => (
            message.turnId,
            message.kind,
            message is AgentTextMessage ? message.text : '',
          ),
        )
        .toList();
    require(
      history.where((message) => message.$2 == AgentMessageKind.user).length ==
              2 &&
          history
                  .where((message) => message.$2 == AgentMessageKind.assistant)
                  .length ==
              2,
      'Expected exactly two user/assistant pairs.',
    );
    await profile.closeRuntime();
    runtime = await profile.open(library);
    await runtime.vault.unlockVault(profile.personId);
    final reopened = await runtime.conversation.loadConversation(
      profile.personId,
      session.id,
    );
    await runtime.conversation.conversationRuntime!.synchronizeConversation(
      reopened,
    );
    require(
      reopened.activeTurn == null &&
          reopened.revision == session.revision &&
          reopened.messages.length == history.length,
      'Reopen changed durable history.',
    );
    for (var index = 0; index < history.length; index++) {
      final message = reopened.messages[index];
      require(
        (
              message.turnId,
              message.kind,
              message is AgentTextMessage ? message.text : '',
            ) ==
            history[index],
        'Reopen duplicated or changed a message.',
      );
    }
    for (final entry in successfulRuns.entries) {
      final current = await rawRun(runtime, entry.key);
      for (final key in [
        'run_id',
        'session_id',
        'revision',
        'executor_generation',
        'state',
        'report',
        'attempt_refs',
        'task_refs',
      ]) {
        require(
          jsonEncode(current[key]) == jsonEncode(entry.value[key]),
          'Reopen changed $key or repeated work.',
        );
      }
      final run = await runtime.client.getRun(entry.key);
      require(
        runtime.readModel.applyRunSnapshot(run),
        'Reopened Run read-model rejection.',
      );
    }
    for (final entry in finalMessages.entries) {
      final current = await runtime.client.getMessage(entry.key);
      require(
        current.messageId == entry.key &&
            current.role == entry.value.role &&
            current.text == entry.value.text,
        'Final message changed on reopen.',
      );
    }
    require(
      runtime.readModel.conversation.syncState ==
              AppReadSyncState.synchronized &&
          runtime.readModel.conversation.runs.length == 2,
      'Final read model not synchronized.',
    );
    stdout.writeln('PRODUCT_CONVERSATION_PASSED');
  } finally {
    await profile.cleanup();
  }
}

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  try {
    await exercise();
    exit(0);
  } on Object catch (error, stack) {
    stderr.writeln('$error\n$stack');
    exit(1);
  }
}
