import { X } from 'lucide-react';
import { mascotUrl } from '../../assets.js';
import { SQUIRCLE_RADIUS, SquircleButton, SquircleSurface } from '../../primitives.jsx';

export function TaskSuggestionCard({ onDismiss }) {
  return (
    <SquircleSurface
      radius={SQUIRCLE_RADIUS.card}
      className="rail-card-border"
      contentClassName="rail-card detail-suggestion"
    >
      <div className="suggestion-heading">
        <div className="floe-attribution">
          <img src={mascotUrl} alt="" />
          <span>Floe suggests</span>
        </div>
        <SquircleButton
          className="close-button"
          aria-label="Dismiss suggestion"
          onClick={() => onDismiss()}
        >
          <X size={18} />
        </SquircleButton>
      </div>
      <p>Team retro starts at 3:30 PM. Review the launch brief first?</p>
      <div className="inline-actions">
        <SquircleButton className="primary-button">Review now</SquircleButton>
        <SquircleButton className="quiet-action">Snooze</SquircleButton>
      </div>
    </SquircleSurface>
  );
}
