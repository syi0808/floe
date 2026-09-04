import { TaskSuggestionCard } from './TaskSuggestionCard.jsx';
import { useState } from 'react';
import { MoreHorizontal, Plus } from 'lucide-react';
import { SQUIRCLE_RADIUS, SquircleButton, SquircleSurface } from '../../primitives.jsx';
import { initialSubtasks } from '../../data.js';
import { SubtaskList } from './SubtaskList.jsx';

export function TaskDetail() {
  const [subtasks, setSubtasks] = useState(initialSubtasks);
  const [suggestionVisible, setSuggestionVisible] = useState(true);

  function toggleSubtask(subtaskId) {
    setSubtasks((current) =>
      current.map((subtask) =>
        subtask.id === subtaskId ? { ...subtask, done: !subtask.done } : subtask,
      ),
    );
  }

  return (
    <div className="task-detail-screen">
      <section className="local-toolbar task-toolbar">
        <SquircleButton className="icon-button" aria-label="Task options">
          <MoreHorizontal size={21} />
        </SquircleButton>
      </section>

      <div className="detail-layout">
        <SquircleSurface
          radius={SQUIRCLE_RADIUS.card}
          className="detail-panel-border"
          contentClassName="task-panel"
        >
          <div className="object-kicker">
            <span className="tone-dot blue" /> Task
          </div>
          <h1>Prepare launch brief</h1>
          <p className="task-description">
            Create a short launch brief for the product launch. Include key messaging, timeline, and
            audience. Share with the team for review.
          </p>

          <dl className="task-metadata">
            <div>
              <dt>Due</dt>
              <dd className="violet-text">Today</dd>
            </div>
            <div>
              <dt>Time context</dt>
              <dd>Before 3:30 PM · Team retro</dd>
            </div>
            <div>
              <dt>Calendar</dt>
              <dd className="mint-text">Product launch</dd>
            </div>
          </dl>

          <div className="section-rule" />
          <h2>Subtasks</h2>
          <SubtaskList subtasks={subtasks} onToggle={toggleSubtask} />
          <button type="button" className="add-subtask">
            <Plus size={18} /> Add a subtask
          </button>
        </SquircleSurface>

        <aside className="context-rail detail-rail">
          {suggestionVisible && (
            <TaskSuggestionCard onDismiss={() => setSuggestionVisible(false)} />
          )}
          <SquircleSurface
            radius={SQUIRCLE_RADIUS.card}
            className="rail-card-border tinted-violet"
            contentClassName="rail-card related-note"
          >
            <h2>Notes</h2>
            <p>Focus on clarity and alignment with our Q3 goals. Keep it short and actionable.</p>
            <small>Updated this morning</small>
          </SquircleSurface>
        </aside>
      </div>
    </div>
  );
}
