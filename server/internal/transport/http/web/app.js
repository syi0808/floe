import {budgetOverrideForTarget} from './budget-override.mjs';

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
let state = {providers: {}, clients: [], model_catalog: null};
let selectedProvider = 'openai_compatible';
let pendingProviderCommand = null;
const providerOperationStorage = 'floe-inference-provider-operation';
let providerStorageUnavailable = false;
let providerOperationContext = readProviderOperationContext();
let providerDraftGeneration = 0;
let providerSubmitBusy = false;
let providerRecoveryBusy = false;
let providerOperationStatus = providerStorageUnavailable
  ? {kind: 'storage_unavailable', message: 'Browser session storage is unavailable. No provider operation will be sent until this tab can retain its operation ID.'}
  : providerOperationContext
    ? {kind: 'unresolved', message: initialProviderOperationMessage(providerOperationContext)}
    : {kind: 'idle', message: ''};

function notice(message) { element('notice').textContent = message; }
function readProviderOperationContext() {
  let raw;
  try { raw = sessionStorage.getItem(providerOperationStorage); }
  catch { providerStorageUnavailable = true; return null; }
  if (!raw) return null;
  try {
    const parsed = JSON.parse(raw);
    if (parsed && typeof parsed === 'object' && typeof parsed.operation_id === 'string') return parsed;
    if (typeof parsed === 'string') return {schema: 0, operation_id: parsed, kind: 'unknown', has_api_key: true};
  } catch {
    // Preserve the prior dashboard's raw operation ID without assuming its body.
    if (/^[0-9a-f]{8}-[0-9a-f-]{27,}$/i.test(raw)) return {schema: 0, operation_id: raw, kind: 'unknown', has_api_key: true};
    providerStorageUnavailable = true;
  }
  return null;
}
function initialProviderOperationMessage(context) {
  if (context?.schema !== 1) return 'A prior provider operation ID is saved, but its request details are unavailable. Check its status before starting another provider change.';
  const action = context.kind === 'remove' ? 'removal' : 'update';
  return `A prior provider ${action} for ${context.provider} needs a status check. Raw API keys are never saved in the recovery context; if a retry is needed, re-enter the same key.`;
}
function cloneJSON(value) { return JSON.parse(JSON.stringify(value)); }
function freezeJSON(value) {
  if (value && typeof value === 'object' && !Object.isFrozen(value)) {
    for (const child of Object.values(value)) freezeJSON(child);
    Object.freeze(value);
  }
  return value;
}
function makeProviderOperation(payload, context, draftGeneration = null) {
  return Object.freeze({payload: freezeJSON(cloneJSON(payload)), context, draftGeneration});
}
function contextForProviderPayload(payload, kind = 'update', draftGeneration = null) {
  const context = {
    schema: 1,
    operation_id: payload.operation_id,
    kind,
    provider: payload.provider,
    has_api_key: Boolean(payload.api_key),
  };
  if (kind === 'update') {
    context.base_url = payload.base_url;
    context.purposes = cloneJSON(payload.purposes);
    context.draft_generation = draftGeneration;
  }
  return context;
}
function rememberProviderOperation(payload, kind = 'update', draftGeneration = null) {
  const context = contextForProviderPayload(payload, kind, draftGeneration);
  try { sessionStorage.setItem(providerOperationStorage, JSON.stringify(context)); }
  catch {
    providerStorageUnavailable = true;
    setProviderOperationStatus('storage_unavailable', 'Browser session storage could not retain this operation ID. No provider request was sent; retry after session storage is available.');
    return null;
  }
  providerStorageUnavailable = false;
  providerOperationContext = context;
  pendingProviderCommand = makeProviderOperation(payload, context, draftGeneration);
  return pendingProviderCommand;
}
function providerPayloadFromContext(context, apiKey = '') {
  if (context?.schema !== 1 || typeof context.operation_id !== 'string' || typeof context.provider !== 'string') return null;
  if (context.kind === 'remove') {
    return {operation_id: context.operation_id, provider: context.provider, base_url: '', api_key: '', purposes: {}};
  }
  if (context.kind !== 'update' || typeof context.base_url !== 'string' || !context.purposes || typeof context.purposes !== 'object') return null;
  return {
    operation_id: context.operation_id,
    provider: context.provider,
    base_url: context.base_url,
    api_key: apiKey,
    purposes: cloneJSON(context.purposes),
  };
}
function setProviderOperationStatus(kind, message, needsKey = false) {
  providerOperationStatus = {kind, message, needsKey};
  renderProviderOperationStatus();
}
function renderProviderOperationStatus() {
  const region = element('provider-operation-recovery');
  if (!region) return;
  const context = providerOperationContext;
  region.hidden = !context && !providerOperationStatus.message && !providerStorageUnavailable;
  element('provider-operation-status').textContent = providerOperationStatus.message;
  element('provider-operation-check').hidden = !context || !context.operation_id;
  element('provider-recovery-key-row').hidden = !providerOperationStatus.needsKey;
  element('provider-operation-retry').hidden = !providerOperationStatus.needsKey;
}
function clearProviderOperationContext() {
  try { sessionStorage.removeItem(providerOperationStorage); }
  catch {
    setProviderOperationStatus('storage_unavailable', 'The operation settled, but this tab could not clear its recovery marker. Check the saved operation again before starting another change.');
    return false;
  }
  providerOperationContext = null;
  pendingProviderCommand = null;
  return true;
}
function currentProviderOperation(command) {
  return Boolean(command && providerOperationContext?.operation_id === command.context.operation_id);
}
function preserveProviderDraft(command) {
  return editing && (command.draftGeneration === null || providerDraftGeneration !== command.draftGeneration);
}
async function settleProviderOperation(command, outcome) {
  if (!currentProviderOperation(command)) return;
  const keepDraft = preserveProviderDraft(command);
  if (!clearProviderOperationContext()) return;
  if (outcome === 'completed' && !keepDraft) editing = false;
  if (outcome === 'aborted' && command.context.kind === 'update' && command.draftGeneration !== null) editing = true;
  if (outcome === 'aborted' && (command.context.kind === 'remove' || command.draftGeneration === null) && !keepDraft) editing = false;
  providerSubmitBusy = false;
  if (outcome === 'completed') {
    setProviderOperationStatus('completed', keepDraft
      ? 'The saved provider operation completed. Your newer form edits remain on screen and have not been submitted.'
      : 'The saved provider operation completed. Current provider settings were refreshed.');
  } else {
    setProviderOperationStatus('aborted', 'The saved provider operation was not applied; the prior provider configuration remains active. Review the form and submit again to start a new operation.');
  }
  try { await refresh(); } catch { /* The operation result remains authoritative if dashboard refresh fails. */ }
}
async function postProviderOperation(command) {
  if (!currentProviderOperation(command)) return;
  setProviderOperationStatus('pending', 'The provider request is being submitted. Its operation ID and saved recovery context will remain until the result is confirmed.');
  try {
    const result = await api('provider', command.payload);
    if (!currentProviderOperation(command)) return;
    if (result?.ok === true) {
      await settleProviderOperation(command, 'completed');
    } else {
      setProviderOperationStatus('pending', 'The provider response did not confirm completion. The operation ID is retained; check its status before starting another change.');
    }
  } catch (error) {
    if (!currentProviderOperation(command)) return;
    if (error.message === 'operation_id_reused') {
      setProviderOperationStatus('pending', 'The saved request body did not match the operation already recorded. Checking the original operation status; its ID is retained.');
      await reconcileProviderOperation();
      return;
    }
    if (error.status === 400) {
      if (!clearProviderOperationContext()) return;
      setProviderOperationStatus('rejected', `The provider request was rejected (${error.message}) and was not applied. Correct or review the form, then submit again to create a new operation.`);
      return;
    }
    setProviderOperationStatus('pending', `The provider response was not confirmed (${error.message}). The operation ID is retained; check its status or retry the saved request.`);
  }
}
async function reconcileProviderOperation() {
  const context = providerOperationContext;
  if (!context?.operation_id || providerRecoveryBusy) return;
  providerRecoveryBusy = true;
  setProviderOperationStatus('checking', `Checking saved provider operation ${context.operation_id}.`);
  try {
    const result = await api('inference/recover', {operation_id: context.operation_id});
    if (result?.status === 'no_record') {
      const command = pendingProviderCommand;
      if (command) {
        setProviderOperationStatus('no_record', 'No durable record is visible yet. The earlier request may still arrive; replaying its exact saved payload with the same operation ID.');
        await postProviderOperation(command);
      } else if (context.schema === 1 && !context.has_api_key) {
        const payload = providerPayloadFromContext(context);
        if (payload) {
          const replay = makeProviderOperation(payload, context, null);
          pendingProviderCommand = replay;
          setProviderOperationStatus('no_record', 'No durable record is visible yet. The earlier request may still arrive; replaying the saved non-secret request with the same operation ID.');
          await postProviderOperation(replay);
        } else {
          setProviderOperationStatus('no_record', 'No durable record is visible at this check. The operation ID is retained, but its saved request details cannot be reconstructed.');
        }
      } else if (context.schema === 1 && context.has_api_key) {
        setProviderOperationStatus('no_record', 'No durable record is visible at this check; the earlier request may still arrive. The operation ID and non-secret request details are retained. Re-enter the same API key below to retry those saved settings.', true);
      } else {
        setProviderOperationStatus('no_record', 'No durable record is visible at this check; the earlier request may still arrive. The operation ID is retained, but request details were not saved.');
      }
      return;
    }
    if (result?.recovered === true && !result.category) {
      const command = pendingProviderCommand || makeProviderOperation({}, context, null);
      await settleProviderOperation(command, 'completed');
      return;
    }
    if (result?.recovered === true && result.category === 'unavailable' && result.code === 'credential_store_unavailable') {
      const command = pendingProviderCommand || makeProviderOperation({}, context, null);
      await settleProviderOperation(command, 'aborted');
      return;
    }
    setProviderOperationStatus('pending', 'Recovery did not confirm completion or abortion. The operation ID is retained; check again before starting another change.');
  } catch (error) {
    setProviderOperationStatus('pending', `Recovery could not confirm the provider operation (${error.message}). Its operation ID and recovery context are retained.`);
  } finally {
    providerRecoveryBusy = false;
  }
}
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

function renderModelCatalogSuggestions() {
  const list = element('model-suggestions');
  const projection = state.model_catalog;
  const providers = projection?.catalog?.providers;
  const provider = Array.isArray(providers) ? providers.find((candidate) => candidate.provider_id === selectedProvider) : null;
  const models = Array.isArray(provider?.models) ? provider.models : [];
  const suggestions = models.filter((model) => typeof model?.model_id === 'string' && model.model_id && model.deprecated !== true);
  list.replaceChildren();
  for (const model of suggestions) {
    const option = document.createElement('option');
    option.value = model.model_id;
    if (typeof model.display_name === 'string' && model.display_name) option.label = model.display_name;
    list.append(option);
  }
  for (const purpose of purposes) {
    const input = document.querySelector(`[name="${purpose}_model"]`);
    if (suggestions.length) input.setAttribute('list', 'model-suggestions');
    else input.removeAttribute('list');
  }

  const status = projection?.status;
  const statusNode = element('model-catalog-status');
  if (!status) {
    statusNode.textContent = 'Model suggestions are unavailable. You can enter any model ID.';
    return;
  }
  const source = ({
    file: 'Local catalog', last_good: 'Durable last known good catalog',
    previous: 'Previous rollback snapshot', bootstrap: 'Built-in suggestions',
  })[status.source] || 'Catalog';
  const errors = ({
    missing: 'The local catalog file is missing.',
    invalid_catalog: 'The local catalog file failed validation.',
    stale_revision: 'The local catalog revision is stale.',
    read_failed: 'The local catalog could not be read.',
    persist_failed: 'The accepted catalog could not be saved as the durable last-good snapshot.',
    writer_lock_unavailable: 'The catalog writer lock is unavailable.',
    rollback_recovery_failed: 'A prior catalog rollback needs recovery.',
  })[status.last_error];
  const version = typeof status.version === 'string' ? ` ${status.version}` : '';
  statusNode.textContent = `${source}${version}. ${errors || 'Enter any model ID, including one not listed here.'}`;
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
  if (editing) {
    renderModelCatalogSuggestions();
    renderProviderOperationStatus();
    return;
  }
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
    form.elements[`${purpose}_effort`].value = configured.reasoning_effort || '';
    const capabilities = configured.capabilities || ['chat'];
    form.elements[`${purpose}_structured_output`].checked = capabilities.includes('structured_output');
    form.elements[`${purpose}_tool_proposals`].checked = capabilities.includes('tool_proposals');
    const row = form.querySelector(`[data-class="${purpose}"]`);
    row.classList.toggle('active-route', configured.active === true);
    row.querySelector('.test-class').disabled = !configured.model || configured.available === false;
  }
  renderModelCatalogSuggestions();
  element('remove-provider').disabled = !state.providers?.[selectedProvider];
  renderProviderOperationStatus();
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
  // Go emits RFC3339Nano; normalize fractional precision for browser Date.parse.
  const expires = Date.parse(String(pairing.expires).replace(/(\.\d{3})\d+(?=Z$|[+-]\d{2}:\d{2}$)/, '$1'));
  const remaining = Number.isFinite(expires) ? Math.max(0, Math.ceil((expires - Date.now()) / 1000)) : null;
  if (remaining === 0) {
    target.textContent = 'This request has expired. Return to Floe and start a new pairing request.';
    for (const name of ['approve', 'reject']) { element(name).hidden = true; element(name).disabled = true; }
    return;
  }
  const next = pairing.phase === 'pending'
    ? 'Floe is verifying this connection. Keep the app open; approval becomes available when verification finishes.'
    : pairing.phase === 'local_confirmed'
      ? 'Compare this code with your Floe app. Choose “Approve connection” only when both codes match and you started this request.'
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
  renderProviderOperationStatus();
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
  option.addEventListener('click', () => {
    const nextProvider = option.dataset.provider;
    if (nextProvider === selectedProvider) return;
    if (editing && !confirm('Discard the unsaved provider form edits before switching providers?')) return;
    selectedProvider = nextProvider;
    providerDraftGeneration++;
    editing = false;
    renderProvider();
  });
}
element('provider-form').addEventListener('input', () => { editing = true; providerDraftGeneration++; });
function providerPayloadFromForm(form) {
  const configured = {};
  const baseURL = form.elements.base_url.value;
  for (const purpose of purposes) {
    const model = form.elements[`${purpose}_model`].value.trim();
    const capabilities = ['chat'];
    if (form.elements[`${purpose}_structured_output`].checked) capabilities.push('structured_output');
    if (form.elements[`${purpose}_tool_proposals`].checked) capabilities.push('tool_proposals');
    if (model) {
      configured[purpose] = {model, reasoning_effort: form.elements[`${purpose}_effort`].value, capabilities};
      const targetBudget = budgetOverrideForTarget(state.providers?.[selectedProvider], purpose, model, baseURL);
      if (targetBudget.budgetOverride) configured[purpose].budget_override = targetBudget.budgetOverride;
    }
  }
  return {
    operation_id: crypto.randomUUID(),
    provider: selectedProvider,
    base_url: baseURL,
    api_key: form.elements.api_key.value,
    purposes: configured,
  };
}
async function submitProviderForm() {
  if (providerStorageUnavailable) {
    setProviderOperationStatus('storage_unavailable', 'Browser session storage is unavailable. No provider operation was sent because its ID could not be retained.');
    return;
  }
  if (providerOperationContext) {
    await reconcileProviderOperation();
    return;
  }
  const payload = providerPayloadFromForm(element('provider-form'));
  const command = rememberProviderOperation(payload, 'update', providerDraftGeneration);
  if (!command) return;
  await postProviderOperation(command);
}
element('provider-form').addEventListener('submit', (event) => {
  event.preventDefault();
  if (providerSubmitBusy) return;
  providerSubmitBusy = true;
  action(event.submitter || event.target.querySelector('button[type="submit"], button:not([type])'), async () => {
    try { await submitProviderForm(); }
    finally { providerSubmitBusy = false; }
  });
});
element('provider-operation-check').addEventListener('click', () => action(element('provider-operation-check'), reconcileProviderOperation));
element('provider-operation-retry').addEventListener('click', () => action(element('provider-operation-retry'), async () => {
  const context = providerOperationContext;
  const key = element('provider-recovery-key').value;
  if (!context || context.schema !== 1 || !context.has_api_key || !key) {
    setProviderOperationStatus('no_record', 'Re-enter the API key for the saved provider operation before replaying it.', true);
    return;
  }
  const payload = providerPayloadFromContext(context, key);
  if (!payload) {
    setProviderOperationStatus('no_record', 'The saved request details are unavailable. The operation ID remains retained; do not submit a different request under it.');
    return;
  }
  pendingProviderCommand = makeProviderOperation(payload, context, null);
  element('provider-recovery-key').value = '';
  await postProviderOperation(pendingProviderCommand);
}));
element('remove-provider').addEventListener('click', () => action(element('remove-provider'), async () => {
  if (providerStorageUnavailable) {
    setProviderOperationStatus('storage_unavailable', 'Browser session storage is unavailable. No provider removal was sent because its operation ID could not be retained.');
    return;
  }
  if (providerOperationContext) {
    await reconcileProviderOperation();
    return;
  }
  if (!confirm('Remove this provider configuration and its active class routes?')) return;
  const payload = {operation_id: crypto.randomUUID(), provider: selectedProvider, base_url: '', api_key: '', purposes: {}};
  const command = rememberProviderOperation(payload, 'remove', providerDraftGeneration);
  if (command) await postProviderOperation(command);
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
