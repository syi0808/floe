Map<String, dynamic> interactionFixture({
  required String id,
  required String sessionId,
  int createdAt = 1000,
  int revision = 1,
  String state = 'pending',
}) => {
  'interaction_id': id,
  'session_id': sessionId,
  'origin_run_id': '00000000-0000-4000-8000-000000000099',
  'interaction_kind': 'source_access',
  'state': state,
  'revision': revision,
  'target_digest': 'a' * 64,
  'created_at': DateTime.fromMillisecondsSinceEpoch(
    createdAt,
    isUtc: true,
  ).toIso8601String(),
  'expires_at': DateTime.fromMillisecondsSinceEpoch(
    createdAt + 3600000,
    isUtc: true,
  ).toIso8601String(),
  'requirement': {
    'kind': 'source_access',
    'reason': 'reconnect',
    'source_id': 'floe.source.calendar',
    'connection_id': null,
    'consumer': 'manager',
    'purpose': 'assistant',
    'inline': false,
  },
  'target': {
    'kind': 'navigation_only',
    'destination': 'connection_settings',
    'source_label': 'Test calendar',
    'source_ref': null,
  },
  'actions': ['open_connection'],
};

Map<String, dynamic> interactionPageFixture(
  String sessionId,
  List<Map<String, dynamic>> interactions, {
  bool more = false,
  bool terminalCursor = false,
}) => {
  'kind': 'interaction_list',
  'session_id': sessionId,
  'interactions': interactions,
  'next_cursor': more
      ? {
          'terminal': terminalCursor,
          'created_at_unix_ms': DateTime.parse(
            interactions.last['created_at'] as String,
          ).millisecondsSinceEpoch,
          'interaction_id': interactions.last['interaction_id'],
        }
      : null,
};
