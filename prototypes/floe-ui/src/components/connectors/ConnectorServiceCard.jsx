import { SquircleBlock, SquircleButton } from '../../primitives.jsx';

export function ConnectorServiceCard({ service, onSelect }) {
  const Icon = service.icon;
  return (
    <SquircleButton
      radius={28}
      className="connector-service-card"
      onClick={() => onSelect(service.id)}
      aria-label={service.name}
    >
      <SquircleBlock radius={16} className="connector-service-icon" asChild>
        <span>
          <Icon size={27} aria-hidden="true" />
        </span>
      </SquircleBlock>
      <strong>{service.name}</strong>
      {service.description && (
        <span className="connector-service-description">{service.description}</span>
      )}
    </SquircleButton>
  );
}
