String agentCapabilityTitle(String identifier, {String? kind}) {
  if (identifier.contains('timeline')) return 'Calendar context';
  if (identifier.contains('schedule')) return 'Schedule planning';
  return kind == 'tool' ? 'Connected information' : 'Specialized assistance';
}

String agentCapabilityDescription(String identifier, {String? kind}) {
  if (identifier.contains('timeline')) {
    return 'Allows Floe to understand events from the calendars you choose.';
  }
  if (identifier.contains('schedule')) {
    return 'Allows Floe to prepare schedule suggestions for you to review.';
  }
  return kind == 'tool'
      ? 'Allows Floe to use this connected information in conversations.'
      : 'Allows Floe to use this ability when helping with related requests.';
}
