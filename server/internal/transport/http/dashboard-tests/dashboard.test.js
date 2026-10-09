import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import {JSDOM} from 'jsdom';
import {budgetOverrideForTarget} from '../web/budget-override.mjs';

const directory = new URL('.', import.meta.url);
const html = readFileSync(new URL('../web/index.html', directory), 'utf8');
const app = readFileSync(new URL('../web/app.js', directory), 'utf8');
const budgetOverrideImport = "import {budgetOverrideForTarget} from './budget-override.mjs';";
const appWithoutBudgetOverrideImport = app.replace(budgetOverrideImport, '');
assert.notEqual(appWithoutBudgetOverrideImport, app, 'dashboard module must import the budget override helper');
const origin = 'http://127.0.0.1:18431';

function jsonResponse(status, body) {
  return {
    status,
    ok: status >= 200 && status < 300,
    json: async () => body,
  };
}

function deferred() {
  let resolve;
  let reject;
  const promise = new Promise((done, fail) => { resolve = done; reject = fail; });
  return {promise, resolve, reject};
}

function makeDashboard(initialExpectations, {now = Date.parse('2026-10-07T12:00:00.000Z'), storageSeed = {}, confirm = () => true} = {}) {
  const dom = new JSDOM(html, {
    url: `${origin}/manage/`,
    runScripts: 'outside-only',
  });
  const {window} = dom;
  for (const [key, value] of Object.entries(storageSeed)) window.sessionStorage.setItem(key, value);
  const expectations = [...initialExpectations];
  const requests = [];
  const unexpected = [];
  let currentTime = now;

  Object.defineProperty(window.document, 'hidden', {configurable: true, get: () => false});
  Object.defineProperty(window.Date, 'now', {configurable: true, value: () => currentTime});
  window.setInterval = () => 0;
  window.clearInterval = () => {};
  window.confirm = confirm;
  let operationSequence = 0;
  Object.defineProperty(window.crypto, 'randomUUID', {
    configurable: true,
    value: () => `00000000-0000-4000-8000-${String(++operationSequence).padStart(12, '0')}`,
  });
  Object.defineProperty(window.AbortSignal, 'timeout', {
    configurable: true,
    value: () => new window.AbortController().signal,
  });
  window.fetch = (input, init = {}) => {
    const request = {
      path: String(input),
      method: init.method || 'GET',
      credentials: init.credentials,
      headers: {...init.headers},
      body: init.body,
    };
    requests.push(request);
    const expected = expectations.shift();
    if (!expected) {
      unexpected.push(request);
      return Promise.reject(new Error(`Unexpected dashboard request: ${request.method} ${request.path}`));
    }
    try {
      assert.equal(request.path, expected.path);
      assert.equal(request.method, expected.method);
      return expected.response;
    } catch (error) {
      unexpected.push({request, error: error.message});
      return Promise.reject(error);
    }
  };
  window.budgetOverrideForTarget = budgetOverrideForTarget;
  window.eval(`const budgetOverrideForTarget = window.budgetOverrideForTarget;\n${appWithoutBudgetOverrideImport}`);

  return {
    dom,
    window,
    requests,
    expectations,
    unexpected,
    expect(path, method, response) {
      expectations.push({path: `/manage/api/${path}`, method, response: Promise.resolve(response)});
    },
    expectDeferred(path, method) {
      const response = deferred();
      expectations.push({path: `/manage/api/${path}`, method, response: response.promise});
      return response;
    },
    async settle() {
      for (let i = 0; i < 16; i++) await Promise.resolve();
    },
    assertDrained() {
      assert.deepEqual(unexpected, []);
      assert.deepEqual(expectations, []);
    },
    close() { dom.window.close(); },
  };
}

function state({csrf = 'csrf-current-session', clients = [], pairing = null, providers = {}, model_catalog = null} = {}) {
  return {csrf, clients, providers, address: `${origin}`, pairing, model_catalog};
}

function pairing({
  id = '11111111-1111-4111-8111-111111111111',
  phase = 'local_confirmed',
  allowed_actions = ['approve', 'reject'],
  issuer_fingerprint = 'a'.repeat(64),
  expires = '2026-10-07T12:05:00.000Z',
} = {}) {
  return {
    id,
    code: 'A1B2C3D4',
    person_id: '22222222-2222-4222-8222-222222222222',
    device_id: 'synthetic-device',
    phase,
    allowed_actions,
    issuer_fingerprint,
    producer_fingerprint: 'b'.repeat(64),
    expires,
  };
}

function element(window, id) {
  return window.document.getElementById(id);
}

test('catalog suggestions preserve a removed selected model and leave entry open', async (t) => {
  const env = makeDashboard([
    {path: '/manage/api/state', method: 'GET', response: Promise.resolve(jsonResponse(200, state({
      providers: {codex_oauth: {purposes: {
        quick_response: {model: 'removed-model', capabilities: ['chat']},
        everyday_assistance: {model: 'retired-model', capabilities: ['chat']},
      }}},
      model_catalog: {
        catalog: {providers: [{provider_id: 'codex_oauth', models: [
          {model_id: 'gpt-5.5', display_name: 'Suggested model'},
          {model_id: 'retired-model', deprecated: true},
        ]}]},
        status: {source: 'previous', version: 'revision-2', last_error: 'invalid_catalog'},
      },
    })))} ,
  ]);
  t.after(() => env.close());
  await env.settle();
  env.window.document.querySelector('.provider-option[data-provider="codex_oauth"]').click();

  const modelInput = env.window.document.querySelector('[name="quick_response_model"]');
  const deprecatedInput = env.window.document.querySelector('[name="everyday_assistance_model"]');
  assert.equal(modelInput.value, 'removed-model');
  assert.equal(deprecatedInput.value, 'retired-model');
  assert.equal(modelInput.getAttribute('list'), 'model-suggestions');
  const choices = [...element(env.window, 'model-suggestions').options].map((option) => option.value);
  assert.deepEqual(choices, ['gpt-5.5']);
  modelInput.value = 'operator-entered-model';
  assert.equal(modelInput.value, 'operator-entered-model');
  assert.match(element(env.window, 'model-catalog-status').textContent, /revision-2/);
  assert.match(element(env.window, 'model-catalog-status').textContent, /failed validation/i);
  env.assertDrained();
});

async function startSignedIn(env, initialPairing = null, providers = {}) {
  env.expect('login', 'POST', jsonResponse(200, {ok: true}));
  env.expect('state', 'GET', jsonResponse(200, state({pairing: initialPairing, providers})));
  const token = element(env.window, 'admin-token');
  token.value = 'synthetic-admin-token';
  element(env.window, 'login-form').dispatchEvent(new env.window.Event('submit', {bubbles: true, cancelable: true}));
  await env.settle();
}

function editProviderForm(env, {model = 'synthetic-model', apiKey = 'synthetic-test-key'} = {}) {
  const form = element(env.window, 'provider-form');
  form.elements.quick_response_model.value = model;
  form.elements.api_key.value = apiKey;
  form.elements.quick_response_model.dispatchEvent(new env.window.Event('input', {bubbles: true}));
  return form;
}

function submitProviderForm(env) {
  return element(env.window, 'provider-form').dispatchEvent(new env.window.Event('submit', {bubbles: true, cancelable: true}));
}

const providerOperationStorage = 'floe-inference-provider-operation';
const savedProviderContext = (operationId, {hasApiKey = false, model = 'saved-model'} = {}) => JSON.stringify({
  schema: 1,
  operation_id: operationId,
  kind: 'update',
  provider: 'openai_compatible',
  has_api_key: hasApiKey,
  base_url: 'https://api.openai.com/v1',
  purposes: {quick_response: {model, reasoning_effort: 'medium', capabilities: ['chat']}},
  draft_generation: 1,
});

test('initial 401 leaves the dashboard locked on the login view', async (t) => {
  const env = makeDashboard([
    {path: '/manage/api/state', method: 'GET', response: Promise.resolve(jsonResponse(401, {error: {code: 'unauthorized'}}))},
  ]);
  t.after(() => env.close());
  await env.settle();

  assert.equal(element(env.window, 'login-panel').hidden, false);
  assert.equal(element(env.window, 'dashboard').hidden, true);
  assert.match(element(env.window, 'notice').textContent, /session is unavailable or expired/i);
  env.assertDrained();
});

test('login state failure can be retried without submitting the token again', async (t) => {
  const env = makeDashboard([
    {path: '/manage/api/state', method: 'GET', response: Promise.resolve(jsonResponse(401, {error: {code: 'unauthorized'}}))},
  ]);
  t.after(() => env.close());
  await env.settle();
  env.expect('login', 'POST', jsonResponse(200, {ok: true}));
  env.expect('state', 'GET', jsonResponse(503, {error: {code: 'configuration_unavailable'}}));
  element(env.window, 'admin-token').value = 'synthetic-admin-token';
  element(env.window, 'login-form').dispatchEvent(new env.window.Event('submit', {bubbles: true, cancelable: true}));
  await env.settle();

  assert.equal(element(env.window, 'login-panel').hidden, true);
  assert.equal(element(env.window, 'dashboard').hidden, false);
  assert.equal(element(env.window, 'dashboard-content').hidden, true);
  assert.equal(element(env.window, 'state-error').hidden, false);
  assert.match(element(env.window, 'state-error-message').textContent, /without signing in again/i);

  env.expect('state', 'GET', jsonResponse(200, state()));
  element(env.window, 'refresh').click();
  await env.settle();

  assert.equal(env.requests.filter((request) => request.path === '/manage/api/login').length, 1);
  assert.equal(element(env.window, 'state-error').hidden, true);
  assert.equal(element(env.window, 'dashboard-content').hidden, false);
  assert.equal(element(env.window, 'address').textContent, `${origin}`);
  env.assertDrained();
});

test('local-confirmed pairing shows only the allowed approve and reject controls', async (t) => {
  const env = makeDashboard([
    {path: '/manage/api/state', method: 'GET', response: Promise.resolve(jsonResponse(200, state({pairing: pairing()})))},
  ]);
  t.after(() => env.close());
  await env.settle();

  assert.equal(element(env.window, 'pair-panel').hidden, false);
  assert.equal(element(env.window, 'approve').hidden, false);
  assert.equal(element(env.window, 'reject').hidden, false);
  assert.equal(element(env.window, 'resume').hidden, true);
  assert.equal(element(env.window, 'abort').hidden, true);
  assert.match(element(env.window, 'pair-phase').textContent, /compare this code/i);
  env.assertDrained();
});

test('pending pairing without the server approve action cannot be approved', async (t) => {
  const env = makeDashboard([
    {path: '/manage/api/state', method: 'GET', response: Promise.resolve(jsonResponse(200, state({
      pairing: pairing({phase: 'pending', allowed_actions: ['reject']}),
    })))},
  ]);
  t.after(() => env.close());
  await env.settle();

  assert.equal(element(env.window, 'approve').hidden, true);
  assert.equal(element(env.window, 'approve').disabled, true);
  assert.equal(element(env.window, 'reject').hidden, false);
  assert.match(element(env.window, 'pair-phase').textContent, /verifying this connection/i);
  env.assertDrained();
});

test('expired pairing hides approval even when an old state snapshot permits it', async (t) => {
  const env = makeDashboard([
    {path: '/manage/api/state', method: 'GET', response: Promise.resolve(jsonResponse(200, state({
      pairing: pairing({expires: '2026-10-07T11:59:59.999Z'}),
    })))},
  ]);
  t.after(() => env.close());
  await env.settle();

  assert.equal(element(env.window, 'approve').hidden, true);
  assert.equal(element(env.window, 'approve').disabled, true);
  assert.equal(element(env.window, 'reject').hidden, true);
  assert.equal(element(env.window, 'reject').disabled, true);
  assert.match(element(env.window, 'pair-phase').textContent, /request has expired/i);
  env.assertDrained();
});

test('activating pairing exposes only server-permitted recovery actions', async (t) => {
  const env = makeDashboard([
    {path: '/manage/api/state', method: 'GET', response: Promise.resolve(jsonResponse(200, state({
      pairing: pairing({phase: 'activating', allowed_actions: ['abort']}),
    })))},
  ]);
  t.after(() => env.close());
  await env.settle();

  assert.equal(element(env.window, 'approve').hidden, true);
  assert.equal(element(env.window, 'reject').hidden, true);
  assert.equal(element(env.window, 'resume').hidden, true);
  assert.equal(element(env.window, 'abort').hidden, false);
  assert.match(element(env.window, 'pair-phase').textContent, /original retained credential/i);
  env.assertDrained();
});

test('approve posts exact pairing identity with current CSRF and refreshes state once', async (t) => {
  const initialPairing = pairing();
  const env = makeDashboard([
    {path: '/manage/api/state', method: 'GET', response: Promise.resolve(jsonResponse(401, {error: {code: 'unauthorized'}}))},
  ]);
  t.after(() => env.close());
  await env.settle();
  await startSignedIn(env, initialPairing);
  const inFlightRefresh = env.expectDeferred('state', 'GET');
  element(env.window, 'refresh').click();
  await env.settle();

  env.expect('pair/approve', 'POST', jsonResponse(200, {schema_version: 1, pairing_id: initialPairing.id, status: 'approved'}));
  element(env.window, 'approve').click();
  element(env.window, 'approve').click();
  await env.settle();
  assert.equal(env.requests.filter((request) => request.path === '/manage/api/state').length, 3);
  assert.equal(env.requests.filter((request) => request.path === '/manage/api/pair/approve').length, 1);

  const approval = env.requests.find((request) => request.path === '/manage/api/pair/approve');
  assert.equal(approval.method, 'POST');
  assert.equal(approval.credentials, 'same-origin');
  assert.equal(approval.headers['X-Floe-CSRF'], 'csrf-current-session');
  assert.deepEqual(JSON.parse(approval.body), {
    schema_version: 1,
    pairing_id: initialPairing.id,
    issuer_fingerprint: initialPairing.issuer_fingerprint,
  });
  assert.equal(element(env.window, 'pair-panel').hidden, false);

  inFlightRefresh.resolve(jsonResponse(200, state()));
  await env.settle();
  assert.equal(element(env.window, 'pair-panel').hidden, true);
  assert.match(element(env.window, 'notice').textContent, /app approved/i);
  env.assertDrained();
});

test('duplicate login submits and stale prior-session success or 401 cannot replace a newer session', async (t) => {
  for (const staleStatus of [200, 401]) {
    const env = makeDashboard([
      {path: '/manage/api/state', method: 'GET', response: Promise.resolve(jsonResponse(401, {error: {code: 'unauthorized'}}))},
    ]);
    t.after(() => env.close());
    await env.settle();

    const loginResponse = env.expectDeferred('login', 'POST');
    element(env.window, 'admin-token').value = 'synthetic-admin-token';
    const submit = () => element(env.window, 'login-form').dispatchEvent(new env.window.Event('submit', {bubbles: true, cancelable: true}));
    submit();
    submit();
    assert.equal(env.requests.filter((request) => request.path === '/manage/api/login').length, 1);
    loginResponse.resolve(jsonResponse(200, {ok: true}));
    env.expect('state', 'GET', jsonResponse(200, state({pairing: pairing()})));
    await env.settle();

    const staleState = env.expectDeferred('state', 'GET');
    element(env.window, 'refresh').click();
    env.expect('pair/approve', 'POST', jsonResponse(401, {error: {code: 'unauthorized'}}));
    element(env.window, 'approve').click();
    await env.settle();
    assert.equal(element(env.window, 'login-panel').hidden, false);

    env.expect('login', 'POST', jsonResponse(200, {ok: true}));
    env.expect('state', 'GET', jsonResponse(200, state({pairing: pairing({
      id: '33333333-3333-4333-8333-333333333333',
      issuer_fingerprint: 'c'.repeat(64),
    })})));
    element(env.window, 'admin-token').value = 'new-synthetic-admin-token';
    element(env.window, 'login-form').dispatchEvent(new env.window.Event('submit', {bubbles: true, cancelable: true}));
    await env.settle();

    staleState.resolve(jsonResponse(staleStatus, staleStatus === 200
      ? state({pairing: pairing({issuer_fingerprint: 'd'.repeat(64)})})
      : {error: {code: 'unauthorized'}}));
    await env.settle();

    assert.equal(element(env.window, 'login-panel').hidden, true);
    assert.equal(element(env.window, 'dashboard-content').hidden, false);
    assert.match(element(env.window, 'pair-fingerprint').textContent, /c{64}/);
    assert.doesNotMatch(element(env.window, 'pair-fingerprint').textContent, /d{64}/);
    env.assertDrained();
  }
});

test('lost provider response recovers no_record then replays the exact saved command ID and body', async (t) => {
  const env = makeDashboard([
    {path: '/manage/api/state', method: 'GET', response: Promise.resolve(jsonResponse(401, {error: {code: 'unauthorized'}}))},
  ]);
  t.after(() => env.close());
  await env.settle();
  await startSignedIn(env);

  editProviderForm(env, {model: 'submitted-model', apiKey: 'synthetic-only-key'});
  const firstPost = env.expectDeferred('provider', 'POST');
  submitProviderForm(env);
  await env.settle();
  const initialRequest = env.requests.find((request) => request.path === '/manage/api/provider');
  const originalBody = JSON.parse(initialRequest.body);
  const storedContext = env.window.sessionStorage.getItem(providerOperationStorage);
  assert.ok(storedContext);
  assert.doesNotMatch(storedContext, /synthetic-only-key/);
  assert.equal(JSON.parse(storedContext).operation_id, originalBody.operation_id);

  firstPost.reject(new TypeError('synthetic network response loss'));
  await env.settle();
  env.expect('inference/recover', 'POST', jsonResponse(200, {ok: true, recovered: true, status: 'no_record'}));
  env.expect('provider', 'POST', jsonResponse(200, {ok: true}));
  env.expect('state', 'GET', jsonResponse(200, state({providers: {openai_compatible: {base_url: 'https://api.openai.com/v1', purposes: {quick_response: {model: 'submitted-model'}}}}})));
  element(env.window, 'provider-operation-check').click();
  await env.settle();

  const posts = env.requests.filter((request) => request.path === '/manage/api/provider');
  assert.equal(posts.length, 2);
  assert.deepEqual(JSON.parse(posts[1].body), originalBody);
  assert.equal(JSON.parse(posts[1].body).operation_id, originalBody.operation_id);
  assert.equal(env.window.sessionStorage.getItem(providerOperationStorage), null);
  assert.match(element(env.window, 'provider-operation-status').textContent, /completed/i);
  env.assertDrained();
});

test('a delayed original provider request and no_record observation replay with one ID', async (t) => {
  const env = makeDashboard([
    {path: '/manage/api/state', method: 'GET', response: Promise.resolve(jsonResponse(401, {error: {code: 'unauthorized'}}))},
  ]);
  t.after(() => env.close());
  await env.settle();
  await startSignedIn(env);

  const form = editProviderForm(env, {model: 'delayed-model', apiKey: 'synthetic-delayed-key'});
  const delayedPost = env.expectDeferred('provider', 'POST');
  submitProviderForm(env);
  await env.settle();
  const originalBody = JSON.parse(env.requests.find((request) => request.path === '/manage/api/provider').body);
  form.elements.quick_response_model.value = 'newer-draft-model';
  form.elements.quick_response_model.dispatchEvent(new env.window.Event('input', {bubbles: true}));
  env.expect('inference/recover', 'POST', jsonResponse(200, {ok: true, recovered: true, status: 'no_record'}));
  env.expect('provider', 'POST', jsonResponse(200, {ok: true}));
  env.expect('state', 'GET', jsonResponse(200, state({providers: {openai_compatible: {base_url: 'https://api.openai.com/v1', purposes: {quick_response: {model: 'delayed-model'}}}}})));
  element(env.window, 'provider-operation-check').click();
  await env.settle();
  assert.equal(env.requests.filter((request) => request.path === '/manage/api/provider').length, 2);
  assert.deepEqual(JSON.parse(env.requests.filter((request) => request.path === '/manage/api/provider')[1].body), originalBody);
  assert.equal(env.window.sessionStorage.getItem(providerOperationStorage), null);
  assert.equal(form.elements.quick_response_model.value, 'newer-draft-model');

  // The late response belongs to the already settled exact command and must not replace current UI state.
  delayedPost.resolve(jsonResponse(200, {ok: true}));
  await env.settle();
  assert.match(element(env.window, 'provider-operation-status').textContent, /completed/i);
  env.assertDrained();
});

test('completion after a pending submit preserves newer unsent provider edits', async (t) => {
  const env = makeDashboard([
    {path: '/manage/api/state', method: 'GET', response: Promise.resolve(jsonResponse(401, {error: {code: 'unauthorized'}}))},
  ]);
  t.after(() => env.close());
  await env.settle();
  await startSignedIn(env);

  const form = editProviderForm(env, {model: 'submitted-model', apiKey: 'synthetic-first-key'});
  const delayedPost = env.expectDeferred('provider', 'POST');
  submitProviderForm(env);
  await env.settle();
  form.elements.quick_response_model.value = 'newer-unsent-model';
  form.elements.quick_response_model.dispatchEvent(new env.window.Event('input', {bubbles: true}));

  env.expect('inference/recover', 'POST', jsonResponse(200, {ok: true, recovered: true}));
  env.expect('state', 'GET', jsonResponse(200, state({providers: {openai_compatible: {base_url: 'https://api.openai.com/v1', purposes: {quick_response: {model: 'submitted-model'}}}}})));
  element(env.window, 'provider-operation-check').click();
  await env.settle();
  assert.equal(form.elements.quick_response_model.value, 'newer-unsent-model');
  assert.match(element(env.window, 'provider-operation-status').textContent, /newer form edits remain on screen/i);
  assert.equal(env.window.sessionStorage.getItem(providerOperationStorage), null);

  delayedPost.resolve(jsonResponse(200, {ok: true}));
  await env.settle();
  assert.equal(form.elements.quick_response_model.value, 'newer-unsent-model');
  env.assertDrained();
});

test('reload retains only recovery context and requires API-key re-entry for a no_record retry', async (t) => {
  const first = makeDashboard([
    {path: '/manage/api/state', method: 'GET', response: Promise.resolve(jsonResponse(401, {error: {code: 'unauthorized'}}))},
  ]);
  t.after(() => first.close());
  await first.settle();
  await startSignedIn(first);
  editProviderForm(first, {model: 'saved-before-reload', apiKey: 'synthetic-reload-key'});
  const firstPost = first.expectDeferred('provider', 'POST');
  submitProviderForm(first);
  await first.settle();
  firstPost.reject(new TypeError('synthetic connection loss'));
  await first.settle();
  const savedContext = first.window.sessionStorage.getItem(providerOperationStorage);
  assert.ok(savedContext);
  assert.doesNotMatch(savedContext, /synthetic-reload-key/);
  first.close();

  const operationId = JSON.parse(savedContext).operation_id;
  const env = makeDashboard([
    {path: '/manage/api/state', method: 'GET', response: Promise.resolve(jsonResponse(401, {error: {code: 'unauthorized'}}))},
  ], {storageSeed: {[providerOperationStorage]: savedContext}});
  t.after(() => env.close());
  await env.settle();
  await startSignedIn(env);
  assert.equal(element(env.window, 'provider-form').elements.api_key.value, '');

  env.expect('inference/recover', 'POST', jsonResponse(200, {ok: true, recovered: true, status: 'no_record'}));
  element(env.window, 'provider-operation-check').click();
  await env.settle();
  assert.equal(env.window.sessionStorage.getItem(providerOperationStorage), savedContext);
  assert.equal(element(env.window, 'provider-recovery-key-row').hidden, false);
  assert.match(element(env.window, 'provider-operation-status').textContent, /earlier request may still arrive/i);
  assert.equal(env.requests.filter((request) => request.path === '/manage/api/provider').length, 0);

  const form = element(env.window, 'provider-form');
  form.elements.quick_response_model.value = 'newer-unsent-after-reload';
  form.elements.quick_response_model.dispatchEvent(new env.window.Event('input', {bubbles: true}));
  element(env.window, 'provider-recovery-key').value = 'synthetic-reload-key';
  env.expect('provider', 'POST', jsonResponse(200, {ok: true}));
  env.expect('state', 'GET', jsonResponse(200, state({providers: {openai_compatible: {base_url: 'https://api.openai.com/v1', purposes: {quick_response: {model: 'saved-before-reload'}}}}})));
  element(env.window, 'provider-operation-retry').click();
  await env.settle();

  const replay = env.requests.find((request) => request.path === '/manage/api/provider');
  const body = JSON.parse(replay.body);
  assert.equal(body.operation_id, operationId);
  assert.equal(body.api_key, 'synthetic-reload-key');
  assert.equal(body.purposes.quick_response.model, 'saved-before-reload');
  assert.notEqual(body.purposes.quick_response.model, form.elements.quick_response_model.value);
  assert.equal(form.elements.quick_response_model.value, 'newer-unsent-after-reload');
  assert.equal(element(env.window, 'provider-recovery-key').value, '');
  assert.equal(env.window.sessionStorage.getItem(providerOperationStorage), null);
  env.assertDrained();
});

test('recovery distinguishes completed, aborted, pending, and no_record states', async (t) => {
  const cases = [
    {name: 'completed', result: jsonResponse(200, {ok: true, recovered: true}), status: /completed/i, retained: false, stateRefresh: true},
    {name: 'aborted', result: jsonResponse(200, {ok: true, recovered: true, category: 'unavailable', code: 'credential_store_unavailable'}), status: /not applied/i, retained: false, stateRefresh: true},
    {name: 'pending', result: jsonResponse(503, {error: {code: 'configuration_unavailable'}}), status: /could not confirm/i, retained: true, stateRefresh: false},
    {name: 'no_record', result: jsonResponse(200, {ok: true, recovered: true, status: 'no_record'}), status: /earlier request may still arrive/i, retained: true, stateRefresh: false},
  ];
  for (const scenario of cases) {
    const context = savedProviderContext('00000000-0000-4000-8000-000000000091', {hasApiKey: true});
    const env = makeDashboard([
      {path: '/manage/api/state', method: 'GET', response: Promise.resolve(jsonResponse(401, {error: {code: 'unauthorized'}}))},
    ], {storageSeed: {[providerOperationStorage]: context}});
    t.after(() => env.close());
    await env.settle();
    await startSignedIn(env);
    env.expect('inference/recover', 'POST', scenario.result);
    if (scenario.stateRefresh) env.expect('state', 'GET', jsonResponse(200, state()));
    element(env.window, 'provider-operation-check').click();
    await env.settle();
    assert.match(element(env.window, 'provider-operation-status').textContent, scenario.status, scenario.name);
    assert.equal(env.window.sessionStorage.getItem(providerOperationStorage) !== null, scenario.retained, scenario.name);
    if (scenario.name === 'no_record') assert.equal(element(env.window, 'provider-recovery-key-row').hidden, false);
    if (scenario.name === 'pending') assert.equal(element(env.window, 'provider-recovery-key-row').hidden, true);
    env.assertDrained();
  }
});

test('provider navigation does not rebind a pending command', async (t) => {
  const env = makeDashboard([
    {path: '/manage/api/state', method: 'GET', response: Promise.resolve(jsonResponse(401, {error: {code: 'unauthorized'}}))},
  ]);
  t.after(() => env.close());
  await env.settle();
  await startSignedIn(env);

  editProviderForm(env, {model: 'original-provider-model', apiKey: 'synthetic-navigation-key'});
  const originalPost = env.expectDeferred('provider', 'POST');
  submitProviderForm(env);
  await env.settle();
  assert.equal(env.requests.filter((request) => request.path === '/manage/api/provider').length, 1);
  const originalBody = JSON.parse(env.requests.find((request) => request.path === '/manage/api/provider').body);

  env.window.document.querySelector('.provider-option[data-provider="codex_oauth"]').click();
  assert.equal(element(env.window, 'provider-form').elements.provider.value, 'codex_oauth');
  originalPost.reject(new TypeError('synthetic network failure'));
  await env.settle();
  env.expect('inference/recover', 'POST', jsonResponse(200, {ok: true, recovered: true, status: 'no_record'}));
  env.expect('provider', 'POST', jsonResponse(200, {ok: true}));
  env.expect('state', 'GET', jsonResponse(200, state({providers: {
    openai_compatible: {base_url: 'https://api.openai.com/v1', purposes: {quick_response: {model: 'original-provider-model'}}},
    codex_oauth: {purposes: {}},
  }})));
  element(env.window, 'provider-operation-check').click();
  await env.settle();
  const posts = env.requests.filter((request) => request.path === '/manage/api/provider');
  assert.equal(posts.length, 2);
  assert.deepEqual(JSON.parse(posts[1].body), originalBody);
  assert.equal(element(env.window, 'provider-form').elements.provider.value, 'codex_oauth');
  env.assertDrained();
});

test('double submit uses one provider request and settles only after its response', async (t) => {
  const env = makeDashboard([
    {path: '/manage/api/state', method: 'GET', response: Promise.resolve(jsonResponse(401, {error: {code: 'unauthorized'}}))},
  ]);
  t.after(() => env.close());
  await env.settle();
  await startSignedIn(env);

  editProviderForm(env, {model: 'one-submit-model', apiKey: 'synthetic-double-submit-key'});
  const response = env.expectDeferred('provider', 'POST');
  submitProviderForm(env);
  submitProviderForm(env);
  await env.settle();
  assert.equal(env.requests.filter((request) => request.path === '/manage/api/provider').length, 1);
  const operationId = JSON.parse(env.requests.find((request) => request.path === '/manage/api/provider').body).operation_id;
  assert.equal(JSON.parse(env.window.sessionStorage.getItem(providerOperationStorage)).operation_id, operationId);
  env.expect('state', 'GET', jsonResponse(200, state()));
  response.resolve(jsonResponse(200, {ok: true}));
  await env.settle();
  assert.equal(env.window.sessionStorage.getItem(providerOperationStorage), null);
  env.assertDrained();
});
