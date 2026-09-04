import { useLayoutEffect, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import {
  ArrowUp,
  Check,
  ChevronLeft,
  ChevronRight,
  Clock3,
  NotebookPen,
  Plus,
  X,
} from 'lucide-react';
import {
  SQUIRCLE_RADIUS,
  SquircleBlock,
  SquircleButton,
  SquircleSurface,
} from '../../primitives.jsx';
import { events, initialTasks } from '../../data.js';
import { mascotUrl } from '../../assets.js';
import { CheckControl } from '../ui/CheckControl.jsx';

const hourHeight = 64;
const timelineHours = Array.from({ length: 12 }, (_, index) => index + 8);

export function TodayScreen({ onNavigate }) {
  const [view, setView] = useState('Day');
  const [suggestionOpen, setSuggestionOpen] = useState(false);
  const [reasonOpen, setReasonOpen] = useState(false);
  const [breakAdded, setBreakAdded] = useState(false);
  const [tasks, setTasks] = useState(initialTasks);
  const [capture, setCapture] = useState('');
  const [capturedText, setCapturedText] = useState('');

  const dayEvents = useMemo(() => {
    if (!breakAdded) return events;
    return [
      ...events,
      {
        id: 'reserved-break',
        title: 'Reserved break',
        time: '3:00 – 3:20 PM',
        start: 420,
        duration: 20,
        tone: 'mint',
      },
    ].sort((left, right) => left.start - right.start);
  }, [breakAdded]);

  function toggleTask(taskId) {
    setTasks((current) =>
      current.map((task) => (task.id === taskId ? { ...task, done: !task.done } : task)),
    );
  }

  function addBreak() {
    setBreakAdded(true);
    setSuggestionOpen(false);
    setReasonOpen(false);
  }

  function submitCapture(event) {
    event.preventDefault();
    const value = capture.trim();
    if (!value) return;
    setCapturedText(value);
    setCapture('');
  }

  return (
    <div className="today-screen">
      <section className="local-toolbar" aria-label="Calendar controls">
        <div className="toolbar-leading">
          <SquircleButton className="icon-button" aria-label="Previous day">
            <ChevronLeft size={20} />
          </SquircleButton>
          <SquircleButton className="icon-button" aria-label="Next day">
            <ChevronRight size={20} />
          </SquircleButton>
          <h1>Thu, Sep 3</h1>
          <button type="button" className="quiet-action">
            Today
          </button>
        </div>
        <SquircleSurface
          radius={SQUIRCLE_RADIUS.field}
          className="segmented-border"
          contentClassName="segmented"
        >
          {['Day', 'Week', 'Month'].map((option) => (
            <SquircleButton
              key={option}
              radius={SQUIRCLE_RADIUS.control}
              className={view === option ? 'segment active' : 'segment'}
              aria-pressed={view === option}
              onClick={() => setView(option)}
            >
              {option}
            </SquircleButton>
          ))}
        </SquircleSurface>
      </section>

      <div className="today-layout">
        <Timeline
          dayEvents={dayEvents}
          suggestionOpen={suggestionOpen}
          reasonOpen={reasonOpen}
          breakAdded={breakAdded}
          onToggleSuggestion={() => setSuggestionOpen((open) => !open)}
          onToggleReason={() => setReasonOpen((open) => !open)}
          onAddBreak={addBreak}
          onKeepCurrent={() => setSuggestionOpen(false)}
        />

        <aside className="context-rail" aria-label="Today context">
          <TasksCard tasks={tasks} onToggleTask={toggleTask} onNavigate={onNavigate} />
          <NoteCard />
        </aside>
      </div>

      <form className="capture-form" onSubmit={submitCapture}>
        <SquircleSurface
          radius={SQUIRCLE_RADIUS.field}
          className="capture-border"
          contentClassName="capture-bar"
        >
          {capturedText ? (
            <div className="capture-feedback" role="status">
              <Check size={18} />
              <span>Captured “{capturedText}”</span>
              <button type="button" onClick={() => setCapturedText('')} aria-label="Dismiss">
                <X size={17} />
              </button>
            </div>
          ) : (
            <>
              <Plus size={22} className="capture-plus" />
              <label className="sr-only" htmlFor="capture-input">
                Capture an item
              </label>
              <input
                id="capture-input"
                value={capture}
                onChange={(event) => setCapture(event.target.value)}
                placeholder="Capture an event, task, or thought"
              />
              <SquircleButton
                className="capture-submit"
                aria-label="Capture"
                disabled={!capture.trim()}
                type="submit"
              >
                <ArrowUp size={19} />
              </SquircleButton>
            </>
          )}
        </SquircleSurface>
      </form>
    </div>
  );
}

function Timeline({
  dayEvents,
  suggestionOpen,
  reasonOpen,
  breakAdded,
  onToggleSuggestion,
  onToggleReason,
  onAddBreak,
  onKeepCurrent,
}) {
  const launchEvent = events.find((event) => event.id === 'launch');
  const floeButtonRef = useRef(null);
  const markerTop = ((launchEvent.start + launchEvent.duration / 2) / 60) * hourHeight - 26;
  const currentTimeTop = ((14 * 60 + 28 - 8 * 60) / 60) * hourHeight;

  return (
    <SquircleSurface
      radius={SQUIRCLE_RADIUS.card}
      className="timeline-border"
      contentClassName="timeline-card"
    >
      <div className="all-day-row">
        <span>All-day</span>
        <SquircleBlock radius={SQUIRCLE_RADIUS.control} className="all-day-event">
          <span className="tone-dot mint" />
          Product launch
        </SquircleBlock>
      </div>

      <div className="timeline-stage" style={{ '--timeline-height': `${hourHeight * 11}px` }}>
        {timelineHours.map((hour, index) => (
          <div className="hour-line" key={hour} style={{ top: `${index * hourHeight}px` }}>
            <time>{formatHour(hour)}</time>
            <span />
          </div>
        ))}

        {dayEvents.map((event) => (
          <TimelineEvent key={event.id} event={event} />
        ))}

        <div className="current-time" style={{ top: `${currentTimeTop}px` }}>
          <time>2:28 PM</time>
          <i />
          <span />
        </div>

        {!breakAdded && (
          <div className="floe-anchor" style={{ top: `${markerTop}px` }}>
            {suggestionOpen && (
              <SuggestionPortal anchorRef={floeButtonRef}>
                <SuggestionPopover
                  reasonOpen={reasonOpen}
                  onClose={onKeepCurrent}
                  onToggleReason={onToggleReason}
                  onAddBreak={onAddBreak}
                  onKeepCurrent={onKeepCurrent}
                />
              </SuggestionPortal>
            )}
            <SquircleButton
              ref={floeButtonRef}
              radius={SQUIRCLE_RADIUS.floating}
              className={suggestionOpen ? 'floe-button active' : 'floe-button'}
              aria-label={suggestionOpen ? 'Close Floe suggestion' : 'Open Floe suggestion'}
              aria-expanded={suggestionOpen}
              onClick={onToggleSuggestion}
            >
              <img src={mascotUrl} alt="" />
            </SquircleButton>
          </div>
        )}
      </div>
    </SquircleSurface>
  );
}

function SuggestionPortal({ anchorRef, children }) {
  const [position, setPosition] = useState(null);

  useLayoutEffect(() => {
    function updatePosition() {
      const anchor = anchorRef.current;
      if (!anchor) return;
      const rect = anchor.getBoundingClientRect();
      const width = Math.min(380, window.innerWidth - 40);
      const left = Math.min(rect.right + 14, window.innerWidth - width - 20);
      const top = Math.max(20, Math.min(rect.top - 130, window.innerHeight - 380));
      setPosition({ left, top, width });
    }

    updatePosition();
    window.addEventListener('resize', updatePosition);
    window.addEventListener('scroll', updatePosition, true);
    return () => {
      window.removeEventListener('resize', updatePosition);
      window.removeEventListener('scroll', updatePosition, true);
    };
  }, [anchorRef]);

  if (!position) return null;

  return createPortal(
    <div className="suggestion-portal" style={position}>
      {children}
    </div>,
    document.body,
  );
}

function TimelineEvent({ event }) {
  const top = (event.start / 60) * hourHeight;
  const height = Math.max((event.duration / 60) * hourHeight - 8, 38);

  return (
    <SquircleBlock
      radius={SQUIRCLE_RADIUS.control}
      className={`timeline-event tone-${event.tone}`}
      style={{ top: `${top + 4}px`, height: `${height}px` }}
    >
      <span className={`tone-dot ${event.tone}`} />
      <span className="event-copy">
        <strong>{event.title}</strong>
        <time>{event.time}</time>
      </span>
    </SquircleBlock>
  );
}

function SuggestionPopover({ reasonOpen, onClose, onToggleReason, onAddBreak, onKeepCurrent }) {
  return (
    <SquircleSurface
      radius={SQUIRCLE_RADIUS.overlay}
      className="suggestion-border"
      contentClassName="suggestion-popover"
    >
      <div className="suggestion-heading">
        <div className="floe-attribution">
          <img src={mascotUrl} alt="" />
          <span>Floe suggestion</span>
        </div>
        <SquircleButton className="close-button" aria-label="Close suggestion" onClick={onClose}>
          <X size={18} />
        </SquircleButton>
      </div>
      <h2>Reserve a 20-minute break?</h2>
      <p>Your afternoon is busy. Add a break after Launch plan, before Team retro?</p>
      <SquircleBlock radius={SQUIRCLE_RADIUS.control} className="proposal-time">
        <Clock3 size={17} />
        <span>3:00 – 3:20 PM</span>
      </SquircleBlock>
      <div className="suggestion-actions">
        <SquircleButton className="primary-button" onClick={onAddBreak}>
          Add break
        </SquircleButton>
        <SquircleButton className="secondary-button" onClick={onKeepCurrent}>
          Keep current
        </SquircleButton>
      </div>
      <button
        type="button"
        className="reason-link"
        onClick={onToggleReason}
        aria-expanded={reasonOpen}
      >
        Why this suggestion?
      </button>
      {reasonOpen && (
        <p className="reason-copy">
          Launch plan ends at 3:00 PM and Team retro starts at 3:30 PM, leaving a 30-minute
          transition window.
        </p>
      )}
    </SquircleSurface>
  );
}

function TasksCard({ tasks, onToggleTask, onNavigate }) {
  return (
    <SquircleSurface
      radius={SQUIRCLE_RADIUS.card}
      className="rail-card-border"
      contentClassName="rail-card tasks-card"
    >
      <h2>Today’s tasks</h2>
      <div className="task-list">
        {tasks.map((task) => (
          <div className={task.done ? 'task-row done' : 'task-row'} key={task.id}>
            <span className={`tone-dot ${task.tone}`} />
            <span className="task-copy">
              <strong>{task.title}</strong>
              <small>{task.done ? 'Completed' : task.meta}</small>
            </span>
            <CheckControl
              checked={Boolean(task.done)}
              label={`Complete ${task.title}`}
              onClick={() => onToggleTask(task.id)}
            />
          </div>
        ))}
      </div>
      <button type="button" className="card-footer-action" onClick={() => onNavigate('tasks')}>
        View task detail
        <ChevronRight size={18} />
      </button>
    </SquircleSurface>
  );
}

function NoteCard() {
  return (
    <SquircleSurface
      radius={SQUIRCLE_RADIUS.card}
      className="rail-card-border"
      contentClassName="rail-card note-card"
    >
      <div className="card-title-row">
        <h2>Note for today</h2>
        <NotebookPen size={20} aria-hidden="true" />
      </div>
      <p>
        Focus on finalizing the launch plan and getting feedback on the deck. Keep the afternoon
        light for deep work and planning.
      </p>
      <small>Updated this morning</small>
    </SquircleSurface>
  );
}

function formatHour(hour) {
  if (hour === 12) return '12 PM';
  if (hour > 12) return `${hour - 12} PM`;
  return `${hour} AM`;
}
