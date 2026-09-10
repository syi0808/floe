part of '../personal_day_screen.dart';

class _FailureDay extends StatelessWidget {
  const _FailureDay({required this.retry, required this.message});
  final VoidCallback retry;
  final String? message;
  @override
  Widget build(BuildContext context) => Center(
    child: FloeSquircle(
      padding: EdgeInsets.all(FloeSpace.xl),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(Icons.error_outline, color: FloePalette.error600),
          SizedBox(height: FloeSpace.md),
          Text(AppLocalizations.of(context).couldNotLoadYourDay),
          SizedBox(height: FloeSpace.sm),
          SelectableText(
            message ?? AppLocalizations.of(context).anUnknownErrorOccurred,
            textAlign: TextAlign.center,
          ),
          SizedBox(height: FloeSpace.base),
          FloeButton.filled(
            onPressed: retry,
            icon: Icon(Icons.refresh),
            child: Text(AppLocalizations.of(context).tryAgain),
          ),
        ],
      ),
    ),
  );
}

class _ErrorNotice extends StatelessWidget {
  const _ErrorNotice({required this.message, required this.dismiss});
  final String message;
  final VoidCallback dismiss;
  @override
  Widget build(BuildContext context) => Padding(
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
          Expanded(child: Text(message)),
          FloeButton.text(
            onPressed: dismiss,
            child: Text(AppLocalizations.of(context).close),
          ),
        ],
      ),
    ),
  );
}

void _showComingSoon(BuildContext context) {
  FloeToastHost.of(context).show(
    title: AppLocalizations.of(context).thisFeatureIsNotAvailableYet,
    tone: FloeToastTone.info,
  );
}

String _time(BuildContext context, DateTime value) =>
    MaterialLocalizations.of(context).formatTimeOfDay(
      TimeOfDay.fromDateTime(value),
      alwaysUse24HourFormat: MediaQuery.alwaysUse24HourFormatOf(context),
    );

String _date(BuildContext context, DateTime value) =>
    DateFormat.MMMEd(AppLocalizations.of(context).localeName).format(value);

TaskItem? _taskById(DaySnapshot snapshot, String? id) {
  if (id == null) return null;
  for (final task in snapshot.items.whereType<TaskItem>()) {
    if (task.id == id) return task;
  }
  return null;
}
