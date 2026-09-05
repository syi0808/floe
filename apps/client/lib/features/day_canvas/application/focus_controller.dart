import 'package:flutter/foundation.dart';

import '../domain/day_models.dart';
import '../domain/focus_models.dart';
import 'focus_gateway.dart';

class FocusController extends ChangeNotifier {
  FocusController({required this.gateway, required this.query});

  final FocusGateway gateway;
  final DayQuery query;
  FocusPreference? preference;
  FocusProposal? proposal;
  String? errorCode;
  bool pending = false;
  bool loaded = false;
  bool _disposed = false;

  Future<bool> load() => _run(() async {
    loaded = false;
    final value = await gateway.loadFocusPreference(query);
    if (_disposed) return;
    preference = value;
    loaded = true;
  });

  Future<bool> save(FocusPreferenceValue? value) {
    if (!loaded) return Future.value(false);
    return _run(() async {
      final saved = await gateway.saveFocusPreference(
        query,
        preference?.revision ?? 0,
        value,
      );
      if (!_disposed) preference = saved;
    });
  }

  Future<bool> suggest({bool allowExternal = false}) {
    if (!loaded) return Future.value(false);
    return _run(() async {
      final result = await gateway.suggestFocus(
        query,
        allowExternal: allowExternal,
      );
      if (!_disposed) proposal = result;
    });
  }

  void clearProposal() {
    if (_disposed || pending) return;
    proposal = null;
    notifyListeners();
  }

  Future<bool> _run(Future<void> Function() operation) async {
    if (_disposed || pending) return false;
    pending = true;
    errorCode = null;
    proposal = null;
    notifyListeners();
    try {
      await operation();
      return !_disposed;
    } on FocusGatewayException catch (error) {
      if (!_disposed) {
        errorCode = error.code;
        if (error.code == 'conflict') loaded = false;
      }
      return false;
    } on Object {
      if (!_disposed) errorCode = 'unavailable';
      return false;
    } finally {
      if (!_disposed) {
        pending = false;
        notifyListeners();
      }
    }
  }

  @override
  void dispose() {
    _disposed = true;
    super.dispose();
  }
}
