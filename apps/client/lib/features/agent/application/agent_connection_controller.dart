import 'package:flutter/foundation.dart';

import '../agent_connections.dart';
import '../agent_vault_gateway.dart';

final class AgentConnectionController extends ChangeNotifier {
  AgentConnectionController({
    required this.gateway,
    required this.personId,
    required this.canOperate,
    required this.onFatalFailure,
  });

  final AgentConnectionsGateway? gateway;
  final String personId;
  final bool Function() canOperate;
  final void Function(String failure) onFatalFailure;

  List<AgentConnection>? connections;
  String? failure;
  bool busy = false;

  bool get available => gateway != null;
  bool get canRead => available && !busy && canOperate();

  Future<void> load() async {
    if (!canRead) return;
    busy = true;
    failure = null;
    notifyListeners();
    try {
      final result = await gateway!.readConnections(personId);
      if (!canOperate()) return;
      if (result.map((entry) => entry.descriptor.id).toSet().length !=
          result.length) {
        throw const FormatException('Duplicate connection');
      }
      connections = result;
    } on Object catch (error) {
      if (!canOperate()) return;
      connections = null;
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
    connections = null;
    failure = null;
    notifyListeners();
  }
}
