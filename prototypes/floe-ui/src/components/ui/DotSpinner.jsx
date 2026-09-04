export function DotSpinner({ label = 'Loading…' }) {
  return (
    <span className="floe-dot-spinner" role="status">
      <span className="sr-only">{label}</span>
      <span className="floe-dot-spinner-ring" aria-hidden="true">
        {Array.from({ length: 8 }, (_, index) => (
          <span
            key={index}
            style={{
              transform: `rotate(${index * 45}deg) translateY(-12px)`,
              opacity: (index + 1) / 8,
            }}
          />
        ))}
      </span>
    </span>
  );
}
