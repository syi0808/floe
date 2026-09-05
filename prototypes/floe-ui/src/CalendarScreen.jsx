import { CalendarDateToolbar } from './components/calendar/CalendarDateToolbar.jsx';
import { useEffect, useLayoutEffect, useReducer, useRef, useState } from 'react';
import './calendar.css';
import { layoutTimedEvents } from './calendar-layout.js';
import {
  calendars as fixtureCalendars,
  scenarios,
  externalEvents,
  getAllDayEvents,
} from './components/calendar/calendar-fixtures.js';
import { CalendarDays, Database } from 'lucide-react';
import { ConnectorList } from './components/connectors/ConnectorList.jsx';
import { SquircleButton } from './primitives.jsx';
import { CalendarSurface as Surface } from './components/calendar/CalendarSurface.jsx';
import { CalendarStatusBanner } from './components/calendar/CalendarStatusBanner.jsx';
import { CalendarAgenda } from './components/calendar/CalendarAgenda.jsx';
import { CalendarContextRail } from './components/calendar/CalendarContextRail.jsx';
import { CalendarConnections } from './components/calendar/CalendarConnections.jsx';
import { CalendarDialogs } from './components/calendar/CalendarDialogs.jsx';
import { CalendarActionCard } from './components/calendar/CalendarActionCard.jsx';
import { CalendarActionDialog } from './components/calendar/CalendarActionDialog.jsx';
import { CalendarScopePicker } from './components/calendar/CalendarScopePicker.jsx';
import { Modal } from './components/ui/Modal.jsx';
import { dstFixture } from './calendar-dst-fixture.js';
import { actionReducer, initialAction, actionEvent, actionScenarios } from './calendar-action-state.js';

export function CalendarScreen({ page, onNavigate, notify }) {
  const dst = dstFixture(new URLSearchParams(window.location.search).get('dst'));
  const [scope, setScope] = useState('all');
  const [selectedIds, setSelectedIds] = useState(fixtureCalendars.map(calendar => calendar.id));
  const [scopeDraft, setScopeDraft] = useState(null);
  const calendars = fixtureCalendars.filter(calendar => scope === 'all' || selectedIds.includes(calendar.id));
  const [action, dispatchAction] = useReducer(actionReducer, null, () => {
    const scenario = new URLSearchParams(window.location.search).get('action');
    return initialAction(actionScenarios.includes(scenario) ? scenario : 'ready');
  });
  const [actionOpen, setActionOpen] = useState(false);
  const timedEvents = layoutTimedEvents([...(dst?.events ?? externalEvents), ...(!dst && action.status === 'succeeded' ? [actionEvent(action)] : [])].filter(event => calendars.some(calendar => calendar.id === event.calendarId)));
  useEffect(() => {
    const type = { checking: 'checked', creating: 'created', importing: 'imported', 'looking-up': 'found' }[action.status];
    if (!type) return;
    const pending = setTimeout(() => dispatchAction({ type }), 900);
    return () => clearTimeout(pending);
  }, [action.status]);
  const [phase, setPhase] = useState(() => {
    const scenario = new URLSearchParams(window.location.search).get('state');
    return Object.hasOwn(scenarios, scenario) ? scenario : 'connected';
  });
  const [dayOffset, setDayOffset] = useState(() => (phase === 'uncollected' ? 1 : 0));
  const [pixelsPerMinute, setPixelsPerMinute] = useState(1);
  const timelineScroll = useRef(null);
  const scrollMinute = useRef(8 * 60);
  const [modal, setModal] = useState(null);
  const [taskDone, setTaskDone] = useState(false);
  const [readDates, setReadDates] = useState(() =>
    ['connected', 'syncing', 'cached', 'offline', 'revoked', 'missing', 'uncollected', 'partial'].includes(
      phase,
    )
      ? [0]
      : [],
  );
  const timer = useRef(null);
  const announceRead = useRef(false);
  const date = dst ? new Date(`${dst.date}T00:00:00Z`) : new Date(Date.UTC(2026, 8, 4 + dayOffset));
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
    ['connected', 'syncing', 'cached', 'offline', 'revoked', 'missing', 'partial'].includes(phase) &&
    dayOffset === 0 &&
    readDates.includes(0);
  const hasConnection = !['disconnected', 'denied', 'noCalendars'].includes(phase);
  const stale = ['cached', 'offline', 'revoked', 'missing', 'partial'].includes(phase);
  const statusTone = ['offline', 'revoked', 'missing', 'loadError', 'partial'].includes(phase)
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
    },
    [],
  );

  useLayoutEffect(() => {
    if (timelineScroll.current) {
      timelineScroll.current.scrollTop = scrollMinute.current * pixelsPerMinute;
    }
  }, [page, pixelsPerMinute, phase === 'loadError']);

  useEffect(() => {
    if (phase !== 'syncing') return;
    const offset = dayOffset;
    const announce = announceRead.current;
    timer.current = setTimeout(() => {
      setPhase(offset === 0 ? 'connected' : 'empty');
      setReadDates((current) => [...new Set([...current, offset])]);
      if (announce) notify('Calendars refreshed', 'Your local tasks and notes are unchanged.');
    }, 1100);
    return () => clearTimeout(timer.current);
  }, [phase, dayOffset, notify]);

  function refresh(offset = dayOffset, announce = true) {
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
      partial: 'Some calendars couldn’t refresh',
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
              description: 'Bring events from your Mac into your day.',
              icon: CalendarDays,
              connected: hasConnection,
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
          onBack={() => onNavigate('connections')}
          scope={scope}
          onScope={() => setScopeDraft({ scope, selectedIds: [...selectedIds] })}
        />
      ) : (
        <>
          <CalendarDateToolbar
            dateLabel={dateLabel}
            disabled={phase === 'syncing' || !!dst}
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
                dayMinutes={dst?.minutes}
                hourLabels={dst?.labels}
                hasCache={hasCache}
                phase={phase}
                stale={stale}
                currentTime={!dst && dayOffset === 0 ? { minutes: 14 * 60 + 28, label: '2:28 PM' } : null}
                dateShort={dateShort}
                pixelsPerMinute={pixelsPerMinute}
                onZoomChange={setPixelsPerMinute}
                timelineScroll={timelineScroll}
                onScrollMinute={(minute) => {
                  scrollMinute.current = minute;
                }}
                timedEvents={timedEvents}
                calendars={calendars}
                allDayEvents={dst ? [] : getAllDayEvents(dateShort).filter(event => calendars.some(calendar => calendar.id === event.calendarId))}
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
            <div className="s1-side-stack">
            {!dst && dayOffset === 0 && <CalendarActionCard action={action} disabled={action.status === 'pending' && phase !== 'connected'} onReview={() => setActionOpen(true)} />}
            <CalendarContextRail
              taskDone={taskDone}
              onTaskChange={(completed) => {
                setTaskDone(completed);
                notify(completed ? 'Task completed' : 'Task marked incomplete', null, 'success', {
                  label: 'Undo',
                  onClick: () => setTaskDone(!completed),
                });
              }}
              hasCache={hasCache}
              onNavigate={onNavigate}
            />
            </div>
          </div>
        </>
      )}

      {modal && (
        <CalendarDialogs
          displayTimezone={dst ? 'America/Los_Angeles' : 'Asia/Seoul'}
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
            notify('Calendar disconnected', 'Your local tasks and notes are still here.', 'info');
          }}
        />
      )}
      {actionOpen && <CalendarActionDialog action={action} calendars={fixtureCalendars} connected={phase === 'connected'} onAction={dispatchAction} onClose={() => setActionOpen(false)} />}
      {scopeDraft && <Modal title="Choose calendar scope" onClose={() => setScopeDraft(null)}><CalendarScopePicker
        calendars={fixtureCalendars} scope={scopeDraft.scope} selectedIds={scopeDraft.selectedIds}
        onScope={scope => setScopeDraft(current => ({ ...current, scope }))}
        onToggle={id => setScopeDraft(current => ({ ...current, selectedIds: current.selectedIds.includes(id) ? current.selectedIds.filter(value => value !== id) : [...current.selectedIds, id] }))}
        onCancel={() => setScopeDraft(null)}
        onSave={() => { setScope(scopeDraft.scope); setSelectedIds(scopeDraft.selectedIds); setScopeDraft(null); refresh(); }}
      /></Modal>}
    </div>
  );
}
