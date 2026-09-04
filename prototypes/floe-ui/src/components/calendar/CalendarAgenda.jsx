import { LoaderCircle } from 'lucide-react';
import { CalendarSurface as Surface } from './CalendarSurface.jsx';
import { CalendarEvent } from './CalendarEvent.jsx';
import { TimelineZoom } from './TimelineZoom.jsx';
import { CalendarEmptyState } from './CalendarEmptyState.jsx';
import { CalendarAllDay } from './CalendarAllDay.jsx';

export function CalendarAgenda({
  hasCache,
  phase,
  stale,
  currentTime,
  dateShort,
  pixelsPerMinute,
  onZoomChange,
  timelineScroll,
  onScrollMinute,
  timedEvents,
  calendars,
  allDayEvents,
  onEventSelect,
  onEmptyAction,
}) {
  const timelineColumns = Math.max(1, ...timedEvents.map((event) => event.columns));
  return (
    <Surface className="s1-agenda">
      {phase === 'syncing' && (
        <div className="s1-calendar-loading" role="status">
          <LoaderCircle size={25} className="s1-spin" aria-hidden="true" />
          <span>Loading calendar…</span>
        </div>
      )}
      <div inert={!hasCache && phase !== 'syncing'} aria-hidden={!hasCache && phase !== 'syncing'}>
        <CalendarAllDay
          events={hasCache ? allDayEvents : []}
          disabled={phase === 'syncing'}
          onSelect={onEventSelect}
        />
        <TimelineZoom value={pixelsPerMinute} onChange={onZoomChange} />
        <div className="s1-timeline-viewport">
          <div
            className="s1-timeline-scroll"
            ref={timelineScroll}
            role="region"
            aria-label="24-hour calendar"
            tabIndex={0}
            onScroll={(event) => {
              onScrollMinute(event.currentTarget.scrollTop / pixelsPerMinute);
            }}
          >
            <div
              className={`s1-time-grid ${stale ? 's1-cached-grid' : ''}`}
              aria-label="Day timeline"
              style={{
                height: 24 * 60 * pixelsPerMinute,
                '--timeline-columns': hasCache ? timelineColumns : 1,
              }}
            >
              {Array.from({ length: 25 }, (_, index) => (
                <div className="s1-hour" key={index} style={{ top: index * 60 * pixelsPerMinute }}>
                  <time>{String(index).padStart(2, '0')}:00</time>
                  <span />
                </div>
              ))}
              {Array.from({ length: 24 }, (_, index) => (
                <div
                  key={index}
                  className="s1-half-hour-line"
                  style={{ top: (index * 60 + 30) * pixelsPerMinute }}
                />
              ))}
              {hasCache &&
                timedEvents.map((event) => (
                  <CalendarEvent
                    key={event.id}
                    event={event}
                    calendar={calendars.find((item) => item.id === event.calendarId)}
                    pixelsPerMinute={pixelsPerMinute}
                    disabled={phase === 'syncing'}
                    onSelect={onEventSelect}
                  />
                ))}
              {currentTime && (
                <div className="s1-now" style={{ top: currentTime.minutes * pixelsPerMinute }}>
                  <time>{currentTime.label}</time>
                  <span />
                </div>
              )}
            </div>
          </div>
        </div>
      </div>
      {!hasCache && phase !== 'syncing' && (
        <CalendarEmptyState phase={phase} dateShort={dateShort} onAction={onEmptyAction} />
      )}
    </Surface>
  );
}
