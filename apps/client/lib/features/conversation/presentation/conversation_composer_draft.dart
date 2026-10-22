import 'package:flutter/widgets.dart';

import 'package:floe_client/features/conversation/application/conversation_controller.dart';

/// In-memory composer state owned above the transient AgentPanel surface.
///
/// The text controller keeps exact text and selection while a panel or sheet
/// is dismissed. Its scope is the controller/person/session tuple, so a new
/// owner or session starts with an empty composer.
final class ConversationComposerDraft {
  ConversationComposerDraft() {
    _composer.addListener(_composerChanged);
  }

  final TextEditingController _composer = TextEditingController();
  ConversationController? _owner;
  String? _personId;
  String? _sessionId;
  int _textRevision = 0;
  bool _disposed = false;

  TextEditingController get controller => _composer;

  bool reconcile(ConversationController controller) {
    final session = controller.session;
    final sessionId = session != null && session.personId == controller.personId
        ? session.id
        : null;
    if (identical(_owner, controller) &&
        _personId == controller.personId &&
        _sessionId == sessionId) {
      return false;
    }

    _owner = controller;
    _personId = controller.personId;
    _sessionId = sessionId;
    _textRevision++;
    _composer.clear();
    return true;
  }

  ConversationComposerSubmission? capture(ConversationController controller) {
    reconcile(controller);
    final sessionId = _sessionId;
    if (_disposed ||
        !identical(_owner, controller) ||
        sessionId == null ||
        controller.session?.id != sessionId) {
      return null;
    }
    return ConversationComposerSubmission._(
      controller: controller,
      personId: _personId!,
      sessionId: sessionId,
      text: _composer.text,
      textRevision: _textRevision,
    );
  }

  /// Clears only the exact text revision accepted by the owner.
  ///
  /// If the user typed something newer while admission was pending, that newer
  /// draft remains available instead of being erased by the older completion.
  bool clearAccepted(ConversationComposerSubmission submission) {
    if (_disposed ||
        !identical(_owner, submission.controller) ||
        _personId != submission.personId ||
        _sessionId != submission.sessionId ||
        submission.controller.session?.id != submission.sessionId ||
        _textRevision != submission.textRevision ||
        _composer.text != submission.text) {
      return false;
    }
    _composer.clear();
    return true;
  }

  void _composerChanged() {
    // Cursor/selection movement is still retained in the TextEditingController,
    // but it does not turn an accepted submission into a newer draft.
    if (_composer.text != _lastObservedText) {
      _lastObservedText = _composer.text;
      _textRevision++;
    }
  }

  String _lastObservedText = '';

  void dispose() {
    if (_disposed) return;
    _disposed = true;
    _composer.removeListener(_composerChanged);
    _composer.dispose();
  }
}

final class ConversationComposerSubmission {
  const ConversationComposerSubmission._({
    required this.controller,
    required this.personId,
    required this.sessionId,
    required this.text,
    required this.textRevision,
  });

  final ConversationController controller;
  final String personId;
  final String sessionId;
  final String text;
  final int textRevision;
}
