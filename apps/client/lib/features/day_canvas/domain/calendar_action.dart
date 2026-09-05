enum CalendarActionDecision { approve, reject }

enum CalendarActionStatus {
  pending,
  approved,
  rejected,
  executing,
  blocked,
  unknown,
  succeeded;

  bool get canDecide => this == pending;
}

final class CalendarAction {
  CalendarAction.fromJson(Map<String, dynamic> json)
    : id = json['id'] as String,
      personId = json['person_id'] as String,
      provider = json['provider'] as String,
      calendarId = json['calendar_id'] as String,
      calendarName = json['calendar_name'] as String,
      title = json['title'] as String,
      startsAt = DateTime.parse(
        (json['schedule'] as Map)['starts_at'] as String,
      ),
      endsAt = DateTime.parse((json['schedule'] as Map)['ends_at'] as String),
      timezone = (json['schedule'] as Map)['timezone'] as String,
      connectionRevision = json['connection_revision'] as int,
      createdAt = DateTime.parse(json['created_at'] as String),
      expiresAt = DateTime.parse(json['expires_at'] as String),
      approvedAt = json['approved_at'] == null
          ? null
          : DateTime.parse(json['approved_at'] as String),
      executionId = json['execution_id'] as String,
      status = CalendarActionStatus.values.byName(
        (json['state'] as Map)['status'] as String,
      ),
      reason = (json['state'] as Map)['reason'] as String?,
      externalId = (json['state'] as Map)['external_id'] as String?;

  final String id;
  final String personId;
  final String provider;
  final String calendarId;
  final String calendarName;
  final String title;
  final DateTime startsAt;
  final DateTime endsAt;
  final String timezone;
  final int connectionRevision;
  final DateTime createdAt;
  final DateTime expiresAt;
  final DateTime? approvedAt;
  final String executionId;
  final CalendarActionStatus status;
  final String? reason;
  final String? externalId;
}
