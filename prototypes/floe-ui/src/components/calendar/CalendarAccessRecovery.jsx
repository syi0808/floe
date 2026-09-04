import { ShieldCheck, ExternalLink } from 'lucide-react';
import { SquircleButton } from '../../primitives.jsx';

export function CalendarAccessRecovery({ onClose, onRefresh }) {
  return (
    <>
      <p className="s1-body-copy">
        Your saved events are safe. To read changes again, allow Calendar access in macOS Settings,
        then return to Floe.
      </p>
      <ol className="s1-settings-steps">
        <li>Open System Settings</li>
        <li>Privacy & Security → Calendars</li>
        <li>Enable full access for Floe</li>
      </ol>
      <div className="s1-note">
        <ShieldCheck size={18} />
        <p>
          This demo won’t open or change your settings. The real app should recheck access when you
          return.
        </p>
      </div>
      <div className="s1-modal-actions">
        <SquircleButton className="secondary-button" onClick={() => onClose()}>
          Keep saved data
        </SquircleButton>
        <SquircleButton
          className="primary-button"
          onClick={() => {
            onRefresh();
          }}
        >
          Simulate access restored <ExternalLink size={15} />
        </SquircleButton>
      </div>
    </>
  );
}
