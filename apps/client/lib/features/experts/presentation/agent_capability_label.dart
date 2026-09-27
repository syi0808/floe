String agentCapabilityTitle(String identifier) {
  if (identifier.contains('timeline')) return 'Calendar context';
  if (identifier.contains('schedule')) return 'Schedule planning';
  return 'Specialized assistance';
}

String agentCapabilityDescription(String identifier) {
  if (identifier.contains('timeline')) {
    return 'Allows Floe to understand events from the calendars you choose.';
  }
  if (identifier.contains('schedule')) {
    return 'Allows Floe to prepare schedule suggestions for you to review.';
  }
  return 'Allows Floe to use this ability when helping with related requests.';
}
