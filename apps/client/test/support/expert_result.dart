Map<String, Object?> expertResultFixture() => {
  'schema_version': 1,
  'invocation_id': 'call-1',
  'instance_id': 'instance-fixture',
  'person_id': 'test',
  'assignment_id': 'schedule-fixture',
  'package': {'kind': 'expert', 'id': 'floe.schedule', 'version': '1.0.0'},
  'view_handle': 'view-fixture',
  'source_handle': 'fixture.synthetic.timeline',
  'data_class': 'synthetic',
  'expires_at_unix_ms': 4102444800000,
  'insights': [
    {
      'kind': 'commitment',
      'evidence_handle': 'event-fixture',
      'untrusted_title': 'Design review',
      'starts_at_unix_ms': 36000000,
      'ends_at_unix_ms': 39600000,
    },
    {
      'kind': 'focus_window',
      'starts_at_unix_ms': 39600000,
      'ends_at_unix_ms': 43200000,
    },
  ],
  'action_proposals': <Object?>[],
  'state_revision': 1,
  'view_calls': 1,
};
