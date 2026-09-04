import {
  ArrowRight,
  CalendarDays,
  Clock3,
  Database,
  Link2,
  LoaderCircle,
  LockKeyhole,
  WifiOff,
} from 'lucide-react';
import { SquircleButton } from '../../primitives.jsx';

export function CalendarStatusBanner({
  phase,
  dateLabel,
  onConnect,
  onRefresh,
  onSettings,
  onPicker,
}) {
  const states = {
    disconnected: [
      'neutral',
      Link2,
      'A calendar, when you’re ready.',
      'Bring your existing events into Floe. Your own tasks and notes work without a connection.',
      'Connect Calendar',
      onConnect,
    ],
    syncing: [
      'neutral',
      LoaderCircle,
      'Reading your calendars…',
      'Your last saved events stay visible until this read finishes. No external changes are made.',
    ],
    cached: [
      'neutral',
      Clock3,
      'Your day, saved on this Mac.',
      'Last collected today at 2:28 PM. Refresh to see changes made since then.',
      'Refresh',
      onRefresh,
    ],
    offline: [
      'warning',
      WifiOff,
      'We couldn’t read the latest changes.',
      'Showing your saved events from 2:28 PM. Nothing was removed. Try again when Calendar is available.',
      'Try again',
      onRefresh,
    ],
    denied: [
      'warning',
      LockKeyhole,
      'Calendar access wasn’t granted.',
      'That’s okay. Your local day still works. Allow access in Settings when you want to connect.',
      'Review access',
      onSettings,
    ],
    revoked: [
      'warning',
      LockKeyhole,
      'Calendar access has changed.',
      'New reads are paused. Your last saved events are still here, but may be out of date.',
      'Reconnect',
      onSettings,
    ],
    missing: [
      'warning',
      CalendarDays,
      'Some calendars need attention.',
      'An account or calendar may be unavailable. Its saved events remain; other calendars are still included.',
      'View calendars',
      onPicker,
    ],
    noCalendars: [
      'neutral',
      CalendarDays,
      'No calendars found on this Mac.',
      'Add one in macOS Calendar, then refresh in Floe. Floe will not create a calendar for you.',
      'View calendars',
      onPicker,
    ],
    uncollected: [
      'neutral',
      CalendarDays,
      `${dateLabel} hasn’t been read yet.`,
      'Other saved dates are untouched. Read this date to find out what’s on your calendar.',
      'Read this date',
      onRefresh,
    ],
    loadError: [
      'warning',
      Database,
      'A local read needs attention.',
      'Calendar refresh is paused until Floe can load your local data.',
    ],
  };
  const [tone, Icon, title, description, action, handler] = states[phase];
  return (
    <div className={`s1-banner ${tone}`} role={tone === 'warning' ? 'alert' : 'status'}>
      <Icon size={19} className={phase === 'syncing' ? 's1-spin' : ''} />
      <div>
        <strong>{title}</strong>
        <p>{description}</p>
      </div>
      {action && (
        <SquircleButton className="s1-banner-action" onClick={handler}>
          {action}
          <ArrowRight size={14} />
        </SquircleButton>
      )}
    </div>
  );
}
