import { Check } from 'lucide-react';
import { SQUIRCLE_RADIUS, SquircleBlock, SquircleButton } from '../../primitives.jsx';

export function CheckControl({ checked, label, onClick }) {
  return (
    <SquircleButton
      radius={SQUIRCLE_RADIUS.compact}
      className={checked ? 'check-control checked' : 'check-control'}
      aria-label={label}
      aria-pressed={checked}
      onClick={onClick}
    >
      <SquircleBlock
        radius={SQUIRCLE_RADIUS.micro}
        className={checked ? 'check-visual checked' : 'check-visual'}
      >
        {checked && <Check size={14} strokeWidth={2.4} />}
      </SquircleBlock>
    </SquircleButton>
  );
}
