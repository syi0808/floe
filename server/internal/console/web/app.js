const element = (id) => document.getElementById(id);
const classes = ['fast', 'balanced', 'high_effort'];
let csrf = '';
let pairing = null;
let unlocked = false;
let polling = false;
let codexPending = false;
let editing = false;
let state = {providers: {}, clients: []};
let selectedProvider = 'openai_compatible';

function notice(message) { element('notice').textContent = message; }
async function api(path, body) {
  const response = await fetch(`/manage/api/${path}`, {
    method: body === undefined ? 'GET' : 'POST', credentials: 'same-origin',
    headers: {'Content-Type': 'application/json', 'X-Floe-CSRF': csrf},
    body: body === undefined ? undefined : JSON.stringify(body), signal: AbortSignal.timeout(45000),
  });
  const value = await response.json();
  if (!response.ok) {
    if (response.status === 401) lock();
    throw new Error(value.error?.code || 'request_failed');
  }
  return value;
}
function lock() {
  unlocked = false; csrf = ''; codexPending = false;
  element('codex-link').removeAttribute('href'); element('codex-link').hidden = true;
  element('dashboard').hidden = true; element('login-panel').hidden = false;
}
async function action(button, operation) {
  button.disabled = true;
  try { await operation(); } catch (error) { notice(`Could not complete: ${error.message}. Check the connection and try again.`); }
  finally { button.disabled = false; }
}
function text(tag, value) { const node = document.createElement(tag); node.textContent = value; return node; }
function button(label, operation) {
  const node = text('button', label); node.className = 'secondary';
  node.addEventListener('click', () => action(node, operation)); return node;
}

function renderProvider() {
  const isClaude = selectedProvider === 'claude_oauth';
  const isCodex = selectedProvider === 'codex_oauth';
  element('claude-panel').hidden = !isClaude;
  element('provider-form').hidden = isClaude;
  for (const option of document.querySelectorAll('.provider-option')) {
    const active = option.dataset.provider === selectedProvider;
    option.classList.toggle('active', active);
    option.setAttribute('aria-pressed', active ? 'true' : 'false');
  }
  if (isClaude) return;
  const form = element('provider-form');
  const profile = state.providers?.[selectedProvider] || {classes: {}};
  form.elements.provider.value = selectedProvider;
  element('provider-heading').replaceChildren(
    text('h2', isCodex ? 'Codex OAuth' : 'OpenAI-compatible API'),
    text('p', isCodex ? 'Experimental Codex-client login; use an API key for the official production path.' : 'Use one compatible endpoint and its server-owned credential.'),
  );
  element('codex-auth').hidden = !isCodex;
  element('api-connection').hidden = isCodex;
  form.elements.base_url.value = profile.base_url || 'https://api.openai.com/v1';
  form.elements.api_key.value = '';
  for (const inferenceClass of classes) {
    const configured = profile.classes?.[inferenceClass] || {};
    const model = form.elements[`${inferenceClass}_model`];
    model.value = configured.model || '';
    if (isCodex) model.setAttribute('list', 'codex-models'); else model.removeAttribute('list');
    form.elements[`${inferenceClass}_effort`].value = configured.reasoning_effort || '';
    const row = form.querySelector(`[data-class="${inferenceClass}"]`);
    row.classList.toggle('active-route', configured.active === true);
    row.querySelector('.test-class').disabled = !configured.model || configured.available === false;
  }
  element('remove-provider').disabled = !state.providers?.[selectedProvider];
}

async function refresh() {
  state = await api('state'); csrf = state.csrf; unlocked = true;
  element('login-panel').hidden = true; element('dashboard').hidden = false;
  element('address').textContent = state.address; pairing = state.pairing;
  element('pair-panel').hidden = !pairing; element('pair-code').textContent = pairing?.code || '';
  renderProvider();
  const clients = element('clients'); clients.replaceChildren();
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
  event.preventDefault(); action(event.submitter, async () => {
    const token = element('admin-token').value; element('admin-token').value = '';
    await api('login', {token}); await refresh(); notice('Node unlocked.');
  });
});
for (const option of document.querySelectorAll('.provider-option')) {
  option.addEventListener('click', () => { selectedProvider = option.dataset.provider; editing = false; renderProvider(); });
}
element('provider-form').addEventListener('input', () => { editing = true; });
element('provider-form').addEventListener('submit', (event) => {
  event.preventDefault(); action(event.submitter, async () => {
    const form = event.target; const configured = {};
    for (const inferenceClass of classes) {
      const model = form.elements[`${inferenceClass}_model`].value.trim();
      if (model) configured[inferenceClass] = {model, reasoning_effort: form.elements[`${inferenceClass}_effort`].value};
    }
    const input = {provider: selectedProvider, base_url: form.elements.base_url.value, api_key: form.elements.api_key.value, classes: configured};
    form.elements.api_key.value = '';
    await api('provider', input); editing = false; await refresh(); notice('Provider configuration saved and active classes updated.');
  });
});
element('remove-provider').addEventListener('click', () => action(element('remove-provider'), async () => {
  if (!confirm('Remove this provider configuration and its active class routes?')) return;
  await api('provider', {provider: selectedProvider, base_url: '', api_key: '', classes: {}});
  editing = false; await refresh(); notice('Provider configuration removed.');
}));
for (const testButton of document.querySelectorAll('.test-class')) {
  testButton.addEventListener('click', () => action(testButton, async () => {
    const inferenceClass = testButton.closest('.class-grid').dataset.class;
    const configured = state.providers?.[selectedProvider]?.classes?.[inferenceClass];
    if (!configured || !confirm('Send a synthetic test with no personal or calendar data? Provider usage may apply.')) return;
    const result = await api('test', {id: `managed_${selectedProvider}_${inferenceClass}`, allow_external: true});
    notice(`${inferenceClass.replace('_', ' ')}: valid response in ${result.elapsed_ms} ms.`);
  }));
}
element('refresh').onclick = () => action(element('refresh'), refresh);
element('logout').onclick = () => action(element('logout'), async () => { await api('logout', {}); lock(); });
for (const operation of ['approve', 'reject']) element(operation).onclick = () => action(element(operation), async () => {
  if (!pairing) return;
  await api(`pair/${operation}`, {id:pairing.id}); await refresh();
  notice(operation === 'approve' ? 'App approved. Return to Floe to finish connecting.' : 'Request rejected.');
});
async function codex(operation) {
  const value = await api(`codex/${operation}`, {}); codexPending = value.status === 'pending';
  element('codex-state').textContent = `Authentication: ${value.status} · Inference ${value.inference_enabled ? 'enabled' : 'unavailable'}`;
  const link = element('codex-link'); link.hidden = !value.auth_url;
  if (value.auth_url) link.href = value.auth_url; else link.removeAttribute('href');
}
for (const operation of ['login', 'status', 'cancel', 'logout']) element(`codex-${operation}`).onclick = () => action(element(`codex-${operation}`), () => codex(operation));
setInterval(async () => {
  if (!unlocked || polling || editing || document.hidden) return;
  polling = true;
  try { await refresh(); if (codexPending) await codex('status'); }
  catch (error) { notice(`Connection unavailable: ${error.message}.`); }
  finally { polling = false; }
}, 5000);
refresh().catch(() => lock());
renderProvider();
