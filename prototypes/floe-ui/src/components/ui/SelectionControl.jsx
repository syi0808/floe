import { useId } from 'react';
import './selection-control.css';

function SelectionControl({ type, checked, label, description, disabled, name, value, onChange, compact }) {
  const descriptionId = useId();
  return (
    <label className={`floe-choice floe-choice--${type}${compact ? ' floe-choice--compact' : ''}`}>
      <input className="floe-choice-input" type={type} name={name} value={value} checked={checked} disabled={disabled} aria-labelledby={`${descriptionId}-label`} aria-describedby={description ? descriptionId : undefined} onChange={(event) => onChange(event.target.checked)} />
      <span className="floe-choice-visual" aria-hidden="true">
        {type === 'checkbox' ? (
          <svg className="floe-choice-mark" viewBox="0 0 20 20" fill="none">
            <path d="m5 10 3.2 3.2L15 6.5" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
        ) : <span className="floe-choice-dot" />}
      </span>
      <span className="floe-choice-copy">
        <span className="floe-choice-label" id={`${descriptionId}-label`}>{label}</span>
        {description && <span className="floe-choice-description" id={descriptionId}>{description}</span>}
      </span>
    </label>
  );
}

export function Checkbox({ checked, label, description, disabled = false, name, value, onChange, compact = false }) {
  return <SelectionControl type="checkbox" checked={checked} label={label} description={description} disabled={disabled} name={name} value={value} onChange={onChange} compact={compact} />;
}

export function Radio({ checked, label, description, disabled = false, name, value, onChange }) {
  return <SelectionControl type="radio" checked={checked} label={label} description={description} disabled={disabled} name={name} value={value} onChange={onChange} />;
}
