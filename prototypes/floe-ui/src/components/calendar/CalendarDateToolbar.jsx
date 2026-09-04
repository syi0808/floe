import { ChevronLeft, ChevronRight, RefreshCw } from 'lucide-react';
import { SquircleButton } from '../../primitives.jsx';

export function CalendarDateToolbar({
  dateLabel,
  disabled,
  refreshDisabled,
  showRefresh,
  onPrevious,
  onNext,
  onToday,
  onRefresh,
}) {
  return (
    <div className="s1-day-toolbar">
      <div className="s1-date-controls">
        <SquircleButton
          className="icon-button"
          aria-label="Previous day"
          disabled={disabled}
          onClick={() => onPrevious()}
        >
          <ChevronLeft size={18} />
        </SquircleButton>
        <SquircleButton
          className="icon-button"
          aria-label="Next day"
          disabled={disabled}
          onClick={() => onNext()}
        >
          <ChevronRight size={18} />
        </SquircleButton>
        <h2>{dateLabel}</h2>
        <button className="quiet-action" onClick={() => onToday()} disabled={disabled}>
          Today
        </button>
      </div>
      {showRefresh && (
        <SquircleButton
          className="s1-refresh"
          aria-label="Refresh selected date"
          disabled={refreshDisabled}
          onClick={() => onRefresh()}
        >
          <RefreshCw size={15} />
        </SquircleButton>
      )}
    </div>
  );
}
