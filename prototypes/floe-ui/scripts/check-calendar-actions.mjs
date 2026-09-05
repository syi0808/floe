import assert from 'node:assert/strict';
import { actionReducer, initialAction, actionEvent } from '../src/calendar-action-state.js';

let assertions = 0;
function check(value, expected) {
  assert.deepEqual(value, expected);
  assertions += 1;
}
function approve(action) {
  return actionReducer(action, { type: 'approve', now: Date.now(), connected: true });
}
function advance(action, ...types) {
  return types.reduce((current, type) => actionReducer(current, { type }), action);
}

const pending = initialAction();
check(advance(pending, 'created', 'imported', 'lookup'), pending);
const rejected = advance(pending, 'reject');
check(approve(rejected), rejected);
check(actionReducer(pending, { type: 'approve', connected: false }).status, 'denied');
check(actionReducer(pending, { type: 'approve', connected: true, now: pending.expiresAt }).status, 'expired');
const approved = approve(pending);
check(approve(approved), approved);
check(actionReducer(approved, { type: 'target', calendarId: 'personal' }), approved);
const succeeded = advance(approved, 'checked', 'created', 'imported');
check(succeeded.status, 'succeeded');
check(advance(succeeded, 'created', 'repropose', 'lookup'), succeeded);
check(actionEvent(succeeded).id, `focus-${succeeded.revision}`);

for (const scenario of ['conflict', 'denied']) {
  const blocked = advance(approve(initialAction(scenario)), 'checked', 'created');
  check(blocked.status, scenario);
  const fresh = advance(blocked, 'repropose');
  check(fresh.status, 'pending');
  check(fresh.revision, blocked.revision + 1);
}
for (const scenario of ['timeout', 'missing']) {
  const unknown = advance(approve(initialAction(scenario)), 'checked', 'created');
  check(unknown.status, 'unknown');
  check(advance(unknown, 'created', 'repropose'), unknown);
  const recovered = advance(unknown, 'lookup', 'found', 'imported');
  check(recovered.status, scenario === 'timeout' ? 'succeeded' : 'unknown');
  check(recovered.revision, unknown.revision);
}
const readError = advance(approve(initialAction('read-error')), 'checked', 'created', 'imported');
check(readError.status, 'read-error');
check(advance(readError, 'created'), readError);
check(advance(readError, 'retry-read', 'imported').status, 'succeeded');
console.log(`Calendar action state checks passed: ${assertions} assertions.`);
