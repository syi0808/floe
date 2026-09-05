const element = (id) => document.getElementById(id);
let csrf = '';
let pairing = null;
let unlocked = false;
let polling = false;
let codexPending = false;

function notice(message) { element('notice').textContent = message; }
async function api(path, body) {
  const response = await fetch(`/manage/api/${path}`, {
    method: body === undefined ? 'GET' : 'POST',
    credentials: 'same-origin',
    headers: {'Content-Type': 'application/json', 'X-Floe-CSRF': csrf},
    body: body === undefined ? undefined : JSON.stringify(body),
    signal: AbortSignal.timeout(45000),
  });
  const value = await response.json();
  if (!response.ok) {
    if (response.status === 401) lock();
    throw new Error(value.error?.code || 'request_failed');
  }
  return value;
}
function lock() {
  unlocked = false;
  csrf = '';
  codexPending = false;
  element('codex-link').removeAttribute('href');
  element('codex-link').hidden = true;
  element('dashboard').hidden = true;
  element('login-panel').hidden = false;
}
async function action(button, operation) {
  button.disabled = true;
  try { await operation(); } catch (error) { notice(`Could not complete: ${error.message}. Check the connection and try again.`); }
  finally { button.disabled = false; }
}
function text(tag, value) { const node = document.createElement(tag); node.textContent = value; return node; }
function button(label, operation) {
  const node = text('button', label);
  node.className = 'secondary';
  node.addEventListener('click', () => action(node, operation));
  return node;
}
async function refresh() {
  const state = await api('state');
  csrf = state.csrf;
  unlocked = true;
  element('login-panel').hidden = true;
  element('dashboard').hidden = false;
  element('address').textContent = state.address;
  pairing = state.pairing;
  element('pair-panel').hidden = !pairing;
  element('pair-code').textContent = pairing?.code || '';
  const targets = element('targets');
  targets.replaceChildren();
  if (!Object.keys(state.targets).length) targets.append(text('p', 'No targets yet. Add a local model or an API connection to get started.'));
  for (const [identifier, target] of Object.entries(state.targets)) {
    const card = document.createElement('div');
    card.className = 'target';
    card.append(text('strong', identifier), text('p', `${target.model} · ${target.provider}`), text('p', target.available ? 'Configured · inference not yet verified' : 'Unavailable · check model configuration or credential store'), text('p', target.requires_external_consent ? 'External transfer consent required for every request.' : 'Loopback Ollama · local models only'));
    const actions = document.createElement('div');
    actions.className = 'actions';
    actions.append(button('Edit', async () => {
      const form = element('target-form');
      for (const name of ['provider', 'base_url', 'model']) form.elements[name].value = target[name];
      form.elements.id.value = identifier;
      form.elements.api_key.value = '';
      syncProviderForm();
      form.elements.id.focus();
    }), button('Test connection', async () => {
      if (!confirm(target.requires_external_consent ? 'Send a synthetic test to this provider? No calendar data is sent. Provider charges may apply.' : 'Run a synthetic local-model test? No calendar data is sent.')) return;
      const result = await api('test', {id: identifier, allow_external: target.requires_external_consent});
      notice(`${identifier}: valid test response in ${result.elapsed_ms} ms.`);
    }), button('Delete', async () => {
      if (!confirm(`Delete ${identifier} and its stored credential?`)) return;
      await api('target/delete', {id:identifier}); await refresh();
    }));
    card.append(actions); targets.append(card);
  }
  const clients = element('clients');
  clients.replaceChildren();
  if (!state.clients.length) clients.append(text('p', 'No apps paired yet.'));
  for (const identifier of state.clients) {
    const row = document.createElement('div'); row.className = 'client';
    row.append(text('span', `Floe app · ${identifier.slice(0, 12)}`), button('Revoke', async () => {
      if (!confirm('Revoke this app’s access? It will need to pair again.')) return;
      await api('client/delete', {id:identifier}); await refresh();
    })); clients.append(row);
  }
}
element('login-form').addEventListener('submit', (event) => {
  event.preventDefault();
  action(event.submitter, async () => {
    const token = element('admin-token').value;
    element('admin-token').value = '';
    await api('login', {token}); await refresh(); notice('Node unlocked.');
  });
});
element('target-form').addEventListener('submit', (event) => {
  event.preventDefault();
  action(event.submitter, async () => {
    const input = Object.fromEntries(new FormData(event.target));
    event.target.elements.api_key.value = '';
    await api('target', input); await refresh(); notice('Target saved. Run a synthetic connection test when ready.');
  });
});
function syncProviderForm() {
  const form = element('target-form');
  const provider = form.elements.provider.value;
  const codex = provider === 'codex_oauth';
  element('endpoint-field').hidden = codex;
  element('api-key-field').hidden = codex;
  element('api-key-help').hidden = codex;
  if (codex) form.elements.base_url.value = 'https://chatgpt.com/backend-api/codex';
  form.elements.api_key.value = '';
}
element('target-form').elements.provider.addEventListener('change', (event) => {
  const form = element('target-form');
  const endpoints = {
    ollama: 'http://127.0.0.1:11434',
    openai_compatible: 'https://api.openai.com/v1',
    codex_oauth: 'https://chatgpt.com/backend-api/codex',
  };
  form.elements.base_url.value = endpoints[event.target.value];
  syncProviderForm();
});
element('refresh').onclick = () => action(element('refresh'), refresh);
element('logout').onclick = () => action(element('logout'), async () => { await api('logout', {}); lock(); });
for (const operation of ['approve', 'reject']) element(operation).onclick = () => action(element(operation), async () => {
  if (!pairing) return;
  await api(`pair/${operation}`, {id:pairing.id}); await refresh(); notice(operation === 'approve' ? 'App approved. Return to Floe to finish connecting.' : 'Request rejected.');
});
async function codex(operation) {
  const value = await api(`codex/${operation}`, {});
  codexPending = value.status === 'pending';
  element('codex-state').textContent = `Authentication: ${value.status} · Inference ${value.inference_enabled ? 'enabled' : 'unavailable'}`;
  const link = element('codex-link');
  link.hidden = !value.auth_url;
  if (value.auth_url) link.href = value.auth_url;
  else link.removeAttribute('href');
}
for (const operation of ['login', 'status', 'cancel', 'logout']) element(`codex-${operation}`).onclick = () => action(element(`codex-${operation}`), () => codex(operation));
setInterval(async () => {
  if (!unlocked || polling || document.hidden) return;
  polling = true;
  try {
    await refresh();
    if (codexPending) await codex('status');
  } catch (error) { notice(`Connection unavailable: ${error.message}.`); }
  finally { polling = false; }
}, 5000);
refresh().catch(() => lock());
syncProviderForm();
