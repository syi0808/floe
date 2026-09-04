import { CheckControl } from '../ui/CheckControl.jsx';

export function SubtaskList({ subtasks, onToggle }) {
  return (
    <div className="subtask-list">
      {subtasks.map((subtask) => (
        <div className={subtask.done ? 'subtask-row done' : 'subtask-row'} key={subtask.id}>
          <CheckControl
            checked={subtask.done}
            label={`Complete ${subtask.title}`}
            onClick={() => onToggle(subtask.id)}
          />
          <span>{subtask.title}</span>
          <time>{subtask.duration}</time>
        </div>
      ))}
    </div>
  );
}
