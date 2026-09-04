import { CheckCircle2, Circle } from 'lucide-react';

export function Checklist({ title, items, complete = false }) {
  return (
    <div className="checkpoint-list">
      <h3>{title}</h3>
      <ul>
        {items.map((item) => (
          <li key={item}>
            {complete ? (
              <CheckCircle2 size={16} aria-hidden="true" />
            ) : (
              <Circle size={16} aria-hidden="true" />
            )}
            <span>{item}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}
