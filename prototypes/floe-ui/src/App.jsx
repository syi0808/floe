import { useLayoutEffect, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import {
  ArrowLeft,
  ArrowUp,
  CalendarDays,
  Check,
  ChevronLeft,
  ChevronRight,
  Clock3,
  Filter,
  MoreHorizontal,
  NotebookPen,
  Plus,
  Search,
  Settings,
  X,
} from 'lucide-react';

import mascotUrl from '../../../assets/floe-mascot.svg?url';
import { events, initialNotes, initialSubtasks, initialTasks } from './data.js';
import {
  SQUIRCLE_RADIUS,
  SquircleBlock,
  SquircleButton,
  SquircleSurface,
} from './primitives.jsx';

const navigationItems = [
  { id: 'today', label: 'Today' },
  { id: 'tasks', label: 'Tasks' },
  { id: 'notes', label: 'Notes' },
];

const hourHeight = 64;
const timelineHours = Array.from({ length: 12 }, (_, index) => index + 8);

export function App() {
  const [screen, setScreen] = useState('today');

  return (
    <main className="prototype-stage">
      <SquircleSurface
        radius={SQUIRCLE_RADIUS.frame}
        className="app-frame"
        contentClassName="app-window"
      >
        <GlobalHeader screen={screen} onNavigate={setScreen} />
        <div className="screen-region" key={screen}>
          {screen === 'today' && <TodayScreen onNavigate={setScreen} />}
          {screen === 'tasks' && <TaskDetail onBack={() => setScreen('today')} />}
          {screen === 'notes' && <NotesCollection />}
        </div>
      </SquircleSurface>
    </main>
  );
}

function GlobalHeader({ screen, onNavigate }) {
  return (
    <header className="global-header">
      <button className="brand" type="button" onClick={() => onNavigate('today')}>
        <img src={mascotUrl} alt="" />
        <span>Floe</span>
      </button>

      <nav className="global-nav" aria-label="Primary navigation">
        {navigationItems.map((item) => (
          <button
            key={item.id}
            type="button"
            className={screen === item.id ? 'nav-link active' : 'nav-link'}
            aria-current={screen === item.id ? 'page' : undefined}
            onClick={() => onNavigate(item.id)}
          >
            {item.label}
          </button>
        ))}
      </nav>

      <div className="header-actions">
        <SquircleButton className="header-utility" aria-label="Settings">
          <Settings size={20} strokeWidth={1.8} />
        </SquircleButton>
      </div>
    </header>
  );
}

function TodayScreen({ onNavigate }) {
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
      current.map((task) =>
        task.id === taskId ? { ...task, done: !task.done } : task,
      ),
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
          <button type="button" className="quiet-action">Today</button>
        </div>
        <SquircleSurface radius={SQUIRCLE_RADIUS.field} className="segmented-border" contentClassName="segmented">
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
        <SquircleSurface radius={SQUIRCLE_RADIUS.field} className="capture-border" contentClassName="capture-bar">
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
              <label className="sr-only" htmlFor="capture-input">Capture an item</label>
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
    <SquircleSurface radius={SQUIRCLE_RADIUS.card} className="timeline-border" contentClassName="timeline-card">
      <div className="all-day-row">
        <span>All-day</span>
        <SquircleBlock radius={SQUIRCLE_RADIUS.control} className="all-day-event">
          <span className="tone-dot mint" />
          Product launch
        </SquircleBlock>
      </div>

      <div className="timeline-stage" style={{ '--timeline-height': `${hourHeight * 11}px` }}>
        {timelineHours.map((hour, index) => (
          <div
            className="hour-line"
            key={hour}
            style={{ top: `${index * hourHeight}px` }}
          >
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

function SuggestionPopover({
  reasonOpen,
  onClose,
  onToggleReason,
  onAddBreak,
  onKeepCurrent,
}) {
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
        <SquircleButton className="primary-button" onClick={onAddBreak}>Add break</SquircleButton>
        <SquircleButton className="secondary-button" onClick={onKeepCurrent}>Keep current</SquircleButton>
      </div>
      <button type="button" className="reason-link" onClick={onToggleReason} aria-expanded={reasonOpen}>
        Why this suggestion?
      </button>
      {reasonOpen && (
        <p className="reason-copy">Launch plan ends at 3:00 PM and Team retro starts at 3:30 PM, leaving a 30-minute transition window.</p>
      )}
    </SquircleSurface>
  );
}

function TasksCard({ tasks, onToggleTask, onNavigate }) {
  return (
    <SquircleSurface radius={SQUIRCLE_RADIUS.card} className="rail-card-border" contentClassName="rail-card tasks-card">
      <h2>Today’s tasks</h2>
      <div className="task-list">
        {tasks.map((task) => (
          <div className={task.done ? 'task-row done' : 'task-row'} key={task.id}>
            <span className={`tone-dot ${task.tone}`} />
            <span className="task-copy">
              <strong>{task.title}</strong>
              <small>{task.done ? 'Completed' : task.meta}</small>
            </span>
            <CheckControl checked={Boolean(task.done)} label={`Complete ${task.title}`} onClick={() => onToggleTask(task.id)} />
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
    <SquircleSurface radius={SQUIRCLE_RADIUS.card} className="rail-card-border" contentClassName="rail-card note-card">
      <div className="card-title-row">
        <h2>Note for today</h2>
        <NotebookPen size={20} aria-hidden="true" />
      </div>
      <p>Focus on finalizing the launch plan and getting feedback on the deck. Keep the afternoon light for deep work and planning.</p>
      <small>Updated this morning</small>
    </SquircleSurface>
  );
}

function TaskDetail({ onBack }) {
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
        <button type="button" className="back-action" onClick={onBack}>
          <ArrowLeft size={19} />
          Back to today
        </button>
        <SquircleButton className="icon-button" aria-label="Task options">
          <MoreHorizontal size={21} />
        </SquircleButton>
      </section>

      <div className="detail-layout">
        <SquircleSurface radius={SQUIRCLE_RADIUS.card} className="detail-panel-border" contentClassName="task-panel">
          <div className="object-kicker"><span className="tone-dot blue" /> Task</div>
          <h1>Prepare launch brief</h1>
          <p className="task-description">Create a short launch brief for the product launch. Include key messaging, timeline, and audience. Share with the team for review.</p>

          <dl className="task-metadata">
            <div><dt>Due</dt><dd className="violet-text">Today</dd></div>
            <div><dt>Time context</dt><dd>Before 3:30 PM · Team retro</dd></div>
            <div><dt>Calendar</dt><dd className="mint-text">Product launch</dd></div>
          </dl>

          <div className="section-rule" />
          <h2>Subtasks</h2>
          <div className="subtask-list">
            {subtasks.map((subtask) => (
              <div className={subtask.done ? 'subtask-row done' : 'subtask-row'} key={subtask.id}>
                <CheckControl checked={subtask.done} label={`Complete ${subtask.title}`} onClick={() => toggleSubtask(subtask.id)} />
                <span>{subtask.title}</span>
                <time>{subtask.duration}</time>
              </div>
            ))}
          </div>
          <button type="button" className="add-subtask"><Plus size={18} /> Add a subtask</button>
        </SquircleSurface>

        <aside className="context-rail detail-rail">
          {suggestionVisible && (
            <SquircleSurface radius={SQUIRCLE_RADIUS.card} className="rail-card-border" contentClassName="rail-card detail-suggestion">
              <div className="suggestion-heading">
                <div className="floe-attribution">
                  <img src={mascotUrl} alt="" />
                  <span>Floe suggests</span>
                </div>
                <SquircleButton className="close-button" aria-label="Dismiss suggestion" onClick={() => setSuggestionVisible(false)}>
                  <X size={18} />
                </SquircleButton>
              </div>
              <p>Team retro starts at 3:30 PM. Review the launch brief first?</p>
              <div className="inline-actions">
                <button type="button" className="violet-action">Review now</button>
                <button type="button" className="quiet-action">Snooze</button>
              </div>
            </SquircleSurface>
          )}
          <SquircleSurface radius={SQUIRCLE_RADIUS.card} className="rail-card-border tinted-violet" contentClassName="rail-card related-note">
            <h2>Notes</h2>
            <p>Focus on clarity and alignment with our Q3 goals. Keep it short and actionable.</p>
            <small>Updated this morning</small>
          </SquircleSurface>
        </aside>
      </div>
    </div>
  );
}

function NotesCollection() {
  const [query, setQuery] = useState('');
  const [personalOnly, setPersonalOnly] = useState(false);
  const [notes, setNotes] = useState(initialNotes);

  const visibleNotes = useMemo(() => {
    const normalizedQuery = query.trim().toLowerCase();
    return notes.filter((note) => {
      const matchesFilter = !personalOnly || note.category === 'Personal';
      const matchesQuery = !normalizedQuery || [note.title, note.excerpt, note.category]
        .some((value) => value.toLowerCase().includes(normalizedQuery));
      return matchesFilter && matchesQuery;
    });
  }, [notes, personalOnly, query]);

  function addNote() {
    if (notes.some((note) => note.id === 'untitled')) return;
    setNotes((current) => [
      {
        id: 'untitled',
        category: 'Draft',
        title: 'Untitled note',
        excerpt: 'Start writing a thought, decision, or detail you want to remember.',
        timestamp: 'Just now',
        tone: 'violet',
      },
      ...current,
    ]);
  }

  return (
    <div className="notes-screen">
      <section className="local-toolbar notes-toolbar">
        <h1>All notes <span>· {notes.length}</span></h1>
        <div className="notes-tools">
          <SquircleSurface radius={SQUIRCLE_RADIUS.field} className="search-border" contentClassName="search-field">
            <Search size={19} />
            <label className="sr-only" htmlFor="notes-search">Search notes</label>
            <input
              id="notes-search"
              type="search"
              placeholder="Search notes"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
            />
          </SquircleSurface>
          <SquircleButton
            className={personalOnly ? 'secondary-button filter-button active' : 'secondary-button filter-button'}
            aria-pressed={personalOnly}
            onClick={() => setPersonalOnly((active) => !active)}
          >
            <Filter size={18} /> Filter
          </SquircleButton>
          <SquircleButton className="primary-button new-note-button" onClick={addNote}>
            <Plus size={18} /> New note
          </SquircleButton>
        </div>
      </section>

      {visibleNotes.length ? (
        <div className="notes-grid">
          {visibleNotes.map((note) => (
            <SquircleSurface
              key={note.id}
              radius={SQUIRCLE_RADIUS.card}
              className={`note-preview-border tone-${note.tone}`}
              contentClassName="note-preview"
            >
              <div className="note-category"><span className={`tone-dot ${note.tone}`} /> {note.category}</div>
              <h2>{note.title}</h2>
              <p>{note.excerpt}</p>
              <time>{note.timestamp}</time>
            </SquircleSurface>
          ))}
        </div>
      ) : (
        <div className="notes-empty">
          <Search size={24} />
          <h2>No notes found</h2>
          <p>Try a different search or clear the current filter.</p>
          <button type="button" className="violet-action" onClick={() => { setQuery(''); setPersonalOnly(false); }}>Clear filters</button>
        </div>
      )}
    </div>
  );
}

function CheckControl({ checked, label, onClick }) {
  return (
    <SquircleButton
      radius={SQUIRCLE_RADIUS.compact}
      className={checked ? 'check-control checked' : 'check-control'}
      aria-label={label}
      aria-pressed={checked}
      onClick={onClick}
    >
      <SquircleBlock radius={SQUIRCLE_RADIUS.micro} className={checked ? 'check-visual checked' : 'check-visual'}>
        {checked && <Check size={14} strokeWidth={2.4} />}
      </SquircleBlock>
    </SquircleButton>
  );
}

function formatHour(hour) {
  if (hour === 12) return '12 PM';
  if (hour > 12) return `${hour - 12} PM`;
  return `${hour} AM`;
}
