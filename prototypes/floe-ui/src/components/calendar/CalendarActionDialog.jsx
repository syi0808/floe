import { ShieldCheck } from 'lucide-react';
import { SquircleButton, SquircleBlock } from '../../primitives.jsx';
import { Modal } from '../ui/Modal.jsx';
import { DotSpinner } from '../ui/DotSpinner.jsx';
import { Select } from '../ui/Select.jsx';
import './calendar-action.css';

const messages = {
  pending: ['Make a little room?', 'Review the exact destination and time. Nothing changes until you approve.'],
  checking: ['One last check.', 'Checking your permission, calendar and current schedule before creating.'],
  creating: ['Adding your focus time.', 'Your approval is recorded. Closing this dialog won’t send another request.'],
  importing: ['Created. Bringing it into your day.', 'Reading Calendar again, not creating another event.'],
  succeeded: ['A little room, reserved.', 'The created event has been collected back into your day.'],
  rejected: ['Not this time.', 'Nothing was created. You can ask for a fresh suggestion later.'],
  conflict: ['Your day has changed.', 'Another event now overlaps this time. Nothing was created. Review a fresh suggestion before trying again.'],
  denied: ['Calendar access needs attention.', 'Write access is unavailable. Nothing was created. Restore access, then review a fresh suggestion.'],
  expired: ['This suggestion has expired.', 'Nothing was created. Get a fresh suggestion and approve its details again.'],
  unknown: ['Let’s check before trying again.', 'Calendar may have saved the event, but its response was lost. We won’t create another one.'],
  'looking-up': ['Looking for the original event.', 'Checking Calendar for this exact event. No new event is being created.'],
  'read-error': ['Created, but not collected yet.', 'Your event is saved in Calendar. Retry the read to show it here—never create it again.'],
};

export function CalendarActionDialog({ action, calendars, connected, onAction, onClose }) {
  const [title, description] = messages[action.status];
  const busy = ['checking', 'creating', 'importing', 'looking-up'].includes(action.status);
  const created = ['succeeded', 'importing', 'read-error'].includes(action.status);
  const calendar = calendars.find((entry) => entry.id === action.calendarId);
  return (
    <Modal title={title} onClose={onClose}>
      <p className="s3-action-description" role="status" aria-live="polite">{description}</p>
      <SquircleBlock radius={22} className="s3-action-summary">
        <span className="s3-action-eyebrow">{created ? 'Calendar event' : 'Add one Calendar event'}</span>
        <strong>A little room to focus</strong>
        <span>Fri, Sep 4, 2026 · 2:45–3:30 PM</span>
        <span className="s3-action-caption">45 minutes</span>
      </SquircleBlock>
      {action.status === 'pending' ? <div className="s3-action-destination"><Select
        label="Target calendar"
        value={action.calendarId}
        options={calendars.filter((entry) => entry.id !== 'team').map((entry) => ({ value: entry.id, label: `${entry.account} · ${entry.name}` }))}
        onChange={(value) => onAction({ type: 'target', calendarId: value })}
      /></div> : <dl className="s1-facts"><div><dt>Destination</dt><dd>{calendar.account} · {calendar.name}</dd></div></dl>}
      <p className="s3-action-impact">Only this event. No guests, alerts or repeat schedule. Existing events stay unchanged.</p>
      {busy && <DotSpinner label="Checking action status…" />}
      {action.status === 'unknown' && action.checked && <p className="s3-action-warning">No exact match could be confirmed. Inspect the original calendar; this action stays unresolved. Do not create a replacement yet.</p>}
      <div className="s1-actions s3-action-buttons">
        {action.status === 'pending' && <>
          <SquircleButton className="secondary-button" onClick={() => onAction({ type: 'reject' })}>Decline</SquircleButton>
          <SquircleButton className="primary-button" disabled={!connected} onClick={() => onAction({ type: 'approve', connected, now: Date.now() })}>Approve & create</SquircleButton>
        </>}
        {['conflict', 'denied', 'expired', 'rejected'].includes(action.status) && <SquircleButton className="primary-button" disabled={!connected} onClick={() => onAction({ type: 'repropose' })}>Review a fresh suggestion</SquircleButton>}
        {action.status === 'unknown' && <SquircleButton className="primary-button" onClick={() => onAction({ type: 'lookup' })}>Check Calendar for this event</SquircleButton>}
        {action.status === 'read-error' && <SquircleButton className="primary-button" onClick={() => onAction({ type: 'retry-read' })}>Retry Calendar read</SquircleButton>}
        {action.status === 'succeeded' && <SquircleButton className="primary-button" onClick={onClose}>Back to my day</SquircleButton>}
      </div>
      <details className="s3-action-trace"><summary>Technical details</summary><dl className="s1-facts">
        <div><dt>Person</dt><dd>You · this device</dd></div>
        <div><dt>Approval window</dt><dd>15 minutes · details are fixed after approval</dd></div>
        <div><dt>Proposal</dt><dd>fixture-proposal-{action.revision}</dd></div>
        <div><dt>Execution</dt><dd>fixture-execution-{action.revision}</dd></div>
        <div><dt>State</dt><dd>{action.status}</dd></div>
        <div><dt>Approval</dt><dd>{action.approvedAt ? 'Explicitly approved by you · simulated' : 'Not approved'}</dd></div>
        <div><dt>External ID</dt><dd>{created ? `fixture-calendar-event-${action.revision}` : 'Not confirmed'}</dd></div>
      </dl></details>
      <div className="s1-note"><ShieldCheck size={17} /><p>Prototype simulation only. No model, EventKit call, real permission change or external write.</p></div>
    </Modal>
  );
}
