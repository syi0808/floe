import { CalendarDateToolbar } from './components/calendar/CalendarDateToolbar.jsx';
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import './calendar.css';
import { layoutTimedEvents } from './calendar-layout.js';
import {
  calendars,
  scenarios,
  externalEvents,
  getAllDayEvents,
} from './components/calendar/calendar-fixtures.js';
import { CalendarDays, Check, Database, X } from 'lucide-react';
import { ConnectorList } from './components/connectors/ConnectorList.jsx';
import { SquircleButton } from './primitives.jsx';
import { CalendarSurface as Surface } from './components/calendar/CalendarSurface.jsx';
import { CalendarStatusBanner } from './components/calendar/CalendarStatusBanner.jsx';
import { CalendarAgenda } from './components/calendar/CalendarAgenda.jsx';
import { CalendarContextRail } from './components/calendar/CalendarContextRail.jsx';
import { CalendarCapture } from './components/calendar/CalendarCapture.jsx';
import { CalendarConnections } from './components/calendar/CalendarConnections.jsx';
import { CalendarDialogs } from './components/calendar/CalendarDialogs.jsx';

const timedEvents = layoutTimedEvents(externalEvents);

export function CalendarScreen({ page, onNavigate }) {
  const [phase, setPhase] = useState(() => {
    const scenario = new URLSearchParams(window.location.search).get('state');
    return Object.hasOwn(scenarios, scenario) ? scenario : 'connected';
  });
  const [dayOffset, setDayOffset] = useState(() => (phase === 'uncollected' ? 1 : 0));
  const [pixelsPerMinute, setPixelsPerMinute] = useState(1);
  const timelineScroll = useRef(null);
  const scrollMinute = useRef(8 * 60);
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
  const announceRead = useRef(false);
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

  useLayoutEffect(() => {
    if (timelineScroll.current) {
      timelineScroll.current.scrollTop = scrollMinute.current * pixelsPerMinute;
    }
  }, [page, pixelsPerMinute, phase === 'loadError']);

  const notify = useCallback((message) => {
    clearTimeout(toastTimer.current);
    setToast(message);
    toastTimer.current = setTimeout(() => setToast(''), 4500);
  }, []);

  useEffect(() => {
    if (phase !== 'syncing') return;
    const offset = dayOffset;
    const announce = announceRead.current;
    timer.current = setTimeout(() => {
      setPhase(offset === 0 ? 'connected' : 'empty');
      setReadDates((current) => [...new Set([...current, offset])]);
      if (announce) notify('All calendars refreshed. Your local tasks and notes are unchanged.');
    }, 1100);
    return () => clearTimeout(timer.current);
  }, [phase, dayOffset, notify]);

  function refresh(offset = dayOffset, announce = true) {
    clearTimeout(toastTimer.current);
    setToast('');
    setModal(null);
    announceRead.current = announce;
    setDayOffset(offset);
    setPhase('syncing');
  }

  function moveDay(offset) {
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
        <ConnectorList
          services={[
            {
              id: 'macos-calendar',
              name: 'macOS Calendar',
              icon: CalendarDays,
              readOnly: true,
              connected: hasConnection,
              description: hasConnection
                ? `${calendars.length} calendars · ${new Set(calendars.map((calendar) => calendar.account)).size} accounts`
                : 'Calendars already on this Mac',
              status: hasConnection ? statusLabel : 'Not connected',
              tone:
                statusTone === 'warning'
                  ? 'warning'
                  : hasConnection && !statusTone
                    ? 'connected'
                    : 'neutral',
            },
          ]}
          onSelect={() => onNavigate('calendar-connection')}
        />
      ) : page === 'calendar-connection' ? (
        <CalendarConnections
          hasConnection={hasConnection}
          phase={phase}
          statusTone={statusTone}
          statusLabel={statusLabel}
          readDates={readDates}
          calendars={calendars}
          onOpenDialog={setModal}
          onRefresh={() => refresh()}
        />
      ) : (
        <>
          <CalendarDateToolbar
            dateLabel={dateLabel}
            disabled={phase === 'syncing'}
            refreshDisabled={phase === 'syncing' || phase === 'loadError'}
            showRefresh={hasConnection}
            onPrevious={() => moveDay(dayOffset - 1)}
            onNext={() => moveDay(dayOffset + 1)}
            onToday={() => moveDay(0)}
            onRefresh={() => (phase === 'revoked' ? setModal('settings') : refresh())}
          />
          {!['connected', 'empty', 'syncing'].includes(phase) && (
            <CalendarStatusBanner
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
              <CalendarAgenda
                hasCache={hasCache}
                phase={phase}
                stale={stale}
                currentTime={dayOffset === 0 ? { minutes: 14 * 60 + 28, label: '2:28 PM' } : null}
                dateShort={dateShort}
                pixelsPerMinute={pixelsPerMinute}
                onZoomChange={setPixelsPerMinute}
                timelineScroll={timelineScroll}
                onScrollMinute={(minute) => {
                  scrollMinute.current = minute;
                }}
                timedEvents={timedEvents}
                calendars={calendars}
                allDayEvents={getAllDayEvents(dateShort)}
                onEventSelect={setModal}
                onEmptyAction={() =>
                  phase === 'empty'
                    ? onNavigate('connections')
                    : phase === 'uncollected'
                      ? refresh()
                      : setModal('disclosure')
                }
              />
            )}
            <CalendarContextRail
              taskDone={taskDone}
              onTaskChange={setTaskDone}
              localNotes={localNotes}
              hasCache={hasCache}
              onNavigate={onNavigate}
            />
          </div>
          <CalendarCapture
            value={capture}
            onChange={setCapture}
            onSubmit={(value) => {
              setLocalNotes((items) => [...items, value]);
              setCapture('');
              notify('Note saved in Floe. No calendar changes.');
            }}
          />
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
        <CalendarDialogs
          modal={modal}
          detailCalendar={detailCalendar}
          dateLabel={dateLabel}
          dateShort={dateShort}
          dayOffset={dayOffset}
          stale={stale}
          onClose={() => setModal(null)}
          onPermission={() => setModal('permission')}
          onDeny={() => {
            setPhase(hasCache ? 'revoked' : 'denied');
            setModal(null);
          }}
          onRefresh={() => refresh()}
          onDisconnect={() => {
            clearTimeout(timer.current);
            setPhase('disconnected');
            setReadDates([]);
            setModal(null);
            notify('Disconnected. Your local tasks and notes are still here.');
          }}
        />
      )}
    </div>
  );
}
