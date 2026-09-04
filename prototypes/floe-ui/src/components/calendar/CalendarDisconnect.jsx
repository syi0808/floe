import { Check, Info } from 'lucide-react';
import { SquircleButton } from '../../primitives.jsx';

export function CalendarDisconnect({ onClose, onDisconnect }) {
  return (
    <>
      <p className="s1-body-copy">
        Remove the saved copies of all connected calendars from this Floe prototype and stop reading
        them.
      </p>
      <ul className="s1-permission-list">
        <li>
          <Check size={17} /> Your external calendar is untouched
        </li>
        <li>
          <Check size={17} /> Your Floe tasks and notes stay here
        </li>
        <li>
          <Info size={17} /> OS permission is managed separately in Settings
        </li>
      </ul>
      <div className="s1-modal-actions">
        <SquircleButton className="secondary-button" onClick={() => onClose()}>
          Keep connected
        </SquircleButton>
        <SquircleButton
          className="s1-danger-button"
          onClick={() => {
            onDisconnect();
          }}
        >
          Disconnect
        </SquircleButton>
      </div>
    </>
  );
}
