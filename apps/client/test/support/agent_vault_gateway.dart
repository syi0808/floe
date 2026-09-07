import 'dart:async';

import 'package:floe_client/features/agent/agent_fixture_gateway.dart';
import 'package:floe_client/features/agent/agent_vault_gateway.dart';

import 'agent_gateway.dart';

class TestVaultGateway extends TestAgentGateway implements AgentVaultGateway {
  TestVaultGateway({super.personId});
  AgentVaultState state = AgentVaultState.missing;
  int creates = 0;
  int unlocks = 0;
  int locks = 0;
  bool unavailable = false;
  Completer<void>? resumeGate;

  void _check() {
    if (unavailable) throw const AgentVaultException('vault_unavailable');
  }

  @override
  Future<AgentVaultState> vaultStatus(String personId) async {
    _check();
    return state;
  }

  @override
  Future<AgentVaultState> createVault(String personId) async {
    _check();
    if (state != AgentVaultState.missing) {
      throw const AgentVaultException('conflict');
    }
    creates++;
    return state = AgentVaultState.ready;
  }

  @override
  Future<AgentVaultState> unlockVault(String personId) async {
    _check();
    unlocks++;
    return state = AgentVaultState.ready;
  }

  @override
  Future<void> lockVault(String personId) async {
    _check();
    locks++;
    state = AgentVaultState.locked;
  }

  @override
  Future<AgentFixtureResult> resumeAgentFixture(String personId) async {
    await resumeGate?.future;
    _check();
    if (state != AgentVaultState.ready) {
      throw const AgentVaultException('vault_unavailable');
    }
    return super.resumeAgentFixture(personId);
  }
}
