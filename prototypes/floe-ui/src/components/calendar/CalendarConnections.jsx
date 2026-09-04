import {
  ArrowRight,
  CalendarDays,
  Info,
  LockKeyhole,
  ShieldCheck,
  Unplug,
  WifiOff,
} from 'lucide-react';
import { SquircleBlock, SquircleButton } from '../../primitives.jsx';
import { CalendarSurface as Surface } from './CalendarSurface.jsx';

export function CalendarConnections({
  hasConnection,
  phase,
  statusTone,
  statusLabel,
  readDates,
  calendars,
  onOpenDialog,
  onRefresh,
}) {
  return (
    <>
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
            Bring all calendars on this Mac into one day. Floe reads their events; it never creates,
            edits, or deletes anything in Calendar.
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
                  ? onOpenDialog('settings')
                  : hasConnection
                    ? onRefresh()
                    : onOpenDialog('disclosure')
              }
            >
              {hasConnection ? 'Refresh all calendars' : 'Connect Calendar'}
              <ArrowRight size={16} />
            </SquircleButton>
            <SquircleButton className="secondary-button" onClick={() => onOpenDialog('settings')}>
              Manage access
            </SquircleButton>
          </div>
          {hasConnection && (
            <SquircleButton
              className="s1-danger-link s1-quiet-action"
              onClick={() => onOpenDialog('disconnect')}
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
            <div className="s1-note s1-connection-note">
              <Info size={16} aria-hidden="true" />
              <p>
                macOS calls this “Full Access,” even for reading. That OS permission does not enable
                writes in Floe.
              </p>
            </div>
          </Surface>
          <Surface>
            <WifiOff size={23} className="s1-violet" aria-hidden="true" />
            <h2>What happens offline?</h2>
            <p className="s1-body-copy">
              Your last saved events remain visible, with their collection time. Revoking permission
              stops new reads; it does not erase the saved copy.
            </p>
          </Surface>
        </div>
      </div>
    </>
  );
}
