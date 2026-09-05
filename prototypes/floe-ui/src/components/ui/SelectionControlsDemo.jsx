import { useState } from 'react';
import { Dropdown, Select } from './Select.jsx';

export function SelectionControlsDemo() {
  const [calendar, setCalendar] = useState('personal');
  const [feedback, setFeedback] = useState('');
  return (
    <section className="selection-controls-demo" aria-labelledby="selection-preview-title">
      <h2 id="selection-preview-title">Interaction preview</h2>
      <p>Prototype only. Try a selection or a menu action; no calendar or server settings change.</p>
      <div className="selection-controls-demo-grid">
        <Select label="Calendar" value={calendar} options={[{ value: 'personal', label: 'Personal', description: 'iCloud' }, { value: 'work', label: 'Work', description: 'iCloud' }, { value: 'holidays', label: 'Holidays', description: 'Read-only', disabled: true }]} onChange={(value) => { setCalendar(value); setFeedback(`Preview calendar: ${value === 'personal' ? 'Personal' : 'Work'}.`); }} />
        <Dropdown label="Preview actions" items={[{ value: 'review', label: 'Review planned event' }, { value: 'copy', label: 'Copy event details' }, { value: 'share', label: 'Share event', description: 'Not available in this preview', disabled: true }]} onAction={(value) => setFeedback(value === 'review' ? 'Review selected — this preview does not create an event.' : 'Copy selected — this preview does not change your clipboard.')} />
      </div>
      <p className="selection-controls-feedback" role="status">{feedback}</p>
    </section>
  );
}
