import { useEffect, useRef, useId, useState } from 'react';
import { X } from 'lucide-react';
import { SquircleButton, SquircleSurface } from '../../primitives.jsx';

export function Modal({ title, children, onClose }) {
  const dialog = useRef(null);
  const titleId = useId();
  const [closing, setClosing] = useState(false);
  const closeAction = useRef(null);
  const closeTimer = useRef(null);

  function finishClose() {
    const action = closeAction.current;
    if (!action) return;
    clearTimeout(closeTimer.current);
    closeAction.current = null;
    action();
  }

  function close(afterClose) {
    if (closeAction.current) return;
    closeAction.current = typeof afterClose === 'function' ? afterClose : onClose;
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
      finishClose();
      return;
    }
    setClosing(true);
    closeTimer.current = setTimeout(finishClose, 240);
  }

  useEffect(() => {
    const previous = document.activeElement;
    const overflow = document.body.style.overflow;
    document.body.style.overflow = 'hidden';
    const element = dialog.current;
    element.showModal();
    return () => {
      clearTimeout(closeTimer.current);
      element.close();
      document.body.style.overflow = overflow;
      previous?.focus();
    };
  }, []);
  return (
    <dialog
      ref={dialog}
      className="s1-dialog"
      data-closing={closing || undefined}
      aria-labelledby={titleId}
      onCancel={(event) => {
        event.preventDefault();
        close();
      }}
      onClickCapture={(event) => {
        if (closeAction.current) {
          event.preventDefault();
          event.stopPropagation();
        }
      }}
      onClick={(event) => {
        if (event.target === dialog.current) close();
      }}
      onAnimationEnd={(event) => {
        if (event.target === dialog.current && event.animationName === 's1-dialog-exit') {
          finishClose();
        }
      }}
    >
      <SquircleSurface radius={34} className="s1-modal-border" contentClassName="s1-modal">
        <div className="s1-modal-heading">
          <h2 id={titleId}>{title}</h2>
          <SquircleButton aria-label="Close dialog" className="icon-button" onClick={() => close()}>
            <X size={19} />
          </SquircleButton>
        </div>
        {typeof children === 'function' ? children({ close }) : children}
      </SquircleSurface>
    </dialog>
  );
}
