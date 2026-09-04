import { ArrowRight } from 'lucide-react';
import { useId } from 'react';
import { SquircleButton } from '../../primitives.jsx';
import { CalendarSurface as Surface } from './CalendarSurface.jsx';

export function CalendarCapture({ value, onChange, onSubmit }) {
  const inputId = useId();
  return (
    <Surface className="s1-capture-card">
      <form
        className="s1-capture"
        onSubmit={(event) => {
          event.preventDefault();
          if (value.trim()) onSubmit(value.trim());
        }}
      >
        <label htmlFor={inputId}>+</label>
        <input
          id={inputId}
          value={value}
          onChange={(event) => onChange(event.target.value)}
          placeholder="A thought for your day…"
          aria-label="Capture a local note"
        />
        <SquircleButton disabled={!value.trim()} type="submit" aria-label="Save local note">
          <ArrowRight size={18} />
        </SquircleButton>
      </form>
    </Surface>
  );
}
