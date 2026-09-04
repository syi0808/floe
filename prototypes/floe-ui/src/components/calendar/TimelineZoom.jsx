import { ZoomIn, ZoomOut } from 'lucide-react';

export function TimelineZoom({ value, onChange }) {
  return (
    <div className="s1-timeline-tools">
      <ZoomOut size={15} aria-hidden="true" />
      <input
        className="s1-zoom-slider"
        type="range"
        min="1"
        max="12"
        step="1"
        aria-label="Timeline zoom"
        aria-valuetext={`${value} times magnification`}
        value={value}
        style={{ '--zoom-progress': `${((value - 1) / 11) * 100}%` }}
        onChange={(event) => onChange(Number(event.target.value))}
      />
      <ZoomIn size={15} aria-hidden="true" />
      <span className="s1-zoom-value" aria-hidden="true">
        {value}×
      </span>
    </div>
  );
}
