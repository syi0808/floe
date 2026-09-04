import { SQUIRCLE_RADIUS, SquircleBlock } from '../../primitives.jsx';

export function ProgressBar({ value, tone }) {
  return (
    <div
      className={`progress-bar ${tone}`}
      role="progressbar"
      aria-valuemin="0"
      aria-valuemax="100"
      aria-valuenow={value}
    >
      <SquircleBlock
        radius={SQUIRCLE_RADIUS.micro}
        className="progress-fill"
        style={{ width: `${Math.max(value, 2)}%` }}
      />
    </div>
  );
}
