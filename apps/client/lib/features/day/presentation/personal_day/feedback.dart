part of '../personal_day_screen.dart';

class _FailureDay extends StatelessWidget {
  const _FailureDay({required this.retry, required this.message});
  final VoidCallback retry;
  final String? message;

  @override
  Widget build(BuildContext context) => LayoutBuilder(
    builder: (context, constraints) {
      final strings = AppLocalizations.of(context);
      final diagnostic = message?.trim().isNotEmpty == true
          ? message!
          : strings.anUnknownErrorOccurred;
      final maxCardWidth = constraints.maxWidth < 720
          ? constraints.maxWidth
          : 720.0;
      final horizontalPadding = 2 * FloeSpace.xl;
      final diagnosticWidth = maxCardWidth > horizontalPadding
          ? maxCardWidth - horizontalPadding
          : maxCardWidth;
      final twoLineDiagnosticPainter = TextPainter(
        text: TextSpan(text: 'M\nM', style: DefaultTextStyle.of(context).style),
        textDirection: Directionality.of(context),
        textScaler: MediaQuery.textScalerOf(context),
      );
      final double twoLineDiagnosticHeight;
      try {
        twoLineDiagnosticPainter.layout(maxWidth: diagnosticWidth);
        twoLineDiagnosticHeight = twoLineDiagnosticPainter.height;
      } finally {
        twoLineDiagnosticPainter.dispose();
      }
      final proportionalDiagnosticHeight = constraints.maxHeight * 0.3;
      final diagnosticViewportHeight =
          proportionalDiagnosticHeight > twoLineDiagnosticHeight
          ? proportionalDiagnosticHeight
          : twoLineDiagnosticHeight;

      return SingleChildScrollView(
        key: const Key('day-failure-card-scroll'),
        child: ConstrainedBox(
          constraints: BoxConstraints(minHeight: constraints.maxHeight),
          child: Center(
            child: ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 720),
              child: FloeSquircle(
                padding: const EdgeInsets.all(FloeSpace.xl),
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    const Icon(
                      Icons.error_outline,
                      color: FloePalette.error600,
                    ),
                    const SizedBox(height: FloeSpace.md),
                    Text(strings.couldNotLoadYourDay),
                    const SizedBox(height: FloeSpace.sm),
                    ConstrainedBox(
                      constraints: BoxConstraints(
                        maxHeight: diagnosticViewportHeight,
                      ),
                      child: SingleChildScrollView(
                        key: const Key('day-failure-diagnostic-scroll'),
                        child: SelectableText(
                          diagnostic,
                          textAlign: TextAlign.center,
                        ),
                      ),
                    ),
                    const SizedBox(height: FloeSpace.base),
                    FloeButton.filled(
                      onPressed: retry,
                      icon: const Icon(Icons.refresh),
                      child: Text(strings.tryAgain),
                    ),
                  ],
                ),
              ),
            ),
          ),
        ),
      );
    },
  );
}

class _ErrorNotice extends StatelessWidget {
  const _ErrorNotice({required this.message, required this.dismiss});
  final String message;
  final VoidCallback dismiss;
  @override
  Widget build(BuildContext context) {
    final compactLargeText =
        MediaQuery.sizeOf(context).width <= 360 &&
        MediaQuery.textScalerOf(context).scale(1) >= 2;
    final errorText = Text(
      message,
      maxLines: compactLargeText ? 2 : null,
      overflow: compactLargeText ? TextOverflow.ellipsis : null,
      semanticsLabel: message,
    );
    final messageWidget = compactLargeText
        ? Tooltip(
            message: message,
            excludeFromSemantics: true,
            child: errorText,
          )
        : errorText;
    return Padding(
      padding: EdgeInsets.only(top: FloeSpace.base),
      child: FloeSquircle(
        size: FloeSquircleSize.md,
        fill: FloePalette.error50,
        borderColor: FloePalette.coral100,
        padding: EdgeInsets.symmetric(
          horizontal: FloeSpace.base,
          vertical: FloeSpace.sm,
        ),
        child: Row(
          children: [
            Icon(Icons.error_outline, color: FloePalette.error600),
            SizedBox(width: FloeSpace.md),
            Expanded(child: messageWidget),
            FloeButton.text(
              onPressed: dismiss,
              child: Text(AppLocalizations.of(context).close),
            ),
          ],
        ),
      ),
    );
  }
}

String _time(BuildContext context, DateTime value) =>
    MaterialLocalizations.of(context).formatTimeOfDay(
      TimeOfDay.fromDateTime(value.toLocal()),
      alwaysUse24HourFormat: MediaQuery.alwaysUse24HourFormatOf(context),
    );

// Deadlines are instants from the owner, unlike the selected civil day. Render
// their local date and time together so a midnight/year crossing stays visible.
String _deadline(BuildContext context, DateTime instant) {
  final local = instant.toLocal();
  final date = DateFormat.yMMMEd(AppLocalizations.of(context).localeName)
      .format(local);
  return '$date · ${_time(context, local)}';
}

String _date(BuildContext context, DateTime value) =>
    DateFormat.MMMEd(AppLocalizations.of(context).localeName).format(value);

TaskItem? _taskById(DaySnapshot snapshot, String? id) {
  if (id == null) return null;
  for (final task in snapshot.items.whereType<TaskItem>()) {
    if (task.id == id) return task;
  }
  return null;
}
