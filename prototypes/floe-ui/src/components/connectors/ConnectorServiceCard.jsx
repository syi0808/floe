import { ArrowRight, LockKeyhole } from 'lucide-react';
import { SquircleBlock, SquircleButton } from '../../primitives.jsx';

export function ConnectorServiceCard({ service, onSelect }) {
  const Icon = service.icon;
  return (
    <SquircleButton
      radius={28}
      className="connector-service-card"
      onClick={() => onSelect(service.id)}
      aria-label={`${service.name} · ${service.status} · View service details`}
    >
      <span className="connector-service-heading">
        <SquircleBlock radius={16} className="connector-service-icon" asChild>
          <span>
            <Icon size={27} aria-hidden="true" />
          </span>
        </SquircleBlock>
        {service.readOnly && (
          <span className="connector-read-only">
            <LockKeyhole size={12} aria-hidden="true" /> Read-only
          </span>
        )}
      </span>
      <strong>{service.name}</strong>
      <span className="connector-service-description">{service.description}</span>
      <span className={`connector-service-status ${service.tone}`}>
        <span aria-hidden="true" />
        {service.status}
      </span>
      <span className="connector-service-footer">
        {service.connected ? 'View connection' : 'Set up connection'}
        <ArrowRight size={16} aria-hidden="true" />
      </span>
    </SquircleButton>
  );
}
