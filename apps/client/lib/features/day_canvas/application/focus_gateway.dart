import '../domain/day_models.dart';
import '../domain/focus_models.dart';

abstract interface class FocusGateway {
  Future<FocusPreference?> loadFocusPreference(DayQuery query);
  Future<FocusPreference> saveFocusPreference(
    DayQuery query,
    int expectedRevision,
    FocusPreferenceValue? value,
  );
  Future<FocusProposal> suggestFocus(
    DayQuery query,
    String model, {
    bool allowExternal = false,
  });
}

class FocusGatewayException implements Exception {
  const FocusGatewayException(this.code);
  final String code;
}
