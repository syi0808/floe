const element = (id) => document.getElementById(id);
const classes = ['fast', 'balanced', 'high_effort'];
let csrf = '';
let pairing = null;
let unlocked = false;
let polling = false;
let codexPending = false;
let gmailPending = false;
let drivePending = false;
let microsoftMailPending = false;
let calendarPending = false;
let microsoftCalendarPending = false;
let editing = false;
let state = {providers: {}, connectors: {}, clients: []};
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
  unlocked = false; csrf = ''; codexPending = false; gmailPending = false; drivePending = false; microsoftMailPending = false; calendarPending = false; microsoftCalendarPending = false;
  element('codex-link').removeAttribute('href'); element('codex-link').hidden = true;
  element('gmail-link').removeAttribute('href'); element('gmail-link').hidden = true;
  element('drive-link').removeAttribute('href'); element('drive-link').hidden = true;
  element('microsoft-mail-link').removeAttribute('href'); element('microsoft-mail-link').hidden = true;
  element('calendar-link').removeAttribute('href'); element('calendar-link').hidden = true;
  element('microsoft-calendar-link').removeAttribute('href'); element('microsoft-calendar-link').hidden = true;
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
  element('github-state').textContent = state.connectors?.github?.configured ? 'Configured · selected repository read only' : 'Not configured';
  element('slack-state').textContent = state.connectors?.slack?.configured ? 'Configured · selected conversation read only' : 'Not configured';
  element('drive-state').textContent = state.connectors?.google_drive?.configured ? 'Configured · selected folder ephemeral read' : 'Not configured';
  element('calendar-state').textContent = state.connectors?.google_calendar?.configured ? 'Configured · selected calendar ephemeral read' : 'Not configured';
  element('microsoft-calendar-state').textContent = state.connectors?.microsoft_calendar?.configured ? 'Configured · selected calendar ephemeral read' : 'Not configured';
  element('home-state').textContent = state.connectors?.home_assistant?.configured ? 'Configured · selected state entities read only' : 'Not configured';
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
async function gmail(operation) {
  const value = await api(`gmail/${operation}`, {}); gmailPending = value.status === 'pending';
  const connection = value.connection?.connection?.state;
  element('gmail-state').textContent = `Authentication: ${value.status || 'connected'} · Context: ${connection || (operation === 'sync' ? 'synced' : 'not inspected')} · Scope: Gmail read-only`;
  const link = element('gmail-link'); link.hidden = !value.auth_url;
  if (value.auth_url) link.href = value.auth_url; else link.removeAttribute('href');
}
for (const operation of ['login', 'status', 'cancel', 'logout']) element(`gmail-${operation}`).onclick = () => action(element(`gmail-${operation}`), () => gmail(operation));
element('gmail-sync').onclick = () => action(element('gmail-sync'), async () => { await gmail('sync'); await gmail('status'); });
async function microsoftMail(operation) {
  const value = await api(`microsoft-mail/${operation}`, {}); microsoftMailPending = value.status === 'pending';
  element('microsoft-mail-state').textContent = `Authentication: ${value.status} · Scope: Mail.Read`;
  const link = element('microsoft-mail-link'); link.hidden = !value.auth_url;
  if (value.auth_url) link.href = value.auth_url; else link.removeAttribute('href');
}
for (const operation of ['login', 'status', 'cancel', 'logout']) element(`microsoft-mail-${operation}`).onclick = () => action(element(`microsoft-mail-${operation}`), () => microsoftMail(operation));
for (const form of [element('github-form'), element('slack-form'), element('drive-form'), element('calendar-form'), element('microsoft-calendar-form'), element('home-form')]) form.addEventListener('input', () => { editing = true; });
element('github-form').addEventListener('submit', (event) => {
  event.preventDefault(); action(event.submitter, async () => {
    const form = event.target;
    await api('connector/github', {enabled: true, owner: form.elements.owner.value.trim(), repository: form.elements.repository.value.trim(), token: form.elements.token.value});
    form.elements.token.value = ''; editing = false; await refresh(); notice('GitHub source configuration saved.');
  });
});
element('home-form').addEventListener('submit', (event) => {
  event.preventDefault(); action(event.submitter, async () => {
    const form = event.target;
    const entities = form.elements.entities.value.split(',').map((value) => value.trim()).filter(Boolean);
    await api('connector/home-assistant', {enabled: true, base_url: form.elements.base_url.value, entities, token: form.elements.token.value});
    form.elements.token.value = ''; editing = false; await refresh(); notice('Home Assistant source configuration saved.');
  });
});
element('slack-form').addEventListener('submit', (event) => {
  event.preventDefault(); action(event.submitter, async () => {
    const form = event.target;
    await api('connector/slack', {enabled: true, channel: form.elements.channel.value.trim(), thread: form.elements.thread.value.trim(), token: form.elements.token.value});
    form.elements.token.value = ''; editing = false; await refresh(); notice('Slack source configuration saved.');
  });
});
element('drive-form').addEventListener('submit', (event) => {
  event.preventDefault(); action(event.submitter, async () => {
    const form = event.target;
    await api('connector/google-drive', {enabled: true, folder_id: form.elements.folder_id.value.trim()});
    editing = false; await refresh(); notice('Google Drive folder selection saved.');
  });
});
element('calendar-form').addEventListener('submit', (event) => {
  event.preventDefault(); action(event.submitter, async () => {
    const form = event.target;
    await api('connector/google-calendar', {enabled: true, calendar_id: form.elements.calendar_id.value.trim()});
    editing = false; await refresh(); notice('Google Calendar selection saved.');
  });
});
element('microsoft-calendar-form').addEventListener('submit', (event) => {
  event.preventDefault(); action(event.submitter, async () => {
    const form = event.target;
    await api('connector/microsoft-calendar', {enabled: true, calendar_id: form.elements.calendar_id.value.trim()});
    editing = false; await refresh(); notice('Microsoft Calendar selection saved.');
  });
});
element('github-disconnect').onclick = () => action(element('github-disconnect'), async () => {
  if (!confirm('Disconnect GitHub and delete its stored token?')) return;
  await api('connector/github', {enabled: false, owner: '', repository: '', token: ''}); editing = false; await refresh(); notice('GitHub source disconnected.');
});
element('home-disconnect').onclick = () => action(element('home-disconnect'), async () => {
  if (!confirm('Disconnect Home Assistant and delete its stored token?')) return;
  await api('connector/home-assistant', {enabled: false, base_url: '', entities: [], token: ''}); editing = false; await refresh(); notice('Home Assistant source disconnected.');
});
element('slack-disconnect').onclick = () => action(element('slack-disconnect'), async () => {
  if (!confirm('Disconnect Slack and delete its stored token?')) return;
  await api('connector/slack', {enabled: false, channel: '', thread: '', token: ''}); editing = false; await refresh(); notice('Slack source disconnected.');
});
element('drive-disconnect').onclick = () => action(element('drive-disconnect'), async () => {
  if (!confirm('Remove the selected Drive folder from Floe?')) return;
  await api('connector/google-drive', {enabled: false, folder_id: ''}); editing = false; await refresh(); notice('Google Drive folder removed.');
});
element('calendar-disconnect').onclick = () => action(element('calendar-disconnect'), async () => {
  if (!confirm('Remove the selected Google Calendar from Floe?')) return;
  await api('connector/google-calendar', {enabled: false, calendar_id: ''}); editing = false; await refresh(); notice('Google Calendar selection removed.');
});
element('microsoft-calendar-disconnect').onclick = () => action(element('microsoft-calendar-disconnect'), async () => {
  if (!confirm('Remove the selected Microsoft Calendar from Floe?')) return;
  await api('connector/microsoft-calendar', {enabled: false, calendar_id: ''}); editing = false; await refresh(); notice('Microsoft Calendar selection removed.');
});
async function drive(operation) {
  const value = await api(`drive/${operation}`, {}); drivePending = value.status === 'pending';
  element('drive-auth-state').textContent = `Authentication: ${value.status} · Scope: Drive read-only`;
  const link = element('drive-link'); link.hidden = !value.auth_url;
  if (value.auth_url) link.href = value.auth_url; else link.removeAttribute('href');
}
for (const operation of ['login', 'status', 'cancel', 'logout']) element(`drive-${operation}`).onclick = () => action(element(`drive-${operation}`), () => drive(operation));
async function calendar(operation) {
  const value = await api(`calendar/${operation}`, {}); calendarPending = value.status === 'pending';
  element('calendar-auth-state').textContent = `Authentication: ${value.status} · Scope: Calendar read-only`;
  const link = element('calendar-link'); link.hidden = !value.auth_url;
  if (value.auth_url) link.href = value.auth_url; else link.removeAttribute('href');
}
for (const operation of ['login', 'status', 'cancel', 'logout']) element(`calendar-${operation}`).onclick = () => action(element(`calendar-${operation}`), () => calendar(operation));
async function microsoftCalendar(operation) {
  const value = await api(`microsoft-calendar/${operation}`, {}); microsoftCalendarPending = value.status === 'pending';
  element('microsoft-calendar-auth-state').textContent = `Authentication: ${value.status} · Scope: Calendars.Read`;
  const link = element('microsoft-calendar-link'); link.hidden = !value.auth_url;
  if (value.auth_url) link.href = value.auth_url; else link.removeAttribute('href');
}
for (const operation of ['login', 'status', 'cancel', 'logout']) element(`microsoft-calendar-${operation}`).onclick = () => action(element(`microsoft-calendar-${operation}`), () => microsoftCalendar(operation));
setInterval(async () => {
  if (!unlocked || polling || editing || document.hidden) return;
  polling = true;
  try { await refresh(); if (codexPending) await codex('status'); if (gmailPending) await gmail('status'); if (drivePending) await drive('status'); if (microsoftMailPending) await microsoftMail('status'); if (calendarPending) await calendar('status'); if (microsoftCalendarPending) await microsoftCalendar('status'); }
  catch (error) { notice(`Connection unavailable: ${error.message}.`); }
  finally { polling = false; }
}, 5000);
refresh().catch(() => lock());
renderProvider();
