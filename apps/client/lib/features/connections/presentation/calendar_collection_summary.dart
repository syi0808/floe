import 'package:flutter/material.dart';
import 'package:intl/intl.dart';

import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/features/day/domain/day_models.dart';
import 'package:floe_client/l10n/app_localizations.dart';

/// Historical Day-owned collection facts, never current source permission.
final class CalendarCollectionSummary extends StatelessWidget {
  const CalendarCollectionSummary({
    super.key,
    required this.coverage,
    required this.selectedResourceRefs,
  });

  final DayCalendarSourceCoverage? coverage;
  final List<String> selectedResourceRefs;

  DayCalendarSourceCoverage? get _matchedCoverage {
    final source = coverage;
    if (source == null) return null;
    final selected = selectedResourceRefs.toSet();
    final observed = source.resources.map((item) => item.resourceRef).toSet();
    if (selected.length != selectedResourceRefs.length ||
        observed.length != source.resources.length ||
        selected.length != observed.length ||
        !selected.containsAll(observed)) {
      return null;
    }
    return source;
  }

  @override
  Widget build(BuildContext context) {
    final strings = AppLocalizations.of(context);
    final source = _matchedCoverage;
    final date = DateFormat.yMMMd(strings.localeName);
    final range = source?.lastRange;
    final success = source?.lastSuccessAt;
    final absent = source == null
        ? strings.collectionStatusUnavailable
        : strings.noCompleteCalendarReadYet;
    final failure = source?.failure;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (failure != null)
          Padding(
            padding: const EdgeInsets.only(bottom: FloeSpace.sm),
            child: Text(
              switch (failure) {
                DayCalendarFailure.source_changed =>
                  strings.collectionSourceChanged,
                DayCalendarFailure.source_fenced =>
                  strings.collectionSourcePaused,
                _ =>
                  success == null
                      ? strings.collectionFailedWithoutSavedData
                      : strings.couldNotCollectEventsShowingTheLast,
              },
              style: FloeType.bodySmall.copyWith(color: FloePalette.warning600),
            ),
          ),
        for (final entry in <String, String>{
          strings.lastCompleteCalendarRange: range == null
              ? absent
              : strings.storedRange(
                  date.format(range.startDate),
                  date.format(range.endDateExclusive),
                ),
          strings.lastCompleteCalendarRead: success == null
              ? absent
              : DateFormat.yMMMd(strings.localeName)
                    .add_jm()
                    .format(success.toLocal()),
        }.entries)
          Padding(
            padding: const EdgeInsets.symmetric(vertical: FloeSpace.md),
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Expanded(
                  child: Text(
                    entry.key,
                    style: FloeType.bodySmall.copyWith(
                      color: FloePalette.neutral600,
                    ),
                  ),
                ),
                Expanded(child: Text(entry.value, style: FloeType.bodySmall)),
              ],
            ),
          ),
      ],
    );
  }
}
