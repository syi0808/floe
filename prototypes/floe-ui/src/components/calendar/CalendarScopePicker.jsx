import { SquircleButton } from '../../primitives.jsx';

export function CalendarScopePicker({ calendars, scope, selectedIds, onScope, onToggle, onSave, onCancel }) {
  return <>
    <p className="s1-body-copy">Choose what Floe reads. This never changes the calendars themselves.</p>
    <fieldset className="s1-scope-options">
      <legend>Calendar scope</legend>
      <label><input type="radio" name="calendar-scope" checked={scope === 'all'} onChange={() => onScope('all')} /> All calendars, including new ones</label>
      <label><input type="radio" name="calendar-scope" checked={scope === 'selected'} onChange={() => onScope('selected')} /> Only selected calendars</label>
    </fieldset>
    <div className="s1-scope-options">
      {calendars.map(calendar => <label key={calendar.id}><input type="checkbox" disabled={scope === 'all'} checked={scope === 'all' || selectedIds.includes(calendar.id)} onChange={() => onToggle(calendar.id)} />{calendar.account} · {calendar.name}</label>)}
    </div>
    <p className="s1-body-copy">{scope === 'all' ? 'New calendars join on the next refresh. Unavailable calendars keep their saved events.' : 'New calendars are not added automatically. Deselected calendars are removed only from Floe’s saved copy.'}</p>
    <div className="s1-actions"><SquircleButton className="secondary-button" onClick={onCancel}>Cancel</SquircleButton><SquircleButton className="primary-button" disabled={scope === 'selected' && selectedIds.length === 0} onClick={onSave}>Save calendar scope</SquircleButton></div>
  </>;
}
