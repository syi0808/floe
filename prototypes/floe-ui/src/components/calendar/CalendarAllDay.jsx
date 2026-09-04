export function CalendarAllDay({ events, disabled = false, onSelect }) {
  return (
    <div className="s1-all-day">
      <span>All day</span>
      {events.length ? (
        events.map((event, index) => (
          <button
            key={event.id}
            className={index > 0 ? 's1-more-all-day' : undefined}
            disabled={disabled}
            onClick={() => onSelect(event)}
          >
            <span className={'tone-dot ' + event.color} />
            <span className="s1-all-day-title">{event.title}</span>
            <small>{event.caption}</small>
          </button>
        ))
      ) : (
        <span className="s1-muted">—</span>
      )}
    </div>
  );
}
