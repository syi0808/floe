import { ConnectorServiceCard } from './ConnectorServiceCard.jsx';
import './connectors.css';

export function ConnectorList({ services, onSelect }) {
  const connected = services.filter((service) => service.connected);
  const available = services.filter((service) => !service.connected);
  return (
    <div className="connector-list">
      <header className="connector-list-heading">
        <h1>Connections</h1>
        <p>Manage the services that bring context to your day.</p>
      </header>
      <section aria-label="Connected services">
        <h2>
          Connected services <span>{connected.length}</span>
        </h2>
        {connected.length ? (
          <div className="connector-service-grid">
            {connected.map((service) => (
              <ConnectorServiceCard key={service.id} service={service} onSelect={onSelect} />
            ))}
          </div>
        ) : (
          <p className="connector-list-empty">
            No services connected yet. Choose a service below to get started.
          </p>
        )}
      </section>
      {available.length > 0 && (
        <section aria-label="Available services">
          <h2>Available services</h2>
          <div className="connector-service-grid">
            {available.map((service) => (
              <ConnectorServiceCard key={service.id} service={service} onSelect={onSelect} />
            ))}
          </div>
        </section>
      )}
    </div>
  );
}
