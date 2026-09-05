import { SquircleButton } from '../../primitives.jsx';
import { Checkbox, Radio } from '../ui/SelectionControl.jsx';

export function CalendarScopePicker({ calendars, scope, selectedIds, onScope, onToggle, onSave, onCancel }) {
  return <>
    <p className="s1-body-copy">Choose what Floe reads. This never changes the calendars themselves.</p>
    <fieldset className="s1-scope-options">
      <legend>Calendar scope</legend>
      <Radio name="calendar-scope" value="all" checked={scope === 'all'} onChange={() => onScope('all')} label="All calendars, including new ones" description="New calendars join on your next refresh." />
      <Radio name="calendar-scope" value="selected" checked={scope === 'selected'} onChange={() => onScope('selected')} label="Only selected calendars" description="Keep your selection exactly as it is." />
    </fieldset>
    <div className="s1-scope-options">
      {calendars.map(calendar => <Checkbox key={calendar.id} disabled={scope === 'all'} checked={scope === 'all' || selectedIds.includes(calendar.id)} onChange={() => onToggle(calendar.id)} label={calendar.name} description={calendar.account} />)}
    </div>
    <p className="s1-body-copy">{scope === 'all' ? 'New calendars join on the next refresh. Unavailable calendars keep their saved events.' : 'New calendars are not added automatically. Deselected calendars are removed only from Floe’s saved copy.'}</p>
    <div className="s1-actions"><SquircleButton className="secondary-button" onClick={onCancel}>Cancel</SquircleButton><SquircleButton className="primary-button" disabled={scope === 'selected' && selectedIds.length === 0} onClick={onSave}>Save calendar scope</SquircleButton></div>
  </>;
}
