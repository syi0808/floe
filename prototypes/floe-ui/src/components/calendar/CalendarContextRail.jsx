import { ArrowRight, Link2 } from 'lucide-react';
import { CalendarSurface as Surface } from './CalendarSurface.jsx';

export function CalendarContextRail({ taskDone, onTaskChange, localNotes, hasCache, onNavigate }) {
  return (
    <aside className="s1-side-stack">
      <Surface>
        <div className="s1-card-heading">
          <h2>Your own rhythm</h2>
        </div>
        <p className="s1-body-copy">A few things that belong to you, not your calendar.</p>
        <label className={`s1-local-task ${taskDone ? 'done' : ''}`}>
          <input
            type="checkbox"
            checked={taskDone}
            onChange={(event) => onTaskChange(event.target.checked)}
          />
          <span>
            Finish the launch brief<small>One good thing to move forward</small>
          </span>
        </label>
        <button className="s1-text-link" onClick={() => onNavigate('tasks')}>
          <span>See your tasks</span> <ArrowRight size={15} />
        </button>
      </Surface>
      <Surface>
        <div className="s1-card-heading">
          <h2>A note to self</h2>
        </div>
        <p className="s1-personal-note">
          Leave a little room between things. Not every empty space needs filling.
        </p>
        {localNotes.map((note, index) => (
          <p className="s1-captured-note" key={index}>
            {note}
          </p>
        ))}
        <small>Saved in Floe · stays when you disconnect</small>
      </Surface>
      {hasCache && (
        <div className="s1-provenance-hint">
          <Link2 size={15} />
          <p>
            Wondering where an event came from?
            <br />
            Open it to see its source and time zone.
          </p>
        </div>
      )}
    </aside>
  );
}
