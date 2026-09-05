import { useState } from 'react';
import { ExternalLink, Server, SlidersHorizontal } from 'lucide-react';
import { SQUIRCLE_RADIUS, SquircleButton, SquircleSurface } from './primitives.jsx';
import './components/settings/settings.css';

export function SettingsScreen({ notify }) {
  const [connected, setConnected] = useState(true);
  const [address, setAddress] = useState('http://127.0.0.1:8431');

  function save(event) {
    event.preventDefault();
    setConnected(true);
    notify('Server connection saved', 'Floe will use this address for model requests.');
  }

  return (
    <section className="settings-screen">
      <header className="settings-heading">
        <h1>Settings</h1>
        <p>Manage Floe on this device.</p>
      </header>
      <div className="settings-layout">
        <nav className="settings-sections" aria-label="Settings sections">
          <button type="button">
            <SlidersHorizontal size={18} aria-hidden="true" /> General
          </button>
          <button type="button" className="active" aria-current="page">
            <Server size={18} aria-hidden="true" /> Remote server
          </button>
        </nav>
        <div className="settings-content">
          <div className="settings-title-row">
            <div>
              <h2>Remote server connection</h2>
              <p>Connect Floe to the Go server that routes network model requests.</p>
            </div>
            <span className={connected ? 'settings-status connected' : 'settings-status'}>
              {connected ? 'Connected' : 'Not connected'}
            </span>
          </div>
          <SquircleSurface radius={SQUIRCLE_RADIUS.card} className="server-settings-card">
            <form onSubmit={save}>
              <label htmlFor="server-address">Server address</label>
              <input
                id="server-address"
                value={address}
                onChange={(event) => setAddress(event.target.value)}
                spellCheck="false"
                autoCapitalize="none"
              />
              <div className="settings-actions">
                <SquircleButton className="settings-primary" type="submit">Save connection</SquircleButton>
                <SquircleButton onClick={() => notify('Dashboard opened', 'Provider and model settings stay on the server.')}>
                  Open dashboard <ExternalLink size={15} aria-hidden="true" />
                </SquircleButton>
                {connected && (
                  <button className="settings-text-action" type="button" onClick={() => setConnected(false)}>
                    Forget on this device
                  </button>
                )}
              </div>
            </form>
          </SquircleSurface>
          <div className="settings-boundary">
            <strong>Connection boundary</strong>
            <p>Pairing authorizes this app to call registered targets. External data transfer is still approved per request. Provider credentials remain on the server.</p>
          </div>
        </div>
      </div>
    </section>
  );
}
