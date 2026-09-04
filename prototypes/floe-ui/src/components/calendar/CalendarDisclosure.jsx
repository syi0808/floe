import { CalendarDays, Check, LockKeyhole, Info, ArrowRight } from 'lucide-react';
import { SquircleButton } from '../../primitives.jsx';

export function CalendarDisclosure({ onClose, onPermission }) {
  return (
    <>
      <div className="s1-modal-icon">
        <CalendarDays size={30} />
      </div>
      <p className="s1-body-copy">
        Connect all calendars already on this Mac. Floe will keep a local copy of their events so
        your day is still there when you’re offline.
      </p>
      <ul className="s1-permission-list">
        <li>
          <Check size={17} /> Read titles, times, and time zones
        </li>
        <li>
          <Check size={17} /> Keep source details with each event
        </li>
        <li>
          <LockKeyhole size={17} /> Never create, edit, or delete external events
        </li>
      </ul>
      <div className="s1-note">
        <Info size={18} />
        <p>
          macOS requires “Full Access” to read events. You’ll see a read-and-write permission, but
          Floe only uses it to read. All available calendars are included, including calendars added
          later when you refresh.
        </p>
      </div>
      <div className="s1-modal-actions">
        <SquircleButton className="secondary-button" onClick={() => onClose()}>
          Not now
        </SquircleButton>
        <SquircleButton className="primary-button" onClick={() => onPermission()}>
          Continue <ArrowRight size={16} />
        </SquircleButton>
      </div>
    </>
  );
}
