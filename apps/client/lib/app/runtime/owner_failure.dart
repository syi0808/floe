/// Safe owner-projected failure data. This decoder does not infer recovery policy.
final class OwnerFailure {
  const OwnerFailure({required this.domain, required this.category, required this.reason,
    required this.incidentId, required this.correlationId, required this.reloadRequired,
    required this.sealSession, required this.recovery, required this.safeActions});

  factory OwnerFailure.fromJson(Object? value) {
    const keys = {'domain','category','reason','incident_id','correlation_id','reload_required','seal_session','recovery','safe_actions'};
    if (value is! Map || value.length != keys.length || !value.keys.toSet().containsAll(keys)) {
      throw const FormatException('Invalid owner failure.');
    }
    String text(String key, int maximum) {
      final item = value[key];
      if (item is! String || item.isEmpty || item.length > maximum ||
          item.runes.any((rune) => rune < 32 || rune >= 127 && rune <= 159)) {
        throw const FormatException('Invalid owner failure field.');
      }
      return item;
    }
    final category = text('category', 64);
    final recovery = text('recovery', 32);
    final actions = value['safe_actions'];
    if (!{'user_configuration','transient','integrity','security','internal'}.contains(category) ||
        !{'none','reobserve','reconcile','unlock','reopen','new_review'}.contains(recovery) ||
        value['reload_required'] is! bool || value['seal_session'] is! bool ||
        actions is! List || actions.length > 16 ||
        actions.any((action) => action is! String || !RegExp(r'^[a-z_]{1,64}$').hasMatch(action)) ||
        actions.toSet().length != actions.length) {
      throw const FormatException('Invalid owner failure policy.');
    }
    return OwnerFailure(domain: text('domain', 64), category: category, reason: text('reason', 128),
      incidentId: text('incident_id', 128), correlationId: text('correlation_id', 128),
      reloadRequired: value['reload_required'] as bool, sealSession: value['seal_session'] as bool,
      recovery: recovery, safeActions: Set<String>.unmodifiable(actions.cast<String>()));
  }
  final String domain;
  final String category;
  final String reason;
  final String incidentId;
  final String correlationId;
  final bool reloadRequired;
  final bool sealSession;
  final String recovery;
  final Set<String> safeActions;
}
