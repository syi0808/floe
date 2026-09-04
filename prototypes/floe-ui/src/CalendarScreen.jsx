import { useEffect, useRef, useState } from 'react';
import {
  ArrowLeft,
  ArrowRight,
  CalendarDays,
  Check,
  ChevronLeft,
  ChevronRight,
  Clock3,
  Database,
  ExternalLink,
  Info,
  Link2,
  LoaderCircle,
  LockKeyhole,
  RefreshCw,
  Search,
  ShieldCheck,
  Unplug,
  WifiOff,
  X,
} from 'lucide-react';
import { SquircleButton, SquircleSurface } from './primitives.jsx';
import mascotUrl from '../../../assets/floe-mascot.svg?url';
import './calendar.css';

const calendars = [
  { id: 'work', name: 'Work', account: 'iCloud', color: 'blue' },
  { id: 'personal', name: 'Personal', account: 'iCloud', color: 'mint' },
  { id: 'team', name: 'Product team', account: 'Google · via macOS Calendar', color: 'violet' },
];
const scenarios = {
  connected: 'Connected · up to date',
  disconnected: 'First visit · not connected',
  syncing: 'Refreshing · keep cached events',
  cached: 'App reopened · cached data',
  offline: 'Read failed · cached data',
  denied: 'Permission denied · no data',
  revoked: 'Permission revoked · cached data',
  missing: 'Calendar unavailable',
  noCalendars: 'No calendars on this Mac',
  empty: 'Successful read · no events',
  uncollected: 'Date not collected',
  loadError: 'Local data failed to load',
};
const externalEvents = [
  {
    id: 'standup',
    title: 'A little alignment',
    time: '9:30 – 10:00 AM',
    top: 90,
    detail: 'Daily stand-up',
    timezone: 'Asia/Seoul',
    original: 'Sep 4, 9:30 – 10:00 AM KST',
    recurring: true,
  },
  {
    id: 'design',
    title: 'Make room for the details',
    time: '11:00 AM – 12:00 PM',
    top: 180,
    detail: 'Design review',
    timezone: 'Asia/Seoul',
    original: 'Sep 4, 11:00 AM – 12:00 PM KST',
  },
  {
    id: 'remote',
    title: 'Across time zones',
    time: '4:00 – 4:45 PM',
    top: 480,
    detail: 'San Francisco team catch-up',
    timezone: 'America/Los_Angeles',
    original: 'Sep 4, 12:00 – 12:45 AM PDT',
  },
];

function Surface({ children, className = '' }) {
  return (
    <SquircleSurface className={`s1-surface ${className}`} contentClassName="s1-surface-content">
      {children}
    </SquircleSurface>
  );
}

export function CalendarScreen({ page, onNavigate }) {
  const [phase, setPhase] = useState('connected');
  const [calendar, setCalendar] = useState(calendars[0]);
  const [dayOffset, setDayOffset] = useState(0);
  const [modal, setModal] = useState(null);
  const [choice, setChoice] = useState('work');
  const [search, setSearch] = useState('');
  const [toast, setToast] = useState('');
  const [taskDone, setTaskDone] = useState(false);
  const [capture, setCapture] = useState('');
  const [localNotes, setLocalNotes] = useState([]);
  const [readDates, setReadDates] = useState([0]);
  const [cachedCalendarId, setCachedCalendarId] = useState('work');
  const [changePending, setChangePending] = useState(false);
  const [externalRevision, setExternalRevision] = useState(0);
  const lab = useRef(null);
  const timer = useRef(null);
  const toastTimer = useRef(null);
  const date = new Date(Date.UTC(2026, 8, 4 + dayOffset));
  const dateLabel = date.toLocaleDateString('en-US', {
    weekday: 'short',
    month: 'short',
    day: 'numeric',
    timeZone: 'UTC',
  });
  const dateShort = date.toLocaleDateString('en-US', {
    month: 'short',
    day: 'numeric',
    timeZone: 'UTC',
  });
  const hasCache =
    ['connected', 'syncing', 'cached', 'offline', 'revoked', 'missing'].includes(phase) &&
    dayOffset === 0 &&
    readDates.includes(0) &&
    cachedCalendarId === calendar.id;
  const hasConnection = !['disconnected', 'denied', 'noCalendars'].includes(phase);
  const stale = ['cached', 'offline', 'revoked', 'missing'].includes(phase);
  const statusTone = ['offline', 'revoked', 'missing', 'loadError'].includes(phase)
    ? 'warning'
    : ['connected', 'empty'].includes(phase)
      ? ''
      : 'neutral';
  const visibleEvents = externalEvents
    .filter((event) => !externalRevision || event.id !== 'standup')
    .map((event) =>
      externalRevision && event.id === 'design'
        ? {
            ...event,
            title: 'Design review · updated',
            time: '11:30 AM – 12:00 PM',
            top: 210,
            original: 'Sep 4, 11:30 AM – 12:00 PM KST',
          }
        : event,
    );

  useEffect(
    () => () => {
      clearTimeout(timer.current);
      clearTimeout(toastTimer.current);
    },
    [],
  );

  function notify(message) {
    clearTimeout(toastTimer.current);
    setToast(message);
    toastTimer.current = setTimeout(() => setToast(''), 4500);
  }

  function preview(value) {
    clearTimeout(timer.current);
    setPhase(value);
    setModal(null);
    setToast('');
    setDayOffset(value === 'uncollected' ? 1 : 0);
    setReadDates(
      ['connected', 'syncing', 'cached', 'offline', 'revoked', 'missing', 'uncollected'].includes(
        value,
      )
        ? [0]
        : [],
    );
    setCachedCalendarId(calendar.id);
    setChangePending(false);
    setExternalRevision(0);
  }

  function refresh(nextCalendar = calendar) {
    clearTimeout(timer.current);
    setCalendar(nextCalendar);
    const changing = nextCalendar.id !== calendar.id;
    if (changing) setReadDates([]);
    if (changing) {
      setExternalRevision(0);
      setChangePending(false);
    }
    setCachedCalendarId(nextCalendar.id);
    setModal(null);
    setPhase('syncing');
    timer.current = setTimeout(() => {
      setPhase(dayOffset === 0 ? 'connected' : 'empty');
      setReadDates((current) => [...new Set([...(changing ? [] : current), dayOffset])]);
      if (changePending && !changing && dayOffset === 0) {
        setExternalRevision(1);
        setChangePending(false);
        notify(
          'Refreshed · 1 event updated, 1 removed. Other dates and local items are unchanged.',
        );
      } else notify('Calendar refreshed. Your local tasks and notes are unchanged.');
    }, 1100);
  }

  function moveDay(offset) {
    clearTimeout(timer.current);
    setDayOffset(offset);
    if (hasConnection && !['revoked', 'missing', 'offline', 'loadError'].includes(phase))
      setPhase(readDates.includes(offset) ? (offset === 0 ? 'cached' : 'empty') : 'uncollected');
  }

  function picker() {
    setChoice(calendar.id);
    setSearch('');
    setModal('picker');
  }

  function selectCalendar() {
    if (hasConnection && choice !== calendar.id) setModal('switch');
    else refresh(calendars.find((item) => item.id === choice));
  }

  const statusLabel =
    {
      connected: 'Up to date',
      syncing: 'Refreshing…',
      cached: 'Saved on this Mac',
      offline: 'Couldn’t refresh',
      revoked: 'Access needs attention',
      missing: 'Calendar unavailable',
      empty: 'Up to date · no events',
      uncollected: 'Not collected yet',
      loadError: 'Local data unavailable',
    }[phase] || 'Not connected';

  return (
    <div className="s1-screen">
      <header className="s1-heading">
        <div>
          <div className="s1-eyebrow">YOUR DAY, WITH CONTEXT</div>
          <h1>{page === 'connections' ? 'A little more connected.' : 'Space for what matters.'}</h1>
          <p>
            {page === 'connections'
              ? 'Your calendars. On your terms. Always read-only.'
              : 'Your calendar and your own plans, quietly in one place.'}
          </p>
        </div>
        <button
          className="s1-demo-label s1-lab-trigger"
          aria-controls="s1-lab"
          onClick={() => {
            lab.current.open = true;
            lab.current.scrollIntoView({ block: 'center' });
          }}
        >
          <span /> S1 · Preview states
        </button>
      </header>

      {page === 'connections' ? (
        <>
          <button className="back-action s1-back" onClick={() => onNavigate('today')}>
            <ArrowLeft size={16} /> Back to your day
          </button>
          <div className="s1-connections-layout">
            <Surface>
              <div className="s1-card-heading">
                <span className="s1-provider-icon">
                  <CalendarDays size={25} />
                </span>
                <div>
                  <h2>macOS Calendar</h2>
                  <p>Calendars already on this Mac</p>
                </div>
                <span className="s1-pill">
                  <LockKeyhole size={12} /> Read-only
                </span>
              </div>
              <p className="s1-body-copy">
                Bring one calendar into your day. Floe reads its events; it never creates, edits, or
                deletes anything in Calendar.
              </p>
              <div className="s1-connection-record">
                <div>
                  <span className="s1-meta-label">SELECTED CALENDAR</span>
                  <strong>{hasConnection ? calendar.name : 'Nothing connected yet'}</strong>
                  <small>
                    {hasConnection ? calendar.account : 'Choose a calendar after granting access'}
                  </small>
                </div>
                <span className={`s1-status ${statusTone}`}>
                  <span />
                  {statusLabel}
                </span>
              </div>
              <dl className="s1-facts">
                <div>
                  <dt>Person</dt>
                  <dd>You · this device</dd>
                </div>
                <div>
                  <dt>Stored range</dt>
                  <dd>
                    {readDates.length
                      ? readDates
                          .map((offset) =>
                            new Date(Date.UTC(2026, 8, 4 + offset)).toLocaleDateString('en-US', {
                              month: 'short',
                              day: 'numeric',
                              timeZone: 'UTC',
                            }),
                          )
                          .join(', ') + ' · Asia/Seoul'
                      : 'Nothing collected yet'}
                  </dd>
                </div>
                <div>
                  <dt>Last successful read</dt>
                  <dd>{readDates.length ? 'Today at 2:28 PM' : '—'}</dd>
                </div>
                <div>
                  <dt>Refresh behavior</dt>
                  <dd>Only when you ask</dd>
                </div>
              </dl>
              <div className="s1-actions">
                <SquircleButton
                  className="primary-button"
                  disabled={phase === 'syncing'}
                  onClick={() => (hasConnection ? picker() : setModal('disclosure'))}
                >
                  {hasConnection ? 'Change calendar' : 'Connect Calendar'}
                  <ArrowRight size={16} />
                </SquircleButton>
                <SquircleButton className="secondary-button" onClick={() => setModal('settings')}>
                  Manage access
                </SquircleButton>
              </div>
              {hasConnection && (
                <button className="s1-danger-link" onClick={() => setModal('disconnect')}>
                  <Unplug size={14} /> Disconnect from Floe
                </button>
              )}
            </Surface>
            <div className="s1-side-stack">
              <Surface>
                <ShieldCheck size={23} className="s1-violet" />
                <h2>A clear boundary.</h2>
                <p className="s1-body-copy">
                  Only the calendar you choose is copied into Floe. Titles, times, time zones, and
                  source identifiers stay on this Mac.
                </p>
                <div className="s1-note">
                  <Info size={16} />
                  <p>
                    macOS calls this “Full Access,” even for reading. That OS permission does not
                    enable writes in Floe.
                  </p>
                </div>
              </Surface>
              <Surface>
                <h2>What happens offline?</h2>
                <p className="s1-body-copy">
                  Your last saved events remain visible, with their collection time. Revoking
                  permission stops new reads; it does not erase the saved copy.
                </p>
                <small>
                  Local storage uses the current unencrypted database. No calendar data is sent to
                  an AI model in S1.
                </small>
              </Surface>
            </div>
          </div>
        </>
      ) : (
        <>
          <div className="s1-day-toolbar">
            <div className="s1-date-controls">
              <SquircleButton
                className="icon-button"
                aria-label="Previous day"
                disabled={phase === 'syncing'}
                onClick={() => moveDay(dayOffset - 1)}
              >
                <ChevronLeft size={18} />
              </SquircleButton>
              <SquircleButton
                className="icon-button"
                aria-label="Next day"
                disabled={phase === 'syncing'}
                onClick={() => moveDay(dayOffset + 1)}
              >
                <ChevronRight size={18} />
              </SquircleButton>
              <h2>{dateLabel}</h2>
              <button
                className="quiet-action"
                onClick={() => moveDay(0)}
                disabled={phase === 'syncing'}
              >
                Today
              </button>
            </div>
            <span className="s1-timezone">
              <Clock3 size={14} /> Asia/Seoul · UTC+09
            </span>
          </div>
          <div className="s1-source-bar">
            <button onClick={() => onNavigate('connections')} className="s1-source-button">
              <CalendarDays size={16} />
              <strong>{hasConnection ? calendar.name : 'Calendar'}</strong>
              <span>{hasConnection ? calendar.account : 'Not connected'}</span>
              <ChevronRight size={14} />
            </button>
            <div className="s1-sync-actions">
              <span role="status" className={`s1-status ${statusTone}`}>
                {phase === 'syncing' ? <LoaderCircle size={13} className="s1-spin" /> : <span />}
                {statusLabel}
              </span>
              {hasConnection && (
                <SquircleButton
                  className="s1-refresh"
                  aria-label="Refresh selected date"
                  disabled={phase === 'syncing' || phase === 'loadError'}
                  onClick={() =>
                    ['revoked', 'missing'].includes(phase)
                      ? setModal(phase === 'revoked' ? 'settings' : 'picker')
                      : refresh()
                  }
                >
                  <RefreshCw size={15} />
                </SquircleButton>
              )}
            </div>
          </div>
          {phase !== 'connected' && phase !== 'empty' && (
            <StatusBanner
              phase={phase}
              dateLabel={dateShort}
              onConnect={() => setModal('disclosure')}
              onRefresh={() => refresh()}
              onSettings={() => setModal('settings')}
              onPicker={picker}
            />
          )}
          <div className="s1-day-layout">
            {phase === 'loadError' ? (
              <Surface className="s1-error-card">
                <Database size={30} />
                <h2>Your day couldn’t be loaded.</h2>
                <p>Your saved data hasn’t been removed. Let’s try reading it again.</p>
                <details>
                  <summary>Technical details</summary>
                  <code>
                    storage · unsupported source format
                    <br />
                    Reference: local-read-004
                  </code>
                </details>
                <SquircleButton className="primary-button" onClick={() => refresh()}>
                  Try again
                </SquircleButton>
              </Surface>
            ) : (
              <Surface className="s1-agenda">
                <div className="s1-all-day">
                  <span>ALL DAY</span>
                  {hasCache ? (
                    <button
                      onClick={() =>
                        setModal({
                          id: 'all-day',
                          title: 'A day for making',
                          time: 'All day',
                          timezone: 'Asia/Seoul',
                          original: `${dateShort}, all day`,
                          allDay: true,
                        })
                      }
                    >
                      <span className={`tone-dot ${calendar.color}`} /> A day for making{' '}
                      <small>{calendar.name}</small>
                    </button>
                  ) : (
                    <span className="s1-muted">—</span>
                  )}
                  {hasCache && (
                    <button
                      className="s1-more-all-day"
                      onClick={() =>
                        setModal({
                          id: 'multi-day',
                          title: 'Research week',
                          time: 'Sep 3–5 · All day',
                          timezone: 'Date-only · no conversion',
                          original: 'Sep 3–5, all day',
                          detail: 'A multi-day event. Dates stay dates, even across time zones.',
                          allDay: true,
                          endDateExclusive: 'Sep 6, 2026 · exclusive',
                        })
                      }
                    >
                      <span className={`tone-dot ${calendar.color}`} /> Research week{' '}
                      <small>Day 2 of 3</small>
                    </button>
                  )}
                </div>
                <div
                  className={`s1-time-grid ${stale ? 's1-cached-grid' : ''}`}
                  aria-label="Day timeline"
                >
                  {Array.from({ length: 10 }, (_, index) => (
                    <div className="s1-hour" key={index} style={{ top: index * 60 }}>
                      <time>
                        {index + 8 > 12 ? index - 4 : index + 8}
                        {index + 8 >= 12 ? ' PM' : ' AM'}
                      </time>
                      <span />
                    </div>
                  ))}
                  {hasCache &&
                    visibleEvents.map((event) => (
                      <SquircleButton
                        key={event.id}
                        className={`s1-event s1-event-${calendar.color}`}
                        style={{ top: event.top }}
                        onClick={() => setModal(event)}
                      >
                        <span className={`tone-dot ${calendar.color}`} />
                        <span>
                          <strong>{event.title}</strong>
                          <time>{event.time}</time>
                          <small>
                            {calendar.name} · {event.detail}
                            {event.recurring ? ' · Repeats' : ''}
                          </small>
                        </span>
                        <LockKeyhole size={13} />
                      </SquircleButton>
                    ))}
                  {!hasCache && (
                    <div className="s1-empty-day">
                      <img src={mascotUrl} alt="" />
                      <h3>
                        {phase === 'syncing'
                          ? 'Finding your day…'
                          : phase === 'empty'
                            ? 'A little breathing room.'
                            : phase === 'uncollected'
                              ? 'An unread page in your day.'
                              : 'Your day starts here.'}
                      </h3>
                      <p>
                        {phase === 'syncing'
                          ? 'Reading the selected calendar. Your own tasks and notes stay available.'
                          : phase === 'empty'
                            ? `No events in ${calendar.name} for ${dateShort}. This date was checked successfully.`
                            : phase === 'uncollected'
                              ? 'No events have been collected for this date. It doesn’t mean your calendar is empty.'
                              : 'Your own tasks and notes are ready. Connect a calendar when you want more context.'}
                      </p>
                      <SquircleButton
                        className="secondary-button"
                        disabled={phase === 'syncing'}
                        onClick={() =>
                          phase === 'empty'
                            ? onNavigate('connections')
                            : phase === 'uncollected'
                              ? refresh()
                              : setModal('disclosure')
                        }
                      >
                        {phase === 'syncing'
                          ? 'Reading…'
                          : phase === 'empty'
                            ? 'Check selected calendar'
                            : phase === 'uncollected'
                              ? 'Read this date'
                              : 'Connect Calendar'}
                      </SquircleButton>
                    </div>
                  )}
                  {dayOffset === 0 && (
                    <div className="s1-now" style={{ top: 388 }}>
                      <time>2:28 PM</time>
                      <span />
                    </div>
                  )}
                </div>
                <div className="s1-agenda-footer">
                  <LockKeyhole size={13} />
                  <span>External events are read-only.</span>
                  {hasCache && <span>Saved at 2:28 PM{stale ? ' · may be out of date' : ''}</span>}
                </div>
              </Surface>
            )}
            <aside className="s1-side-stack">
              <Surface>
                <div className="s1-card-heading">
                  <h2>Your own rhythm</h2>
                  <span className="s1-pill">Local</span>
                </div>
                <p className="s1-body-copy">A few things that belong to you, not your calendar.</p>
                <label className={`s1-local-task ${taskDone ? 'done' : ''}`}>
                  <input
                    type="checkbox"
                    checked={taskDone}
                    onChange={(event) => setTaskDone(event.target.checked)}
                  />
                  <span>
                    Finish the launch brief<small>One good thing to move forward</small>
                  </span>
                </label>
                <button className="s1-text-link" onClick={() => onNavigate('tasks')}>
                  See your tasks <ArrowRight size={15} />
                </button>
              </Surface>
              <Surface>
                <div className="s1-card-heading">
                  <h2>A note to self</h2>
                  <span className="tone-dot mint" />
                </div>
                <p className="s1-personal-note">
                  Leave a little room between things. Not every empty space needs filling.
                </p>
                {localNotes.map((note, index) => (
                  <p className="s1-captured-note" key={index}>
                    {note}
                  </p>
                ))}
                <small>Saved in Floe · stays when you disconnect</small>
              </Surface>
              {hasCache && (
                <div className="s1-provenance-hint">
                  <Link2 size={15} />
                  <p>
                    Wondering where an event came from?
                    <br />
                    Open it to see its source and time zone.
                  </p>
                </div>
              )}
            </aside>
          </div>
          <form
            className="s1-capture"
            onSubmit={(event) => {
              event.preventDefault();
              if (capture.trim()) {
                setLocalNotes((items) => [...items, capture.trim()]);
                setCapture('');
                notify('Note saved in Floe. No calendar changes.');
              }
            }}
          >
            <label htmlFor="s1-capture">+</label>
            <input
              id="s1-capture"
              value={capture}
              onChange={(event) => setCapture(event.target.value)}
              placeholder="A thought for your day…"
              aria-label="Capture a local note"
            />
            <span>Only in Floe</span>
            <SquircleButton disabled={!capture.trim()} type="submit" aria-label="Save local note">
              <ArrowRight size={18} />
            </SquircleButton>
          </form>
        </>
      )}

      <details className="s1-lab" id="s1-lab" ref={lab}>
        <summary>
          <span className="s1-demo-dot" /> Prototype lab{' '}
          <span>Explore the edges, not just the happy path.</span>
        </summary>
        <div className="s1-lab-content">
          <label htmlFor="s1-scenario">Preview a state</label>
          <select id="s1-scenario" value={phase} onChange={(event) => preview(event.target.value)}>
            {Object.entries(scenarios).map(([value, label]) => (
              <option key={value} value={value}>
                {label}
              </option>
            ))}
          </select>
          <div className="s1-lab-actions">
            <SquircleButton
              className="secondary-button"
              disabled={
                !hasCache || phase === 'syncing' || changePending || Boolean(externalRevision)
              }
              onClick={() => {
                setChangePending(true);
                setPhase('cached');
                notify('Sample provider changed. Refresh to import the update and deletion.');
              }}
            >
              Simulate an external edit & deletion
            </SquircleButton>
            {changePending && <span>Pending in sample provider · refresh to import</span>}
          </div>
          <p>
            Sample data only. Permissions, refreshes, and device settings are simulated. State
            resets on browser reload. No external calendar is accessed or changed.
          </p>
          <button className="s1-text-link" onClick={() => onNavigate('reference')}>
            Open the earlier Personal Day reference <ArrowRight size={14} />
          </button>
        </div>
      </details>
      {toast && (
        <div className="s1-toast" role="status">
          <Check size={17} />
          {toast}
          <button aria-label="Dismiss message" onClick={() => setToast('')}>
            <X size={16} />
          </button>
        </div>
      )}

      {modal && (
        <Modal
          title={
            typeof modal === 'object'
              ? modal.title
              : {
                  disclosure: 'Your calendar, with a clear boundary.',
                  permission: 'A macOS permission, explained.',
                  picker: 'Which calendar belongs in your day?',
                  switch: 'Switch your calendar?',
                  settings: 'Let Floe read your calendar again.',
                  disconnect: 'Disconnect this calendar?',
                }[modal]
          }
          onClose={() => setModal(null)}
        >
          {modal === 'disclosure' && (
            <>
              <div className="s1-modal-icon">
                <CalendarDays size={30} />
              </div>
              <p className="s1-body-copy">
                Connect one calendar already on this Mac. Floe will keep a local copy of its events
                so your day is still there when you’re offline.
              </p>
              <ul className="s1-permission-list">
                <li>
                  <Check size={17} /> Read titles, times, and time zones
                </li>
                <li>
                  <Check size={17} /> Keep source details with each event
                </li>
                <li>
                  <LockKeyhole size={17} /> Never create, edit, or delete external events
                </li>
              </ul>
              <div className="s1-note">
                <Info size={18} />
                <p>
                  macOS requires “Full Access” to read events. You’ll see a read-and-write
                  permission, but Floe only uses it to read. Only your selected calendar is stored
                  locally.
                </p>
              </div>
              <div className="s1-modal-actions">
                <SquircleButton className="secondary-button" onClick={() => setModal(null)}>
                  Not now
                </SquircleButton>
                <SquircleButton className="primary-button" onClick={() => setModal('permission')}>
                  Continue <ArrowRight size={16} />
                </SquircleButton>
              </div>
            </>
          )}
          {modal === 'permission' && (
            <>
              <span className="s1-demo-label">SIMULATED OS HANDOFF</span>
              <p className="s1-body-copy">
                In the app, macOS asks whether Floe can access your calendars here. This preview
                does not request device permissions.
              </p>
              <Surface>
                <ShieldCheck size={26} />
                <h3>“Floe” would like full access to Calendar</h3>
                <p className="s1-body-copy">
                  Floe reads the calendar you select. It won’t change your external events.
                </p>
              </Surface>
              <div className="s1-modal-actions">
                <SquircleButton
                  className="secondary-button"
                  onClick={() => {
                    setPhase(hasCache ? 'revoked' : 'denied');
                    setModal(null);
                  }}
                >
                  Simulate denial
                </SquircleButton>
                <SquircleButton className="primary-button" onClick={picker}>
                  Simulate allow
                </SquircleButton>
              </div>
            </>
          )}
          {modal === 'picker' && (
            <>
              <p className="s1-body-copy">
                Choose one calendar. Your other calendars stay outside Floe.
              </p>
              <div className="s1-search">
                <Search size={18} />
                <input
                  autoFocus
                  aria-label="Search calendars"
                  placeholder="Find a calendar"
                  value={search}
                  onChange={(event) => setSearch(event.target.value)}
                />
              </div>
              <fieldset className="s1-calendar-options">
                <legend className="sr-only">Available calendars</legend>
                {phase !== 'noCalendars' &&
                  calendars
                    .filter((item) =>
                      `${item.name} ${item.account}`.toLowerCase().includes(search.toLowerCase()),
                    )
                    .map((item) => (
                      <label key={item.id} className={choice === item.id ? 'selected' : ''}>
                        <input
                          type="radio"
                          name="calendar"
                          value={item.id}
                          checked={choice === item.id}
                          onChange={() => setChoice(item.id)}
                        />
                        <span className={`tone-dot ${item.color}`} />
                        <span>
                          <strong>{item.name}</strong>
                          <small>{item.account}</small>
                        </span>
                        {choice === item.id && <Check size={18} />}
                      </label>
                    ))}
              </fieldset>
              {(phase === 'noCalendars' ||
                !calendars.some((item) =>
                  `${item.name} ${item.account}`.toLowerCase().includes(search.toLowerCase()),
                )) && (
                <div className="s1-picker-empty">
                  <CalendarDays size={25} />
                  <h3>
                    {phase === 'noCalendars'
                      ? 'No calendars on this Mac yet.'
                      : 'No matching calendars.'}
                  </h3>
                  <p>
                    {phase === 'noCalendars'
                      ? 'Add an account or a calendar in macOS Calendar, then come back here.'
                      : 'Try another name. Your selection hasn’t changed.'}
                  </p>
                  <button
                    className="s1-text-link"
                    onClick={() => {
                      setSearch('');
                      if (phase === 'noCalendars') {
                        setPhase('disconnected');
                        notify('Demo calendar list restored.');
                      }
                    }}
                  >
                    {phase === 'noCalendars' ? 'Simulate calendars added' : 'Clear search'}
                  </button>
                </div>
              )}
              <div className="s1-note">
                <Database size={16} />
                <p>
                  Floe will read {dateShort} in Asia/Seoul. Other dates are collected only when you
                  refresh them.
                </p>
              </div>
              <div className="s1-modal-actions">
                <SquircleButton className="secondary-button" onClick={() => setModal(null)}>
                  Cancel
                </SquircleButton>
                <SquircleButton
                  className="primary-button"
                  disabled={
                    phase === 'noCalendars' ||
                    !calendars.some(
                      (item) =>
                        item.id === choice &&
                        `${item.name} ${item.account}`.toLowerCase().includes(search.toLowerCase()),
                    )
                  }
                  onClick={selectCalendar}
                >
                  {hasConnection ? 'Use this calendar' : 'Connect calendar'}
                </SquircleButton>
              </div>
            </>
          )}
          {modal === 'switch' && (
            <>
              <p className="s1-body-copy">
                Replace <strong>{calendar.name}</strong> with{' '}
                <strong>{calendars.find((item) => item.id === choice).name}</strong> in Floe?
              </p>
              <div className="s1-note">
                <Info size={18} />
                <p>
                  The saved copy of {calendar.name} will be removed from Floe. Local tasks and notes
                  stay. Nothing changes in macOS Calendar.
                </p>
              </div>
              <div className="s1-modal-actions">
                <SquircleButton className="secondary-button" onClick={() => setModal('picker')}>
                  Go back
                </SquircleButton>
                <SquircleButton
                  className="primary-button"
                  onClick={() => refresh(calendars.find((item) => item.id === choice))}
                >
                  Switch & read
                </SquircleButton>
              </div>
            </>
          )}
          {modal === 'settings' && (
            <>
              <p className="s1-body-copy">
                Your saved events are safe. To read changes again, allow Calendar access in macOS
                Settings, then return to Floe.
              </p>
              <ol className="s1-settings-steps">
                <li>Open System Settings</li>
                <li>Privacy & Security → Calendars</li>
                <li>Enable full access for Floe</li>
              </ol>
              <div className="s1-note">
                <ShieldCheck size={18} />
                <p>
                  This demo won’t open or change your settings. The real app should recheck access
                  when you return.
                </p>
              </div>
              <div className="s1-modal-actions">
                <SquircleButton className="secondary-button" onClick={() => setModal(null)}>
                  Keep saved data
                </SquircleButton>
                <SquircleButton
                  className="primary-button"
                  onClick={() => {
                    setPhase(hasCache ? 'cached' : 'disconnected');
                    picker();
                  }}
                >
                  Simulate access restored <ExternalLink size={15} />
                </SquircleButton>
              </div>
            </>
          )}
          {modal === 'disconnect' && (
            <>
              <p className="s1-body-copy">
                Remove the saved copy of <strong>{calendar.name}</strong> from this Floe prototype
                and stop reading it.
              </p>
              <ul className="s1-permission-list">
                <li>
                  <Check size={17} /> Your external calendar is untouched
                </li>
                <li>
                  <Check size={17} /> Your Floe tasks and notes stay here
                </li>
                <li>
                  <Info size={17} /> OS permission is managed separately in Settings
                </li>
              </ul>
              <div className="s1-modal-actions">
                <SquircleButton className="secondary-button" onClick={() => setModal(null)}>
                  Keep connected
                </SquircleButton>
                <SquircleButton
                  className="s1-danger-button"
                  onClick={() => {
                    preview('disconnected');
                    notify('Disconnected. Your local tasks and notes are still here.');
                  }}
                >
                  Disconnect
                </SquircleButton>
              </div>
            </>
          )}
          {typeof modal === 'object' && (
            <>
              <div className="s1-detail-source">
                <span className={`tone-dot ${calendar.color}`} />
                {calendar.name} · {calendar.account}
                <span className="s1-pill">
                  <LockKeyhole size={12} /> Read-only
                </span>
              </div>
              <p className="s1-event-purpose">
                {modal.detail || 'A full day, not a midnight appointment.'}
              </p>
              <div className="s1-detail-time">
                <Clock3 size={20} />
                <div>
                  <strong>{modal.time}</strong>
                  <span>{dateLabel} · Asia/Seoul</span>
                </div>
              </div>
              <dl className="s1-facts">
                <div>
                  <dt>Original time</dt>
                  <dd>
                    {modal.id === 'remote' && dayOffset !== 0
                      ? `${dateShort}, 12:00 – 12:45 AM PDT`
                      : modal.original.replace('Sep 4', dateShort)}
                  </dd>
                </div>
                <div>
                  <dt>Source time zone</dt>
                  <dd>{modal.timezone}</dd>
                </div>
                {modal.recurring && (
                  <div>
                    <dt>Repeats</dt>
                    <dd>Weekdays · this occurrence only</dd>
                  </div>
                )}
                <div>
                  <dt>Last collected</dt>
                  <dd>{stale ? 'Saved at 2:28 PM · may be out of date' : 'Today at 2:28 PM'}</dd>
                </div>
                {modal.allDay && (
                  <div>
                    <dt>All-day boundary</dt>
                    <dd>{modal.endDateExclusive || 'Sep 5, 2026 · exclusive'}</dd>
                  </div>
                )}
              </dl>
              <div className="s1-note">
                <LockKeyhole size={17} />
                <p>
                  Manage this event in its original calendar. Floe has no edit or delete action for
                  imported events.
                </p>
              </div>
              <details className="s1-source-details">
                <summary>Source details</summary>
                <dl>
                  <dt>Connection / Person</dt>
                  <dd>demo-{calendar.id} / You</dd>
                  <dt>External occurrence ID</dt>
                  <dd>
                    fixture:{calendar.id}:{modal.id}:2026-09-
                    {String(4 + dayOffset).padStart(2, '0')}
                  </dd>
                  <dt>Change token</dt>
                  <dd>
                    {externalRevision && modal.id === 'design'
                      ? 'fixture-revision-05'
                      : 'fixture-revision-04'}
                  </dd>
                  <dt>Integration</dt>
                  <dd>Fixture · not a live EventKit record</dd>
                </dl>
              </details>
              <div className="s1-modal-actions">
                <SquircleButton className="primary-button" onClick={() => setModal(null)}>
                  Back to my day
                </SquircleButton>
              </div>
            </>
          )}
        </Modal>
      )}
    </div>
  );
}

function StatusBanner({ phase, dateLabel, onConnect, onRefresh, onSettings, onPicker }) {
  const states = {
    disconnected: [
      'neutral',
      Link2,
      'A calendar, when you’re ready.',
      'Bring your existing events into Floe. Your own tasks and notes work without a connection.',
      'Connect Calendar',
      onConnect,
    ],
    syncing: [
      'neutral',
      LoaderCircle,
      'Reading your calendar…',
      'Your last saved events stay visible until this read finishes. No external changes are made.',
    ],
    cached: [
      'neutral',
      Clock3,
      'Your day, saved on this Mac.',
      'Last collected today at 2:28 PM. Refresh to see changes made since then.',
      'Refresh',
      onRefresh,
    ],
    offline: [
      'warning',
      WifiOff,
      'We couldn’t read the latest changes.',
      'Showing your saved events from 2:28 PM. Nothing was removed. Try again when Calendar is available.',
      'Try again',
      onRefresh,
    ],
    denied: [
      'warning',
      LockKeyhole,
      'Calendar access wasn’t granted.',
      'That’s okay. Your local day still works. Allow access in Settings when you want to connect.',
      'Review access',
      onSettings,
    ],
    revoked: [
      'warning',
      LockKeyhole,
      'Calendar access has changed.',
      'New reads are paused. Your last saved events are still here, but may be out of date.',
      'Reconnect',
      onSettings,
    ],
    missing: [
      'warning',
      CalendarDays,
      'This calendar is no longer available.',
      'It may have been removed or its account disconnected. Your last saved copy is unchanged.',
      'Choose a calendar',
      onPicker,
    ],
    noCalendars: [
      'neutral',
      CalendarDays,
      'No calendars found on this Mac.',
      'Add one in macOS Calendar, then return to select it. Floe will not create a calendar for you.',
      'Choose a calendar',
      onPicker,
    ],
    uncollected: [
      'neutral',
      CalendarDays,
      `${dateLabel} hasn’t been read yet.`,
      'Other saved dates are untouched. Read this date to find out what’s on your calendar.',
      'Read this date',
      onRefresh,
    ],
    loadError: [
      'warning',
      Database,
      'A local read needs attention.',
      'Calendar refresh is paused until Floe can load your local data.',
    ],
  };
  const [tone, Icon, title, description, action, handler] = states[phase];
  return (
    <div className={`s1-banner ${tone}`} role={tone === 'warning' ? 'alert' : 'status'}>
      <Icon size={19} className={phase === 'syncing' ? 's1-spin' : ''} />
      <div>
        <strong>{title}</strong>
        <p>{description}</p>
      </div>
      {action && (
        <SquircleButton className="s1-banner-action" onClick={handler}>
          {action}
          <ArrowRight size={14} />
        </SquircleButton>
      )}
    </div>
  );
}

function Modal({ title, children, onClose }) {
  const dialog = useRef(null);
  useEffect(() => {
    const previous = document.activeElement;
    const overflow = document.body.style.overflow;
    document.body.style.overflow = 'hidden';
    const element = dialog.current;
    element.showModal();
    return () => {
      element.close();
      document.body.style.overflow = overflow;
      previous?.focus();
    };
  }, []);
  return (
    <dialog
      ref={dialog}
      className="s1-dialog"
      aria-labelledby="s1-modal-title"
      onCancel={(event) => {
        event.preventDefault();
        onClose();
      }}
      onClick={(event) => {
        if (event.target === dialog.current) onClose();
      }}
    >
      <SquircleSurface radius={34} className="s1-modal-border" contentClassName="s1-modal">
        <div className="s1-modal-heading">
          <span className="s1-eyebrow">FLOE / CALENDAR</span>
          <SquircleButton aria-label="Close dialog" className="icon-button" onClick={onClose}>
            <X size={19} />
          </SquircleButton>
        </div>
        <h2 id="s1-modal-title">{title}</h2>
        {children}
      </SquircleSurface>
    </dialog>
  );
}
