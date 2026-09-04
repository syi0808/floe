import { useEffect, useRef, useId } from 'react';
import { X } from 'lucide-react';
import { SquircleButton, SquircleSurface } from '../../primitives.jsx';

export function Modal({ title, children, onClose }) {
  const dialog = useRef(null);
  const titleId = useId();
  useEffect(() => {
    const previous = document.activeElement;
    const overflow = document.body.style.overflow;
    document.body.style.overflow = 'hidden';
    const element = dialog.current;
    element.showModal();
    return () => {
      element.close();
      document.body.style.overflow = overflow;
      previous?.focus();
    };
  }, []);
  return (
    <dialog
      ref={dialog}
      className="s1-dialog"
      aria-labelledby={titleId}
      onCancel={(event) => {
        event.preventDefault();
        onClose();
      }}
      onClick={(event) => {
        if (event.target === dialog.current) onClose();
      }}
    >
      <SquircleSurface radius={34} className="s1-modal-border" contentClassName="s1-modal">
        <div className="s1-modal-heading">
          <h2 id={titleId}>{title}</h2>
          <SquircleButton aria-label="Close dialog" className="icon-button" onClick={onClose}>
            <X size={19} />
          </SquircleButton>
        </div>
        {children}
      </SquircleSurface>
    </dialog>
  );
}
