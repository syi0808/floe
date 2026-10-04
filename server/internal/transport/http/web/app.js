const element = (id) => document.getElementById(id);
const purposes = ['quick_response', 'everyday_assistance', 'deep_work'];
let csrf = '';
let pairing = null;
let unlocked = false;
let stateReady = false;
let stateRequest = null;
let sessionGeneration = 0;
let loginPending = false;
let polling = false;
let codexPending = false;
let editing = false;
let state = {providers: {}, clients: []};
let selectedProvider = 'openai_compatible';

function notice(message) { element('notice').textContent = message; }
async function api(path, body) {
  const generation = sessionGeneration;
  const response = await fetch(`/manage/api/${path}`, {
    method: body === undefined ? 'GET' : 'POST', credentials: 'same-origin',
    headers: {'Content-Type': 'application/json', 'X-Floe-CSRF': csrf},
    body: body === undefined ? undefined : JSON.stringify(body), signal: AbortSignal.timeout(45000),
  });
  if (response.status === 401 && generation === sessionGeneration) lock();
  const value = await response.json().catch(() => null);
  if (!response.ok) {
    throw Object.assign(new Error(value?.error?.code || 'request_failed'), {status: response.status, operation: path});
  }
  if (value === null) throw Object.assign(new Error('invalid_response'), {status: response.status, operation: path});
  return value;
}
function lock() {
  sessionGeneration++;
  unlocked = false; stateReady = false; csrf = ''; codexPending = false; pairing = null; editing = false;
  element('codex-link').removeAttribute('href'); element('codex-link').hidden = true;
  element('dashboard-content').hidden = true;
  element('dashboard').hidden = true; element('login-panel').hidden = false;
}
function showShell() {
  element('login-panel').hidden = true; element('dashboard').hidden = false;
  element('dashboard-content').hidden = !stateReady;
  element('logout').disabled = !csrf;
}
async function action(button, operation) {
  button.disabled = true;
  try { await operation(); } catch (error) {
    if (!error.stateUnavailable) notice(`${error.operation === 'login' ? 'Sign-in failed' : 'Could not complete'}: ${error.message}. Check the connection and try again.`);
  }
  finally { button.disabled = button.id === 'logout' && !csrf; }
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
  const profile = state.providers?.[selectedProvider] || {purposes: {}};
  form.elements.provider.value = selectedProvider;
  element('provider-heading').replaceChildren(
    text('h2', isCodex ? 'Codex OAuth' : 'OpenAI-compatible API'),
    text('p', isCodex ? 'Experimental Codex-client login; use an API key for the official production path.' : 'Use one compatible endpoint and its server-owned credential.'),
  );
  element('codex-auth').hidden = !isCodex;
  element('api-connection').hidden = isCodex;
  form.elements.base_url.value = profile.base_url || 'https://api.openai.com/v1';
  form.elements.api_key.value = '';
  for (const purpose of purposes) {
    const configured = profile.purposes?.[purpose] || {};
    const model = form.elements[`${purpose}_model`];
    model.value = configured.model || '';
    if (isCodex) model.setAttribute('list', 'codex-models'); else model.removeAttribute('list');
    form.elements[`${purpose}_effort`].value = configured.reasoning_effort || '';
    const capabilities = configured.capabilities || ['chat'];
    form.elements[`${purpose}_structured_output`].checked = capabilities.includes('structured_output');
    form.elements[`${purpose}_tool_proposals`].checked = capabilities.includes('tool_proposals');
    const row = form.querySelector(`[data-class="${purpose}"]`);
    row.classList.toggle('active-route', configured.active === true);
    row.querySelector('.test-class').disabled = !configured.model || configured.available === false;
  }
  element('remove-provider').disabled = !state.providers?.[selectedProvider];
}

function refresh() {
  if (stateRequest?.generation === sessionGeneration) return stateRequest.promise;
  const generation = sessionGeneration;
  const promise = loadState(generation).finally(() => {
    if (stateRequest?.generation === generation) stateRequest = null;
  });
  stateRequest = {generation, promise};
  return promise;
}
async function loadState(generation) {
  try {
    const next = await api('state');
    if (generation !== sessionGeneration) return;
    if (typeof next.csrf !== 'string' || !next.csrf || !Array.isArray(next.clients)
        || !next.providers || typeof next.providers !== 'object' || Array.isArray(next.providers)
        || typeof next.address !== 'string') throw new Error('invalid_state');
    state = next; csrf = next.csrf; unlocked = true; stateReady = true;
    showShell();
    element('state-error').hidden = true;
    element('refresh').textContent = 'Refresh';
    renderState();
  } catch (error) {
    error.stateUnavailable = true;
    if (error.status === 401) {
      if (!unlocked) notice('Your administrator session is unavailable or expired. Sign in to continue.');
    } else if (generation === sessionGeneration) {
      stateReady = false;
      showShell();
      element('state-error').hidden = false;
      element('address').textContent = unlocked ? 'Signed in · dashboard unavailable' : 'Dashboard unavailable';
      element('refresh').textContent = 'Retry loading dashboard';
      const status = error.status ? ` (HTTP ${error.status})` : '';
      element('state-error-message').textContent = `${unlocked ? 'Sign-in succeeded, but dashboard state could not be loaded' : 'Dashboard state could not be loaded'}: ${error.message}${status}. Retry checks the existing session without signing in again.`;
      notice('');
    }
    throw error;
  }
}
function renderPairingGuidance() {
  const target = element('pair-phase');
  if (!pairing) { target.textContent = ''; return; }
  if (pairing.phase === 'activating') {
    target.textContent = 'Activation needs an operator decision. Resume uses only the original retained credential and unexpired proof. Abort succeeds only if Trust confirms activation did not commit. An active client must be explicitly revoked below.';
    return;
  }
  const expires = Date.parse(pairing.expires);
  const remaining = Number.isFinite(expires) ? Math.max(0, Math.ceil((expires - Date.now()) / 1000)) : null;
  if (remaining === 0) {
    target.textContent = 'This request has expired. Return to Floe and start a new pairing request.';
    return;
  }
  const next = pairing.phase === 'pending'
    ? 'First compare both codes, then choose “Codes match” in Floe. Approval becomes available here after the app confirms.'
    : pairing.phase === 'local_confirmed'
      ? 'Floe confirmed the matching code. Choose “Approve connection” here to finish pairing.'
      : 'Waiting for the current pairing state. Refresh to check again.';
  target.textContent = next + (remaining === null ? '' : ` Time remaining: ${Math.floor(remaining / 60)}:${String(remaining % 60).padStart(2, '0')}.`);
}
setInterval(renderPairingGuidance, 1000);

function renderState() {
  element('address').textContent = state.address; pairing = state.pairing;
  element('pair-panel').hidden = !pairing; element('pair-code').textContent = pairing?.code || '';
  const pairingActions = pairing?.allowed_actions || [];
  for (const name of ['approve', 'reject', 'resume', 'abort']) {
    element(name).hidden = !pairingActions.includes(name);
    element(name).disabled = !pairingActions.includes(name);
  }
  renderPairingGuidance();
  element('pair-identity').textContent = pairing
    ? `${pairing.person_id} · ${pairing.device_id}` : '';
  element('pair-fingerprint').textContent = pairing
    ? `Issuer fingerprint: ${pairing.issuer_fingerprint || 'unavailable'} · Producer fingerprint: ${pairing.producer_fingerprint || 'unavailable'}` : '';
  renderProvider();
  const clients = element('clients'); clients.replaceChildren();
  if (!state.clients.length) clients.append(text('p', 'No apps paired yet.'));
  for (const identifier of state.clients) {
    const row = document.createElement('div'); row.className = 'client';
    row.append(text('span', `Floe app · ${identifier.slice(0, 12)}`), button('Revoke app and issuer', async () => {
      if (!confirm('Revoke this app and its trusted issuer? It will need to pair again.')) return;
      await api('client/delete', {id:identifier}); await refresh();
    })); clients.append(row);
  }
}

element('login-form').addEventListener('submit', async (event) => {
  event.preventDefault();
  if (unlocked || loginPending) return;
  loginPending = true;
  try {
    await action(event.submitter || event.target.querySelector('button'), async () => {
      const token = element('admin-token').value; element('admin-token').value = '';
      await api('login', {token});
      sessionGeneration++; unlocked = true; stateReady = false; csrf = '';
      element('state-error').hidden = true;
      element('address').textContent = 'Signed in · loading dashboard';
      showShell(); notice('Signed in. Loading dashboard…');
      await refresh();
      if (stateReady) notice('Node unlocked.');
    });
  } finally { loginPending = false; }
});
for (const option of document.querySelectorAll('.provider-option')) {
  option.addEventListener('click', () => { selectedProvider = option.dataset.provider; editing = false; renderProvider(); });
}
element('provider-form').addEventListener('input', () => { editing = true; });
element('provider-form').addEventListener('submit', (event) => {
  event.preventDefault(); action(event.submitter, async () => {
    const form = event.target; const configured = {};
    for (const purpose of purposes) {
      const model = form.elements[`${purpose}_model`].value.trim();
      const capabilities = ['chat'];
      if (form.elements[`${purpose}_structured_output`].checked) capabilities.push('structured_output');
      if (form.elements[`${purpose}_tool_proposals`].checked) capabilities.push('tool_proposals');
      if (model) configured[purpose] = {model, reasoning_effort: form.elements[`${purpose}_effort`].value, capabilities};
    }
    const input = {provider: selectedProvider, base_url: form.elements.base_url.value, api_key: form.elements.api_key.value, purposes: configured};
    form.elements.api_key.value = '';
    await api('provider', input); editing = false; await refresh(); notice('Provider configuration saved and active purposes updated.');
  });
});
element('remove-provider').addEventListener('click', () => action(element('remove-provider'), async () => {
  if (!confirm('Remove this provider configuration and its active class routes?')) return;
  await api('provider', {provider: selectedProvider, base_url: '', api_key: '', purposes: {}});
  editing = false; await refresh(); notice('Provider configuration removed.');
}));
for (const testButton of document.querySelectorAll('.test-class')) {
  testButton.addEventListener('click', () => action(testButton, async () => {
    const purpose = testButton.closest('.class-grid').dataset.class;
    const configured = state.providers?.[selectedProvider]?.purposes?.[purpose];
    if (!configured || !confirm('Send a synthetic test with no personal or calendar data? Provider usage may apply.')) return;
    const result = await api('test', {id: `managed_${selectedProvider}_${purpose}`});
    notice(`${purpose.replace('_', ' ')}: valid response in ${result.elapsed_ms} ms.`);
  }));
}
element('refresh').onclick = () => action(element('refresh'), refresh);
element('logout').onclick = () => action(element('logout'), async () => { await api('logout', {}); lock(); });
for (const operation of ['approve', 'reject']) element(operation).onclick = () => action(element(operation), async () => {
  if (!pairing) return;
  const body = operation === 'approve'
    ? {schema_version: 1, pairing_id: pairing.id, issuer_fingerprint: pairing.issuer_fingerprint}
    : {schema_version: 1, pairing_id: pairing.id};
  await api(`pair/${operation}`, body); await refresh();
  notice(operation === 'approve' ? 'App approved. Return to Floe to finish connecting.' : 'Request rejected.');
});
for (const operation of ['resume', 'abort']) element(operation).onclick = () => action(element(operation), async () => {
  if (!pairing || !pairing.allowed_actions.includes(operation)) return;
  if (!confirm(operation === 'resume'
    ? 'Resume this exact activation using its original proof and retained credential?'
    : 'Abort this activation only if it has not committed? An already active client remains active.')) return;
  const result = await api('pair/recover', {schema_version: 1, pairing_id: pairing.id,
    issuer_fingerprint: pairing.issuer_fingerprint, action: operation});
  await refresh();
  notice(result.status === 'approved'
    ? (result.credential_delivery === 'repair_required' ? 'Activation already committed, but credential delivery needs repair. Revoke the active client below if recovery is unavailable.' : 'Activation committed. Return to Floe to finish connecting.')
    : 'The uncommitted activation was aborted. Its recovery receipt was retained.');
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
  catch (error) { if (!error.stateUnavailable) notice(`Connection unavailable: ${error.message}.`); }
  finally { polling = false; }
}, 5000);
showShell();
element('address').textContent = 'Checking administrator session…';
refresh().catch(() => {});
renderProvider();
