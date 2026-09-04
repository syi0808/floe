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
  ShieldCheck,
  Unplug,
  WifiOff,
  X,
} from 'lucide-react';
import { SquircleBlock, SquircleButton, SquircleSurface } from './primitives.jsx';
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
const timelineStartMinutes = 8 * 60;
const pixelsPerMinute = 1;
const externalEvents = [
  {
    id: 'standup',
    calendarId: 'work',
    title: 'A little alignment',
    time: '9:30 – 10:00 AM',
    startMinutes: 9 * 60 + 30,
    endMinutes: 10 * 60,
    detail: 'Daily stand-up',
    timezone: 'Asia/Seoul',
    original: 'Sep 4, 9:30 – 10:00 AM KST',
    recurring: true,
  },
  {
    id: 'design',
    calendarId: 'work',
    title: 'Make room for the details',
    time: '11:00 AM – 12:00 PM',
    startMinutes: 11 * 60,
    endMinutes: 12 * 60,
    detail: 'Design review',
    timezone: 'Asia/Seoul',
    original: 'Sep 4, 11:00 AM – 12:00 PM KST',
  },
  {
    id: 'remote',
    calendarId: 'team',
    title: 'Across time zones',
    time: '4:00 – 4:45 PM',
    startMinutes: 16 * 60,
    endMinutes: 16 * 60 + 45,
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
  const [phase, setPhase] = useState(() => {
    const scenario = new URLSearchParams(window.location.search).get('state');
    return Object.hasOwn(scenarios, scenario) ? scenario : 'connected';
  });
  const [dayOffset, setDayOffset] = useState(() => (phase === 'uncollected' ? 1 : 0));
  const [modal, setModal] = useState(null);
  const [toast, setToast] = useState('');
  const [taskDone, setTaskDone] = useState(false);
  const [capture, setCapture] = useState('');
  const [localNotes, setLocalNotes] = useState([]);
  const [readDates, setReadDates] = useState(() =>
    ['connected', 'syncing', 'cached', 'offline', 'revoked', 'missing', 'uncollected'].includes(
      phase,
    )
      ? [0]
      : [],
  );
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
    readDates.includes(0);
  const hasConnection = !['disconnected', 'denied', 'noCalendars'].includes(phase);
  const stale = ['cached', 'offline', 'revoked', 'missing'].includes(phase);
  const statusTone = ['offline', 'revoked', 'missing', 'loadError'].includes(phase)
    ? 'warning'
    : ['connected', 'empty'].includes(phase)
      ? ''
      : 'neutral';
  const detailCalendar =
    typeof modal === 'object' && modal
      ? calendars.find((item) => item.id === modal.calendarId)
      : null;

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

  function refresh(offset = dayOffset, announce = true) {
    clearTimeout(timer.current);
    clearTimeout(toastTimer.current);
    setToast('');
    setModal(null);
    setPhase('syncing');
    timer.current = setTimeout(() => {
      setPhase(offset === 0 ? 'connected' : 'empty');
      setReadDates((current) => [...new Set([...current, offset])]);
      if (announce) notify('All calendars refreshed. Your local tasks and notes are unchanged.');
    }, 1100);
  }

  function moveDay(offset) {
    clearTimeout(timer.current);
    setDayOffset(offset);
    if (hasConnection && !['revoked', 'missing', 'offline', 'loadError'].includes(phase))
      refresh(offset, false);
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
      {page === 'connections' ? (
        <>
          <button className="back-action s1-back" onClick={() => onNavigate('today')}>
            <ArrowLeft size={16} /> Back to your day
          </button>
          <div className="s1-connections-layout">
            <Surface>
              <div className="s1-card-heading">
                <SquircleBlock className="s1-provider-icon">
                  <CalendarDays size={25} />
                </SquircleBlock>
                <div>
                  <h2>macOS Calendar</h2>
                  <p>Calendars already on this Mac</p>
                </div>
                <SquircleBlock className="s1-pill" radius={8}>
                  <LockKeyhole size={12} /> Read-only
                </SquircleBlock>
              </div>
              <p className="s1-body-copy">
                Bring all calendars on this Mac into one day. Floe reads their events; it never
                creates, edits, or deletes anything in Calendar.
              </p>
              <div className="s1-connection-record">
                <div>
                  <span className="s1-meta-label">Connected calendars</span>
                  <strong>
                    {hasConnection ? 'All calendars on this Mac' : 'Nothing connected yet'}
                  </strong>
                  <small>
                    {hasConnection
                      ? '3 calendars · 2 accounts'
                      : 'All available calendars are included after granting access'}
                  </small>
                </div>
                {!['connected', 'empty'].includes(phase) && (
                  <span className={`s1-status ${statusTone}`}>
                    <span />
                    {statusLabel}
                  </span>
                )}
              </div>
              {hasConnection && (
                <ul className="s1-connected-calendars" aria-label="Connected calendars">
                  {calendars.map((item) => (
                    <li key={item.id}>
                      <span className={'tone-dot ' + item.color} />
                      <strong>{item.name}</strong>
                      <small>{item.account}</small>
                    </li>
                  ))}
                </ul>
              )}
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
                  <dd>On date changes · or when you refresh</dd>
                </div>
              </dl>
              <div className="s1-actions">
                <SquircleButton
                  className="primary-button"
                  disabled={phase === 'syncing' || phase === 'loadError'}
                  onClick={() =>
                    phase === 'revoked'
                      ? setModal('settings')
                      : hasConnection
                        ? refresh()
                        : setModal('disclosure')
                  }
                >
                  {hasConnection ? 'Refresh all calendars' : 'Connect Calendar'}
                  <ArrowRight size={16} />
                </SquircleButton>
                <SquircleButton className="secondary-button" onClick={() => setModal('settings')}>
                  Manage access
                </SquircleButton>
              </div>
              {hasConnection && (
                <SquircleButton
                  className="s1-danger-link s1-quiet-action"
                  onClick={() => setModal('disconnect')}
                >
                  <Unplug size={14} /> Disconnect from Floe
                </SquircleButton>
              )}
            </Surface>
            <div className="s1-side-stack">
              <Surface>
                <ShieldCheck size={23} className="s1-violet" />
                <h2>A clear boundary.</h2>
                <p className="s1-body-copy">
                  All calendars available through macOS Calendar are included. Titles, times, time
                  zones, and source identifiers stay on this Mac.
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
            {hasConnection && (
              <SquircleButton
                className="s1-refresh"
                aria-label="Refresh selected date"
                disabled={phase === 'syncing' || phase === 'loadError'}
                onClick={() => (phase === 'revoked' ? setModal('settings') : refresh())}
              >
                <RefreshCw size={15} />
              </SquircleButton>
            )}
          </div>
          {!['connected', 'empty', 'syncing'].includes(phase) && (
            <StatusBanner
              phase={phase}
              dateLabel={dateShort}
              onConnect={() => setModal('disclosure')}
              onRefresh={() => refresh()}
              onSettings={() => setModal('settings')}
              onPicker={() => onNavigate('connections')}
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
                {phase === 'syncing' && (
                  <div className="s1-calendar-loading" role="status">
                    <LoaderCircle size={25} className="s1-spin" aria-hidden="true" />
                    <span>Loading calendar…</span>
                  </div>
                )}
                <div className="s1-all-day">
                  <span>All day</span>
                  {hasCache ? (
                    <button
                      disabled={phase === 'syncing'}
                      onClick={() =>
                        setModal({
                          id: 'all-day',
                          calendarId: 'personal',
                          title: 'A day for making',
                          time: 'All day',
                          timezone: 'Asia/Seoul',
                          original: `${dateShort}, all day`,
                          allDay: true,
                        })
                      }
                    >
                      <span className="tone-dot mint" />
                      <span className="s1-all-day-title">A day for making</span>
                      <small>Personal</small>
                    </button>
                  ) : (
                    <span className="s1-muted">—</span>
                  )}
                  {hasCache && (
                    <button
                      className="s1-more-all-day"
                      disabled={phase === 'syncing'}
                      onClick={() =>
                        setModal({
                          id: 'multi-day',
                          calendarId: 'team',
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
                      <span className="tone-dot violet" />
                      <span className="s1-all-day-title">Research week</span>
                      <small>Day 2 of 3</small>
                    </button>
                  )}
                </div>
                <div
                  className={`s1-time-grid ${stale ? 's1-cached-grid' : ''}`}
                  aria-label="Day timeline"
                  style={{ height: 10 * 60 * pixelsPerMinute }}
                >
                  {Array.from({ length: 10 }, (_, index) => (
                    <div
                      className="s1-hour"
                      key={index}
                      style={{ top: index * 60 * pixelsPerMinute }}
                    >
                      <time>
                        {index + 8 > 12 ? index - 4 : index + 8}
                        {index + 8 >= 12 ? ' PM' : ' AM'}
                      </time>
                      <span />
                    </div>
                  ))}
                  {hasCache &&
                    externalEvents.map((event) => (
                      <SquircleButton
                        key={event.id}
                        disabled={phase === 'syncing'}
                        className={`s1-event s1-event-${calendars.find((item) => item.id === event.calendarId).color}`}
                        data-density={
                          event.endMinutes - event.startMinutes <= 30
                            ? 'compact'
                            : event.endMinutes - event.startMinutes < 60
                              ? 'medium'
                              : 'full'
                        }
                        aria-label={`${event.title} · ${event.time} · ${calendars.find((item) => item.id === event.calendarId).name} · ${event.detail}`}
                        style={{
                          top: (event.startMinutes - timelineStartMinutes) * pixelsPerMinute,
                          height: (event.endMinutes - event.startMinutes) * pixelsPerMinute,
                        }}
                        onClick={() => setModal(event)}
                      >
                        <span
                          className={`tone-dot ${calendars.find((item) => item.id === event.calendarId).color}`}
                        />
                        <span>
                          <strong>{event.title}</strong>
                          <time>{event.time}</time>
                          <small>
                            {calendars.find((item) => item.id === event.calendarId).name} ·{' '}
                            {event.detail}
                            {event.recurring ? ' · Repeats' : ''}
                          </small>
                        </span>
                        <LockKeyhole size={13} />
                      </SquircleButton>
                    ))}
                  {!hasCache && phase !== 'syncing' && (
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
                          ? 'Reading all connected calendars. Your own tasks and notes stay available.'
                          : phase === 'empty'
                            ? `No events across your connected calendars for ${dateShort}. This date was checked successfully.`
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
                            ? 'View connected calendars'
                            : phase === 'uncollected'
                              ? 'Read this date'
                              : 'Connect Calendar'}
                      </SquircleButton>
                    </div>
                  )}
                  {dayOffset === 0 && (
                    <div
                      className="s1-now"
                      style={{ top: (14 * 60 + 28 - timelineStartMinutes) * pixelsPerMinute }}
                    >
                      <time>2:28 PM</time>
                      <span />
                    </div>
                  )}
                </div>
              </Surface>
            )}
            <aside className="s1-side-stack">
              <Surface>
                <div className="s1-card-heading">
                  <h2>Your own rhythm</h2>
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
                <SquircleButton
                  className="s1-text-link s1-quiet-action"
                  onClick={() => onNavigate('tasks')}
                >
                  See your tasks <ArrowRight size={15} />
                </SquircleButton>
              </Surface>
              <Surface>
                <div className="s1-card-heading">
                  <h2>A note to self</h2>
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
          <Surface className="s1-capture-card">
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
              <SquircleButton disabled={!capture.trim()} type="submit" aria-label="Save local note">
                <ArrowRight size={18} />
              </SquircleButton>
            </form>
          </Surface>
        </>
      )}

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
                  settings: 'Let Floe read your calendar again.',
                  disconnect: 'Disconnect Calendar?',
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
                Connect all calendars already on this Mac. Floe will keep a local copy of their
                events so your day is still there when you’re offline.
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
                  permission, but Floe only uses it to read. All available calendars are included,
                  including calendars added later when you refresh.
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
              <span className="s1-demo-label">Simulated OS handoff</span>
              <p className="s1-body-copy">
                In the app, macOS asks whether Floe can access your calendars here. This preview
                does not request device permissions.
              </p>
              <Surface>
                <ShieldCheck size={26} />
                <h3>“Floe” would like full access to Calendar</h3>
                <p className="s1-body-copy">
                  Floe reads all calendars on this Mac. It won’t change your external events.
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
                <SquircleButton className="primary-button" onClick={() => refresh()}>
                  Simulate allow
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
                    refresh();
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
                Remove the saved copies of all connected calendars from this Floe prototype and stop
                reading them.
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
                    clearTimeout(timer.current);
                    setPhase('disconnected');
                    setReadDates([]);
                    setModal(null);
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
                <span className={`tone-dot ${detailCalendar.color}`} />
                {detailCalendar.name} · {detailCalendar.account}
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
                  <dd>demo-macos / You</dd>
                  <dt>External occurrence ID</dt>
                  <dd>
                    fixture:{detailCalendar.id}:{modal.id}:2026-09-
                    {String(4 + dayOffset).padStart(2, '0')}
                  </dd>
                  <dt>Change token</dt>
                  <dd>fixture-revision-04</dd>
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
      'Reading your calendars…',
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
      'Some calendars need attention.',
      'An account or calendar may be unavailable. Its saved events remain; other calendars are still included.',
      'View calendars',
      onPicker,
    ],
    noCalendars: [
      'neutral',
      CalendarDays,
      'No calendars found on this Mac.',
      'Add one in macOS Calendar, then refresh in Floe. Floe will not create a calendar for you.',
      'View calendars',
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
          <h2 id="s1-modal-title">{title}</h2>
          <SquircleButton aria-label="Close dialog" className="icon-button" onClick={onClose}>
            <X size={19} />
          </SquircleButton>
        </div>
        {children}
      </SquircleSurface>
    </dialog>
  );
}
