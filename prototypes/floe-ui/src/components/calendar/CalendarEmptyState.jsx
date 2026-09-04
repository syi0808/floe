import { SquircleButton } from '../../primitives.jsx';
import { mascotUrl } from '../../assets.js';

export function CalendarEmptyState({ phase, dateShort, onAction }) {
  return (
    <div className="s1-empty-day" role="status">
      <div className="s1-empty-content">
        <img src={mascotUrl} alt="" />
        <h3>
          {phase === 'syncing'
            ? 'Finding your day…'
            : phase === 'empty'
              ? 'A little breathing room.'
              : phase === 'uncollected'
                ? 'An unread page in your day.'
                : 'Your day starts here.'}
        </h3>
        <p>
          {phase === 'syncing'
            ? 'Reading all connected calendars. Your own tasks and notes stay available.'
            : phase === 'empty'
              ? `No events across your connected calendars for ${dateShort}. This date was checked successfully.`
              : phase === 'uncollected'
                ? 'No events have been collected for this date. It doesn’t mean your calendar is empty.'
                : 'Your own tasks and notes are ready. Connect a calendar when you want more context.'}
        </p>
        <SquircleButton
          className="secondary-button"
          disabled={phase === 'syncing'}
          onClick={onAction}
        >
          {phase === 'syncing'
            ? 'Reading…'
            : phase === 'empty'
              ? 'View connected calendars'
              : phase === 'uncollected'
                ? 'Read this date'
                : 'Connect Calendar'}
        </SquircleButton>
      </div>
    </div>
  );
}
