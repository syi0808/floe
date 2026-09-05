import { Checkbox } from './SelectionControl.jsx';

export function CheckControl({ checked, label, onClick }) {
  return <Checkbox compact checked={checked} label={label} onChange={onClick} />;
}
