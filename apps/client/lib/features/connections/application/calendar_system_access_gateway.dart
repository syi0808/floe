/// OS authorization only. None of these values grants Floe source access.
enum CalendarSystemAccess {
  allowed,
  notRequested,
  denied,
  restricted,
  writeOnly,
  unavailable,
}

abstract interface class CalendarSystemAccessGateway {
  /// Must not enumerate calendars, request permission, or mutate a Floe grant.
  Future<CalendarSystemAccess> inspect();

  /// Navigation only, invoked by the user's explicit Manage access action.
  Future<void> openSettings();
}
