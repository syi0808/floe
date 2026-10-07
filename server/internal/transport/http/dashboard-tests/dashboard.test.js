import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import {JSDOM} from 'jsdom';

const directory = new URL('.', import.meta.url);
const html = readFileSync(new URL('../web/index.html', directory), 'utf8');
const app = readFileSync(new URL('../web/app.js', directory), 'utf8');
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
  const promise = new Promise((done) => { resolve = done; });
  return {promise, resolve};
}

function makeDashboard(initialExpectations, {now = Date.parse('2026-10-07T12:00:00.000Z'), qaMode = false} = {}) {
  const page = qaMode
    ? html.replace('name="floe-qa-no-auth" content="false"', 'name="floe-qa-no-auth" content="true"')
      .replace('name="floe-qa-csrf" content=""', 'name="floe-qa-csrf" content="csrf-qa-session"')
    : html;
  const dom = new JSDOM(page, {
    url: `${origin}/manage/`,
    runScripts: 'outside-only',
  });
  const {window} = dom;
  const expectations = [...initialExpectations];
  const requests = [];
  const unexpected = [];
  let currentTime = now;

  Object.defineProperty(window.document, 'hidden', {configurable: true, get: () => false});
  Object.defineProperty(window.Date, 'now', {configurable: true, value: () => currentTime});
  window.setInterval = () => 0;
  window.clearInterval = () => {};
  window.confirm = () => true;
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
  window.eval(app);

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

function state({csrf = 'csrf-current-session', clients = [], pairing = null, qa_mode = false} = {}) {
  return {csrf, clients, providers: {}, address: `${origin}`, pairing, qa_mode};
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

async function startSignedIn(env, initialPairing = null) {
  env.expect('login', 'POST', jsonResponse(200, {ok: true}));
  env.expect('state', 'GET', jsonResponse(200, state({pairing: initialPairing})));
  const token = element(env.window, 'admin-token');
  token.value = 'synthetic-admin-token';
  element(env.window, 'login-form').dispatchEvent(new env.window.Event('submit', {bubbles: true, cancelable: true}));
  await env.settle();
}

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

test('development QA mode shows its warning and loads state without token login', async (t) => {
  const env = makeDashboard([
    {path: '/manage/api/state', method: 'GET', response: Promise.resolve(jsonResponse(200, state({qa_mode: true})))},
  ], {qaMode: true});
  t.after(() => env.close());
  await env.settle();

  assert.equal(element(env.window, 'qa-mode-warning').hidden, false);
  assert.equal(element(env.window, 'login-panel').hidden, true);
  assert.equal(element(env.window, 'logout').hidden, true);
  assert.equal(env.requests.length, 1);
  assert.equal(env.requests[0].path, '/manage/api/state');
  assert.equal(env.requests[0].headers['X-Floe-CSRF'], 'csrf-qa-session');
  assert.equal(element(env.window, 'dashboard-content').hidden, false);
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
