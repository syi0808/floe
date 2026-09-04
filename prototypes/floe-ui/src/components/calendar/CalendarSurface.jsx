import { SquircleSurface } from '../../primitives.jsx';

export function CalendarSurface({ children, className = '' }) {
  return (
    <SquircleSurface className={`s1-surface ${className}`} contentClassName="s1-surface-content">
      {children}
    </SquircleSurface>
  );
}
