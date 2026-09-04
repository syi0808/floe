export function SectionHeading({ icon: Icon, eyebrow, title, meta }) {
  return (
    <div className="section-heading">
      <div className="section-icon" aria-hidden="true">
        <Icon size={18} strokeWidth={1.8} />
      </div>
      <div>
        <span>{eyebrow}</span>
        <h2>{title}</h2>
      </div>
      <small>{meta}</small>
    </div>
  );
}
