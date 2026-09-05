import { ArrowRight, Link2 } from 'lucide-react';
import { CalendarSurface as Surface } from './CalendarSurface.jsx';
import { Checkbox } from '../ui/SelectionControl.jsx';

export function CalendarContextRail({ taskDone, onTaskChange, hasCache, onNavigate }) {
  return (
    <aside className="s1-side-stack">
      <Surface>
        <div className="s1-card-heading">
          <h2>Your own rhythm</h2>
        </div>
        <div className={`s1-local-task ${taskDone ? 'done' : ''}`}>
          <Checkbox checked={taskDone} onChange={onTaskChange} label="Finish the launch brief" description="One good thing to move forward" />
        </div>
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
