import 'package:flutter/foundation.dart';

import 'package:floe_client/app/runtime/app_owner_exception.dart';
import 'package:floe_client/features/conversation/assistant_features/domain/assistant_feature.dart';

final class AssistantFeatureController extends ChangeNotifier {
  AssistantFeatureController({
    required this.gateway,
    required this.canOperate,
    required this.onFatalFailure,
  });

  final AssistantFeatureGateway? gateway;
  final bool Function() canOperate;
  final void Function(AppOwnerException failure) onFatalFailure;

  int _operationGeneration = 0;
  bool _disposed = false;
  final Map<String, AssistantFeatureSourceReview> _reviews = {};
  final Map<String, String> _reviewFeatures = {};
  final Map<String, AssistantFeatureSourceSelection> _stagedSelections = {};
  final Map<String, int> _reviewGenerations = {};
  final Map<String, bool> _enabledDrafts = {};
  String? _pendingConfigureFeatureRef;
  List<AssistantFeatureSourceSelection> _pendingConfigureSelections = const [];
  Map<String, int> _pendingConfigureReviewGenerations = const {};
  int _pendingConfigureFocusGeneration = 0;
  List<AssistantFeatureSourceSelection> _lastConfiguredSelections = const [];

  AssistantFeatureSnapshot? snapshot;
  AssistantFeatureSourceReview? focusedReview;
  String? failure;
  String? reviewFailure;
  bool loaded = false;
  bool busy = false;

  bool didConfigureReview(AssistantFeatureSourceReviewRef reviewRef) =>
      _lastConfiguredSelections.any(
        (selection) => selection.review.reviewRef.matches(reviewRef),
      );

  void acknowledgeConfiguredReview(AssistantFeatureSourceReviewRef reviewRef) {
    final focused = focusedReview;
    if (_disposed ||
        !didConfigureReview(reviewRef) ||
        focused == null ||
        !focused.reviewRef.matches(reviewRef)) {
      return;
    }
    final key = _key(focused.sourceScopeRef, focused.sourceRequirementRef);
    final staged = _stagedSelections[key];
    if (staged != null && staged.review.reviewRef.matches(reviewRef)) {
      _stagedSelections.remove(key);
      _reviewFeatures.remove(key);
      _reviews.remove(key);
      _reviewGenerations.remove(key);
    }
    focusedReview = null;
    _focusGeneration++;
    notifyListeners();
  }

  bool get available => gateway != null;
  AssistantFeatureCommandKind? get pendingCommandKind =>
      gateway?.pendingCommandKind;
  bool get canRead => available && !busy && canOperate();
  bool get canManage =>
      available && !busy && canOperate() && pendingCommandKind == null;
  bool get canRetryPending =>
      available && !busy && canOperate() && pendingCommandKind != null;

  bool enabledDraft(AssistantFeature feature) =>
      _enabledDrafts[feature.featureRef] ?? feature.enabled;

  AssistantFeatureSourceReview? reviewFor(
    AssistantFeatureSourceGroup group,
    AssistantFeatureSourceRequirement requirement,
  ) => _reviews[_key(group.sourceScopeRef, requirement.requirementRef)];

  bool hasStagedChanges(AssistantFeature feature) =>
      enabledDraft(feature) != feature.enabled ||
      _stagedSelections.entries.any((entry) {
        if (_reviewFeatures[entry.key] != feature.featureRef) return false;
        final selected = entry.value.review.candidates
            .where((candidate) => candidate.selected)
            .map((candidate) => candidate.candidateRef)
            .toSet();
        return !setEquals(selected, entry.value.candidateRefs.toSet());
      });

  Future<void> load() async {
    final generation = _operationGeneration;
    if (!_isCurrent(generation) || !canRead) return;
    busy = true;
    failure = null;
    _notifyIfCurrent(generation);
    try {
      final loadedSnapshot = await gateway!.readSnapshot();
      if (!_isCurrent(generation)) return;
      snapshot = loadedSnapshot;
      loaded = true;
      _enabledDrafts
        ..clear()
        ..addEntries(
          loadedSnapshot.features.map(
            (feature) => MapEntry(feature.featureRef, feature.enabled),
          ),
        );
      _reviews.clear();
      _reviewFeatures.clear();
      _stagedSelections.clear();
      _reviewGenerations.clear();
      focusedReview = null;
      _focusGeneration++;
      _lastConfiguredSelections = const [];
      if (pendingCommandKind == null) {
        _pendingConfigureFeatureRef = null;
        _pendingConfigureSelections = const [];
        _pendingConfigureReviewGenerations = const {};
      }
    } on Object catch (error) {
      if (!_isCurrent(generation)) return;
      snapshot = null;
      loaded = false;
      failure = _failure(error);
      _reportFatal(error, generation);
    } finally {
      if (_isCurrent(generation, requireCanOperate: false)) {
        busy = false;
        notifyListeners();
      }
    }
  }

  void setEnabledDraft(AssistantFeature feature, bool enabled) {
    final generation = _operationGeneration;
    if (!_isCurrent(generation) || !canManage) return;
    _enabledDrafts[feature.featureRef] = enabled;
    _notifyIfCurrent(generation);
  }

  Future<void> prepareSourceReview(
    AssistantFeature feature,
    AssistantFeatureSourceGroup group,
    AssistantFeatureSourceRequirement requirement,
  ) async {
    final generation = _operationGeneration;
    if (!_isCurrent(generation) || !canManage) return;
    await _run(generation, (operationGeneration) async {
      final prepared = await gateway!.prepareSourceReview(
        featureRef: feature.featureRef,
        sourceScopeRef: group.sourceScopeRef,
        sourceRequirementRef: requirement.requirementRef,
        expectedBindingRevision: group.bindingRevision,
      );
      if (!_isCurrent(operationGeneration)) return;
      _storeReview(feature.featureRef, prepared, preserveDraft: false);
    }, reviewOperation: true);
  }

  Future<void> loadReview(AssistantFeatureSourceReviewRef reviewRef) async {
    final generation = _operationGeneration;
    if (!_isCurrent(generation) || !canRead) return;
    await _run(generation, (operationGeneration) async {
      final loadedReview = await gateway!.inspectSourceReview(reviewRef);
      if (!_isCurrent(operationGeneration)) return;
      if (!loadedReview.reviewRef.matches(reviewRef)) {
        throw const FormatException('Source review reference mismatch.');
      }
      final matches = snapshot?.features.where(
        (feature) => feature.sourceGroups.any(
          (group) => group.sourceScopeRef == loadedReview.sourceScopeRef,
        ),
      );
      if (matches == null || matches.length != 1) {
        throw const FormatException('Source review scope mismatch.');
      }
      _storeReview(
        matches.single.featureRef,
        loadedReview,
        preserveDraft: true,
      );
    }, reviewOperation: true);
  }

  void usePreparedReview(AssistantFeatureSourceReview review) {
    final generation = _operationGeneration;
    if (!_isCurrent(generation)) return;
    final matches = snapshot?.features.where(
      (feature) => feature.sourceGroups.any(
        (group) => group.sourceScopeRef == review.sourceScopeRef,
      ),
    );
    if (matches == null || matches.length != 1) {
      reviewFailure = 'conflict';
      _notifyIfCurrent(generation);
      return;
    }
    _focusGeneration++;
    _storeReview(matches.single.featureRef, review, preserveDraft: true);
    focusedReview = review;
    _notifyIfCurrent(generation);
  }

  void setSourceCandidates(
    AssistantFeatureSourceReview review,
    Set<String> candidateRefs,
  ) {
    final generation = _operationGeneration;
    if (!_isCurrent(generation) || !canManage || !review.canReplace) return;
    final featureRef =
        _reviewFeatures[_key(
          review.sourceScopeRef,
          review.sourceRequirementRef,
        )];
    if (featureRef == null) return;
    try {
      _stagedSelections[_key(
        review.sourceScopeRef,
        review.sourceRequirementRef,
      )] = AssistantFeatureSourceSelection(
        review: review,
        candidateRefs: candidateRefs.toList()..sort(),
      );
      _notifyIfCurrent(generation);
    } on Object catch (error) {
      reviewFailure = _failure(error);
      _notifyIfCurrent(generation);
    }
  }

  Set<String> selectedCandidates(AssistantFeatureSourceReview review) =>
      _stagedSelections[_key(
            review.sourceScopeRef,
            review.sourceRequirementRef,
          )]
          ?.candidateRefs
          .toSet() ??
      review.candidates
          .where((candidate) => candidate.selected)
          .map((candidate) => candidate.candidateRef)
          .toSet();

  bool canConfigure(AssistantFeature feature) =>
      canManage && hasStagedChanges(feature);

  Future<bool> configure(AssistantFeature feature) async {
    final generation = _operationGeneration;
    final current = snapshot;
    if (!_isCurrent(generation) || !canConfigure(feature) || current == null) {
      return false;
    }
    final selections = _stagedSelections.entries
        .where((entry) => _reviewFeatures[entry.key] == feature.featureRef)
        .map((entry) => entry.value)
        .toList(growable: false);
    final submittedReviewGenerations = <String, int>{};
    for (final selection in selections) {
      final key = _key(
        selection.review.sourceScopeRef,
        selection.review.sourceRequirementRef,
      );
      final generationForReview = _reviewGenerations[key] ?? _focusGeneration;
      submittedReviewGenerations[key] = generationForReview;
    }
    final submittedFocusGeneration = _focusGeneration;
    _pendingConfigureFeatureRef = feature.featureRef;
    _pendingConfigureSelections = List.unmodifiable(selections);
    _pendingConfigureReviewGenerations = Map.unmodifiable(
      submittedReviewGenerations,
    );
    _pendingConfigureFocusGeneration = submittedFocusGeneration;
    _lastConfiguredSelections = const [];
    return _run(generation, (operationGeneration) async {
      final updated = await gateway!.configure(
        featureRef: feature.featureRef,
        expectedRevision: current.revision,
        enabled: enabledDraft(feature),
        sourceSelections: selections,
      );
      if (!_isCurrent(operationGeneration)) return;
      final resultFeature = updated.features
          .where((entry) => entry.featureRef == feature.featureRef)
          .singleOrNull;
      if (resultFeature == null ||
          resultFeature.enabled != enabledDraft(feature)) {
        throw const FormatException('Assistant feature result scope mismatch.');
      }
      _applyConfiguredSnapshot(
        updated,
        feature.featureRef,
        selections,
        submittedReviewGenerations,
        submittedFocusGeneration,
      );
    });
  }

  Future<AssistantFeatureCommandResult?> retryPendingCommand() async {
    final generation = _operationGeneration;
    if (!_isCurrent(generation) || !canRetryPending) return null;
    AssistantFeatureCommandResult? result;
    final succeeded = await _run(generation, (operationGeneration) async {
      final retried = await gateway!.retryPendingCommand();
      if (!_isCurrent(operationGeneration)) return;
      result = retried;
      switch (retried) {
        case AssistantFeatureSnapshotCommandResult(:final snapshot):
          if (retried.kind == AssistantFeatureCommandKind.configure &&
              _pendingConfigureFeatureRef != null) {
            _applyConfiguredSnapshot(
              snapshot,
              _pendingConfigureFeatureRef!,
              _pendingConfigureSelections,
              _pendingConfigureReviewGenerations,
              _pendingConfigureFocusGeneration,
            );
          } else {
            this.snapshot = snapshot;
            loaded = true;
            _enabledDrafts
              ..clear()
              ..addEntries(
                snapshot.features.map(
                  (feature) => MapEntry(feature.featureRef, feature.enabled),
                ),
              );
          }
        case AssistantFeatureSourceReviewCommandResult(:final review):
          usePreparedReview(review);
      }
    }, reviewOperation: true);
    if (!_isCurrent(generation) || !succeeded) return null;
    return result;
  }

  void clear() {
    _operationGeneration++;
    if (_disposed) return;
    snapshot = null;
    focusedReview = null;
    _reviews.clear();
    _reviewFeatures.clear();
    _stagedSelections.clear();
    _reviewGenerations.clear();
    _enabledDrafts.clear();
    _pendingConfigureFeatureRef = null;
    _pendingConfigureSelections = const [];
    _pendingConfigureReviewGenerations = const {};
    _pendingConfigureFocusGeneration = 0;
    _lastConfiguredSelections = const [];
    _focusGeneration++;
    failure = null;
    reviewFailure = null;
    loaded = false;
    busy = false;
    notifyListeners();
  }

  @override
  void dispose() {
    _operationGeneration++;
    _disposed = true;
    super.dispose();
  }

  Future<bool> _run(
    int generation,
    Future<void> Function(int generation) operation, {
    bool reviewOperation = false,
  }) async {
    if (!_isCurrent(generation) || busy) return false;
    busy = true;
    failure = null;
    reviewFailure = null;
    _notifyIfCurrent(generation);
    try {
      await operation(generation);
      return _isCurrent(generation);
    } on Object catch (error) {
      if (!_isCurrent(generation)) return false;
      final message = _failure(error);
      if (reviewOperation) {
        reviewFailure = message;
      } else {
        failure = message;
      }
      _reportFatal(error, generation);
      return false;
    } finally {
      if (_isCurrent(generation, requireCanOperate: false)) {
        busy = false;
        notifyListeners();
      }
    }
  }

  void _storeReview(
    String featureRef,
    AssistantFeatureSourceReview review, {
    required bool preserveDraft,
  }) {
    final key = _key(review.sourceScopeRef, review.sourceRequirementRef);
    _reviews[key] = review;
    _reviewFeatures[key] = featureRef;
    _reviewGenerations[key] = _focusGeneration;
    final existing = _stagedSelections[key];
    if (!preserveDraft ||
        existing == null ||
        !_sameReview(existing.review, review)) {
      _stagedSelections[key] = AssistantFeatureSourceSelection(
        review: review,
        candidateRefs: review.candidates
            .where((candidate) => candidate.selected)
            .map((candidate) => candidate.candidateRef)
            .toList(growable: false),
      );
    }
  }

  int _focusGeneration = 0;

  void _applyConfiguredSnapshot(
    AssistantFeatureSnapshot updated,
    String featureRef,
    List<AssistantFeatureSourceSelection> submittedSelections,
    Map<String, int> submittedReviewGenerations,
    int submittedFocusGeneration,
  ) {
    final resultFeature = updated.features
        .where((entry) => entry.featureRef == featureRef)
        .singleOrNull;
    if (resultFeature == null) {
      throw const FormatException('Assistant feature result scope mismatch.');
    }
    snapshot = updated;
    loaded = true;
    _enabledDrafts[featureRef] = resultFeature.enabled;
    _lastConfiguredSelections = List.unmodifiable(submittedSelections);
    _pendingConfigureFeatureRef = null;
    _pendingConfigureSelections = const [];
    _pendingConfigureReviewGenerations = const {};
    _pendingConfigureFocusGeneration = 0;

    for (final submitted in submittedSelections) {
      final key = _key(
        submitted.review.sourceScopeRef,
        submitted.review.sourceRequirementRef,
      );
      final current = _stagedSelections[key];
      if (current == null ||
          _reviewGenerations[key] != submittedReviewGenerations[key] ||
          !_sameReview(current.review, submitted.review) ||
          !listEquals(current.candidateRefs, submitted.candidateRefs)) {
        continue;
      }
      _stagedSelections.remove(key);
      _reviewFeatures.remove(key);
      _reviews.remove(key);
      _reviewGenerations.remove(key);
    }
    final focused = focusedReview;
    if (_focusGeneration == submittedFocusGeneration &&
        focused != null &&
        submittedSelections.any(
          (selection) => _sameReview(selection.review, focused),
        )) {
      focusedReview = null;
      _focusGeneration++;
    }
  }

  bool _sameReview(
    AssistantFeatureSourceReview left,
    AssistantFeatureSourceReview right,
  ) =>
      left.reviewRef.matches(right.reviewRef) &&
      left.sourceScopeRef == right.sourceScopeRef &&
      left.sourceRequirementRef == right.sourceRequirementRef;

  String _key(String sourceScopeRef, String requirementRef) =>
      '$sourceScopeRef/$requirementRef';

  String _failure(Object error) =>
      error is AppOwnerException ? error.failure : 'storage_unavailable';

  bool _isCurrent(int generation, {bool requireCanOperate = true}) =>
      !_disposed &&
      generation == _operationGeneration &&
      (!requireCanOperate || canOperate());

  void _notifyIfCurrent(int generation) {
    if (_isCurrent(generation, requireCanOperate: false)) notifyListeners();
  }

  void _reportFatal(Object error, int generation) {
    if (_isCurrent(generation) && error is AppOwnerException) {
      onFatalFailure(error);
    }
  }
}
