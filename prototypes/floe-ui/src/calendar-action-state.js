export const actionScenarios = ['ready', 'conflict', 'denied', 'expired', 'timeout', 'missing', 'read-error'];

export function initialAction(scenario = 'ready') {
  return { status: 'pending', scenario, revision: 1, calendarId: 'work', expiresAt: Date.now() + 15 * 60_000 };
}

export function actionReducer(action, event) {
  if (event.type === 'target' && action.status === 'pending') {
    return { ...action, calendarId: event.calendarId, revision: action.revision + 1 };
  }
  if (event.type === 'reject' && action.status === 'pending') return { ...action, status: 'rejected' };
  if (event.type === 'approve' && action.status === 'pending') {
    if (!event.connected) return { ...action, status: 'denied' };
    if (event.now >= action.expiresAt || action.scenario === 'expired') return { ...action, status: 'expired' };
    return { ...action, status: 'checking', approvedAt: event.now };
  }
  if (event.type === 'checked' && action.status === 'checking') {
    return { ...action, status: ['conflict', 'denied'].includes(action.scenario) ? action.scenario : 'creating' };
  }
  if (event.type === 'created' && action.status === 'creating') {
    return { ...action, status: ['timeout', 'missing'].includes(action.scenario) ? 'unknown' : 'importing' };
  }
  if (event.type === 'lookup' && action.status === 'unknown') return { ...action, status: 'looking-up' };
  if (event.type === 'found' && action.status === 'looking-up') {
    return { ...action, status: action.scenario === 'missing' ? 'unknown' : 'importing', checked: true };
  }
  if (event.type === 'imported' && action.status === 'importing') {
    return { ...action, status: action.scenario === 'read-error' && !action.readRetried ? 'read-error' : 'succeeded' };
  }
  if (event.type === 'retry-read' && action.status === 'read-error') return { ...action, status: 'importing', readRetried: true };
  if (event.type === 'repropose' && ['conflict', 'denied', 'expired', 'rejected'].includes(action.status)) {
    return { ...initialAction(), revision: action.revision + 1, calendarId: action.calendarId };
  }
  return action;
}

export function actionEvent(action) {
  return {
    id: `focus-${action.revision}`,
    externalId: `fixture-calendar-event-${action.revision}`,
    proposalId: `fixture-proposal-${action.revision}`,
    executionId: `fixture-execution-${action.revision}`,
    calendarId: action.calendarId,
    title: 'A little room to focus',
    time: '2:45 – 3:30 PM',
    startMinutes: 14 * 60 + 45,
    endMinutes: 15 * 60 + 30,
    detail: 'Approved by you · simulated Calendar re-import',
    timezone: 'Asia/Seoul',
    original: 'Sep 4, 2:45 – 3:30 PM KST',
  };
}
