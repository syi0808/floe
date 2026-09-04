import { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { Check, Info, X } from 'lucide-react';
import './toast.css';

function ToastCard({ toast, index, offset, expanded, paused, onDismiss, onMeasure }) {
  const card = useRef(null);
  const remaining = useRef(4500);
  const [leaving, setLeaving] = useState(false);

  useEffect(() => {
    const observer = new ResizeObserver(([entry]) => {
      onMeasure(toast.id, entry.target.offsetHeight);
    });
    observer.observe(card.current);
    return () => observer.disconnect();
  }, [toast.id, onMeasure]);

  useEffect(() => {
    if (leaving) {
      const timeout = setTimeout(() => onDismiss(toast.id), 180);
      return () => clearTimeout(timeout);
    }
    if (paused) return;
    const started = performance.now();
    const timeout = setTimeout(() => setLeaving(true), remaining.current);
    return () => {
      clearTimeout(timeout);
      remaining.current = Math.max(0, remaining.current - (performance.now() - started));
    };
  }, [paused, leaving, toast.id, onDismiss]);

  const Icon = toast.tone === 'info' ? Info : Check;
  return (
    <li
      ref={card}
      className="floe-toast"
      data-leaving={leaving}
      data-covered={!expanded && index > 0}
      style={{
        '--offset': `${expanded ? offset : index * 9}px`,
        '--scale': expanded ? 1 : 1 - index * 0.045,
        zIndex: 3 - index,
      }}
    >
      <div className={`floe-toast-icon ${toast.tone}`}><Icon size={17} aria-hidden="true" /></div>
      <div className="floe-toast-copy" role="status" aria-atomic="true">
        <strong>{toast.title}</strong>
        <p>{toast.description}</p>
      </div>
      <button type="button" className="floe-toast-close" aria-label={`Dismiss: ${toast.title}`} onClick={() => setLeaving(true)}>
        <X size={14} aria-hidden="true" />
      </button>
    </li>
  );
}

export function ToastViewport({ toasts, onDismiss, heights, onMeasure }) {
  const stack = useRef(null);
  const [hovered, setHovered] = useState(false);
  const [focused, setFocused] = useState(false);
  const [hidden, setHidden] = useState(document.hidden);
  const expanded = hovered || focused;

  useEffect(() => {
    if (!stack.current?.contains(document.activeElement)) setFocused(false);
    if (!toasts.length) setHovered(false);
  }, [toasts]);

  useEffect(() => {
    const onVisibility = () => setHidden(document.hidden);
    document.addEventListener('visibilitychange', onVisibility);
    return () => document.removeEventListener('visibilitychange', onVisibility);
  }, []);

  const ordered = [...toasts].reverse();
  const stackHeight = !toasts.length ? 0 : expanded
    ? ordered.reduce((sum, toast) => sum + (heights[toast.id] || 88) + 10, -10)
    : (heights[ordered[0].id] || 88) + (toasts.length - 1) * 9;
  let offset = 0;
  return createPortal(
    <section aria-label="Notifications" className="floe-toast-region">
      <ol
        ref={stack}
        className="floe-toast-stack"
        style={{ height: stackHeight }}
        onMouseEnter={() => setHovered(true)}
        onMouseLeave={() => setHovered(false)}
        onFocus={() => setFocused(true)}
        onBlur={(event) => { if (!event.currentTarget.contains(event.relatedTarget)) setFocused(false); }}
        onKeyDown={(event) => {
          if (event.key === 'Escape') {
            const closeButtons = event.currentTarget.querySelectorAll('button');
            closeButtons[0]?.focus();
            closeButtons[0]?.click();
          }
        }}
      >
        {ordered.map((toast, index) => {
          const currentOffset = offset;
          offset += (heights[toast.id] || 88) + 10;
          return (
            <ToastCard
              key={toast.id}
              toast={toast}
              index={index}
              offset={currentOffset}
              expanded={expanded}
              paused={expanded || hidden}
              onDismiss={onDismiss}
              onMeasure={onMeasure}
            />
          );
        })}
      </ol>
    </section>,
    document.body,
  );
}
