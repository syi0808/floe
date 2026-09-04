import { ShieldCheck } from 'lucide-react';
import { SquircleButton } from '../../primitives.jsx';
import { CalendarSurface as Surface } from './CalendarSurface.jsx';

export function CalendarPermission({ onDeny, onRefresh }) {
  return (
    <>
      <span className="s1-demo-label">Simulated OS handoff</span>
      <p className="s1-body-copy">
        In the app, macOS asks whether Floe can access your calendars here. This preview does not
        request device permissions.
      </p>
      <Surface>
        <ShieldCheck size={26} />
        <h3>“Floe” would like full access to Calendar</h3>
        <p className="s1-body-copy">
          Floe reads all calendars on this Mac. It won’t change your external events.
        </p>
      </Surface>
      <div className="s1-modal-actions">
        <SquircleButton
          className="secondary-button"
          onClick={() => {
            onDeny();
          }}
        >
          Simulate denial
        </SquircleButton>
        <SquircleButton className="primary-button" onClick={() => onRefresh()}>
          Simulate allow
        </SquircleButton>
      </div>
    </>
  );
}
