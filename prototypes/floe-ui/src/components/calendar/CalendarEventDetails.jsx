import { Clock3, LockKeyhole } from 'lucide-react';
import { SquircleBlock, SquircleButton } from '../../primitives.jsx';

export function CalendarEventDetails({
  event,
  calendar,
  dateLabel,
  dateShort,
  dayOffset,
  stale,
  onClose,
}) {
  return (
    <>
      <div className="s1-detail-source">
        <span className={`tone-dot ${calendar.color}`} />
        {calendar.name} · {calendar.account}
        <SquircleBlock className="s1-pill" radius={8}>
          <LockKeyhole size={12} /> Read-only
        </SquircleBlock>
      </div>
      <p className="s1-event-purpose">
        {event.detail || 'A full day, not a midnight appointment.'}
      </p>
      <SquircleBlock className="s1-detail-time" radius={22}>
        <Clock3 size={20} />
        <div>
          <strong>{event.time}</strong>
          <span>{dateLabel} · Asia/Seoul</span>
        </div>
      </SquircleBlock>
      <dl className="s1-facts">
        <div>
          <dt>Original time</dt>
          <dd>
            {event.id === 'remote' && dayOffset !== 0
              ? `${dateShort}, 12:00 – 12:45 AM PDT`
              : event.original.replace('Sep 4', dateShort)}
          </dd>
        </div>
        <div>
          <dt>Source time zone</dt>
          <dd>{event.timezone}</dd>
        </div>
        {event.recurring && (
          <div>
            <dt>Repeats</dt>
            <dd>Weekdays · this occurrence only</dd>
          </div>
        )}
        <div>
          <dt>Last collected</dt>
          <dd>{stale ? 'Saved at 2:28 PM · may be out of date' : 'Today at 2:28 PM'}</dd>
        </div>
        {event.allDay && (
          <div>
            <dt>All-day boundary</dt>
            <dd>{event.endDateExclusive || 'Sep 5, 2026 · exclusive'}</dd>
          </div>
        )}
      </dl>
      <div className="s1-note">
        <LockKeyhole size={17} />
        <p>
          Manage this event in its original calendar. Floe has no edit or delete action for imported
          events.
        </p>
      </div>
      <details className="s1-source-details">
        <summary>Source details</summary>
        <SquircleBlock radius={22} asChild>
          <dl>
            <dt>Connection / Person</dt>
            <dd>demo-macos / You</dd>
            <dt>External occurrence ID</dt>
            <dd>
              fixture:{calendar.id}:{event.id}:2026-09-
              {String(4 + dayOffset).padStart(2, '0')}
            </dd>
            <dt>Change token</dt>
            <dd>fixture-revision-04</dd>
            <dt>Integration</dt>
            <dd>Fixture · not a live EventKit record</dd>
          </dl>
        </SquircleBlock>
      </details>
      <div className="s1-modal-actions">
        <SquircleButton className="primary-button" onClick={() => onClose()}>
          Back to my day
        </SquircleButton>
      </div>
    </>
  );
}
