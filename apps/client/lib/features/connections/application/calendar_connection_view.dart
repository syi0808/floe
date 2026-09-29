import 'package:floe_client/features/connections/domain/source_connection.dart';
import 'package:floe_client/features/day/domain/day_models.dart';

final class ConnectedCalendar {
  const ConnectedCalendar({
    required this.id,
    required this.name,
    this.error,
    this.lastSuccessAt,
  });

  final String id;
  final String name;
  final String? error;
  final DateTime? lastSuccessAt;

  String? get account {
    final separator = name.indexOf(' · ');
    return separator > 0 ? name.substring(0, separator) : null;
  }

  String get title {
    final separator = name.indexOf(' · ');
    return separator > 0 ? name.substring(separator + 3) : name;
  }
}

final class CalendarConnectionView {
  const CalendarConnectionView({
    required this.connectionId,
    required this.deviceId,
    required this.provider,
    required this.revision,
    this.isServing = true,
    this.sourceAuthority,
    required this.calendars,
    this.lastSuccessAt,
    this.error,
    this.rangeStart,
    this.rangeEnd,
    this.includeAll = false,
  });

  factory CalendarConnectionView.compose(
    SourceConnection source,
    CalendarMirrorState? mirror,
  ) {
    final observed =
        mirror?.sourceConnectionId == source.connectionId &&
            mirror?.provider == source.provider
        ? mirror
        : null;
    return CalendarConnectionView(
      connectionId: source.connectionId,
      deviceId: source.executionOwnerId,
      provider: source.provider,
      revision: source.revision,
      isServing: source.isServing,
      sourceAuthority: source.sourceAuthority,
      calendars: source.resources
          .map((resource) {
            final status = observed?.sourceStatuses[resource.handle];
            return ConnectedCalendar(
              id: resource.handle,
              name: resource.label,
              error: status?.error,
              lastSuccessAt: status?.lastSuccessAt,
            );
          })
          .toList(growable: false),
      lastSuccessAt: observed?.lastSuccessAt,
      error: observed?.error,
      rangeStart: observed?.rangeStart,
      rangeEnd: observed?.rangeEnd,
      includeAll: source.includeAll,
    );
  }

  final String connectionId;
  final String deviceId;
  final String provider;
  final int revision;
  final bool isServing;
  final SourceAuthority? sourceAuthority;
  final DateTime? lastSuccessAt;
  final String? error;
  final String? rangeStart;
  final String? rangeEnd;
  final List<ConnectedCalendar> calendars;
  final bool includeAll;

  List<ConnectedCalendar> get connectedCalendars => calendars;
  List<String> get selectedCalendarIds =>
      calendars.map((calendar) => calendar.id).toList(growable: false);
}
