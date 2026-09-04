import { LockKeyhole } from 'lucide-react';
import { SquircleButton } from '../../primitives.jsx';

export function CalendarEvent({ event, calendar, pixelsPerMinute, disabled = false, onSelect }) {
  return (
    <SquircleButton
      radius={Math.min(16, ((event.endMinutes - event.startMinutes) * pixelsPerMinute) / 4)}
      title={`${event.title} · ${event.time}`}
      disabled={disabled}
      className={`s1-event s1-event-${calendar.color}`}
      data-density={
        (event.endMinutes - event.startMinutes) * pixelsPerMinute < 24
          ? 'micro'
          : (event.endMinutes - event.startMinutes) * pixelsPerMinute < 40
            ? 'compact'
            : (event.endMinutes - event.startMinutes) * pixelsPerMinute < 58
              ? 'medium'
              : 'full'
      }
      aria-label={`${event.title} · ${event.time} · ${calendar.name} · ${event.detail}`}
      style={{
        '--event-column': event.column,
        '--event-columns': event.columns,
        top: event.startMinutes * pixelsPerMinute,
        height: (event.endMinutes - event.startMinutes) * pixelsPerMinute,
      }}
      data-overlapping={event.columns > 1}
      onClick={() => onSelect(event)}
    >
      <span className={`tone-dot ${calendar.color}`} />
      <span>
        <strong>{event.title}</strong>
        <time>{event.time}</time>
        <small>
          {calendar.name} · {event.detail}
          {event.recurring ? ' · Repeats' : ''}
        </small>
      </span>
      <LockKeyhole size={13} />
    </SquircleButton>
  );
}
