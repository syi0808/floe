import { useEffect, useId, useLayoutEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { Check, ChevronDown } from 'lucide-react';
import { SquircleBlock, SquircleButton, SQUIRCLE_RADIUS } from '../../primitives.jsx';
import { matchingOption, nextEnabled } from './selection-navigation.js';
import './selection-popover.css';

function SelectionPopover({ label, value, options, onChange, disabled, placeholder, description, menu }) {
  const identity = useId();
  const trigger = useRef(null);
  const panel = useRef(null);
  const search = useRef({ text: '', at: 0 });
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(-1);
  const [position, setPosition] = useState({ left: 0, top: 0, width: 240, maxHeight: 320, side: 'bottom' });
  const selected = options.find((option) => option.value === value);

  function close(restoreFocus = true) {
    setOpen(false);
    search.current = { text: '', at: 0 };
    if (restoreFocus) trigger.current?.focus();
  }

  function show(last = false) {
    const selectedIndex = options.findIndex((option) => option.value === value && !option.disabled);
    setActive(!menu && selectedIndex >= 0 ? selectedIndex : nextEnabled(options, last ? 0 : -1, last ? -1 : 1));
    setOpen(true);
  }

  useLayoutEffect(() => {
    if (!open) return;
    function place() {
      const bounds = trigger.current.getBoundingClientRect();
      const below = window.innerHeight - bounds.bottom - 16;
      const above = bounds.top - 16;
      const side = below < Math.min(320, panel.current?.scrollHeight || 320) && above > below ? 'top' : 'bottom';
      const maxHeight = Math.max(44, Math.min(320, side === 'top' ? above : below));
      const width = Math.min(Math.max(bounds.width, 240), window.innerWidth - 24);
      const height = Math.min(panel.current?.scrollHeight || 320, maxHeight);
      setPosition({ left: Math.max(12, Math.min(bounds.left, window.innerWidth - width - 12)), top: side === 'top' ? Math.max(12, bounds.top - height - 6) : bounds.bottom + 6, width, maxHeight, side });
    }
    place();
    panel.current?.focus({ preventScroll: true });
    window.addEventListener('resize', place);
    window.addEventListener('scroll', place, true);
    return () => {
      window.removeEventListener('resize', place);
      window.removeEventListener('scroll', place, true);
    };
  }, [open]);

  useEffect(() => {
    if (!open) return;
    document.getElementById(`${identity}-option-${active}`)?.scrollIntoView({ block: 'nearest' });
  }, [active, open, identity]);

  useEffect(() => {
    if (!open) return;
    function outside(event) {
      if (!panel.current?.contains(event.target) && !trigger.current?.contains(event.target)) close(false);
    }
    document.addEventListener('pointerdown', outside);
    document.addEventListener('focusin', outside);
    return () => {
      document.removeEventListener('pointerdown', outside);
      document.removeEventListener('focusin', outside);
    };
  }, [open]);

  useEffect(() => {
    if (disabled) setOpen(false);
  }, [disabled]);

  function choose(index) {
    if (!options[index] || options[index].disabled) return;
    close();
    onChange(options[index].value);
  }

  function navigate(event) {
    if (['ArrowDown', 'ArrowUp', 'Home', 'End', 'Enter', ' ', 'Escape'].includes(event.key)) event.preventDefault();
    if (event.key === 'ArrowDown') setActive(nextEnabled(options, active, 1));
    else if (event.key === 'ArrowUp') setActive(nextEnabled(options, active < 0 ? 0 : active, -1));
    else if (event.key === 'Home') setActive(nextEnabled(options, -1, 1));
    else if (event.key === 'End') setActive(nextEnabled(options, 0, -1));
    else if (event.key === 'Enter' || event.key === ' ') choose(active);
    else if (event.key === 'Escape') { event.stopPropagation(); close(); }
    else if (event.key === 'Tab') {
      trigger.current?.focus();
      close(false);
    } else if (event.key.length === 1 && !event.metaKey && !event.ctrlKey && !event.altKey) {
      const now = Date.now();
      search.current.text = now - search.current.at < 700 ? search.current.text + event.key : event.key;
      search.current.at = now;
      const query = [...search.current.text].every((character) => character === event.key) ? event.key : search.current.text;
      setActive(matchingOption(options, query, active));
      event.preventDefault();
    }
  }

  return (
    <div className="floe-select">
      {!menu && <span className="floe-select-label" id={`${identity}-label`}>{label}</span>}
      <SquircleButton ref={trigger} className="floe-select-trigger" disabled={disabled} aria-labelledby={!menu ? `${identity}-label ${identity}-value` : undefined} aria-describedby={description ? `${identity}-description` : undefined} aria-haspopup={menu ? 'menu' : 'listbox'} aria-expanded={open} aria-controls={open ? `${identity}-popup` : undefined} onClick={() => open ? close() : show()} onKeyDown={(event) => {
        if (['ArrowDown', 'ArrowUp'].includes(event.key)) { event.preventDefault(); show(event.key === 'ArrowUp'); }
      }}>
        <span id={`${identity}-value`}>{menu ? label : selected?.label || placeholder}</span>
        <ChevronDown size={16} aria-hidden="true" />
      </SquircleButton>
      {description && <span className="floe-select-description" id={`${identity}-description`}>{description}</span>}
      {open && createPortal(
        <div className="floe-selection-position" style={{ left: position.left, top: position.top, width: position.width }}>
          <SquircleBlock radius={SQUIRCLE_RADIUS.control} className="floe-selection-shape" data-side={position.side}>
            <div ref={panel} id={`${identity}-popup`} className="floe-selection-popup" role={menu ? 'menu' : 'listbox'} aria-label={label} aria-activedescendant={active >= 0 ? `${identity}-option-${active}` : undefined} tabIndex={-1} style={{ maxHeight: position.maxHeight }} onKeyDown={navigate}>
              {options.map((option, index) => (
                <SquircleBlock key={option.value} radius={SQUIRCLE_RADIUS.compact} id={`${identity}-option-${index}`} className={`floe-selection-option${active === index ? ' is-active' : ''}`} role={menu ? 'menuitem' : 'option'} aria-selected={!menu ? value === option.value : undefined} aria-disabled={option.disabled || undefined} onPointerMove={(event) => { if (event.pointerType === 'mouse' && !option.disabled) setActive(index); }} onClick={() => choose(index)}>
                  <span><span className="floe-selection-option-label">{option.label}</span>{option.description && <span className="floe-selection-option-description">{option.description}</span>}</span>
                  {!menu && value === option.value && <Check size={16} aria-hidden="true" />}
                </SquircleBlock>
              ))}
              {!options.length && <span className="floe-selection-empty">No options available</span>}
            </div>
          </SquircleBlock>
        </div>, trigger.current?.closest('dialog') || document.body,
      )}
    </div>
  );
}

export function Select({ label, value, options, onChange, disabled = false, placeholder = 'Choose an option', description }) {
  return <SelectionPopover label={label} value={value} options={options} onChange={onChange} disabled={disabled} placeholder={placeholder} description={description} />;
}

export function Dropdown({ label, items, onAction, disabled = false }) {
  return <SelectionPopover label={label} options={items} onChange={onAction} disabled={disabled} menu />;
}
