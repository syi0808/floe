export function ValidationItem({ value, icon: Icon, label }) {
  return (
    <div className="validation-item">
      {value ? <strong>{value}</strong> : <Icon size={18} aria-hidden="true" />}
      <span>{label}</span>
    </div>
  );
}
