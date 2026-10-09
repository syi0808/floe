package httptransport

import (
	"bytes"
	"context"
	"crypto/ed25519"
	"crypto/rand"
	"encoding/base64"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"sync"
	"testing"

	storageadapter "floe/server/internal/adapters/storage"
	"floe/server/internal/credentials"
	"floe/server/internal/inference"
	"floe/server/internal/inference/providers"
	"floe/server/internal/modelcatalog"
	"floe/server/internal/pairing"
	"floe/server/internal/storage"
	"floe/server/internal/trust"
)

const dashboardTestAddress = "127.0.0.1:18431"

type memoryCredentialStore struct {
	mu     sync.Mutex
	values map[string]string
}

func newMemoryCredentialStore() *memoryCredentialStore {
	return &memoryCredentialStore{values: make(map[string]string)}
}

func (store *memoryCredentialStore) Get(ctx context.Context, name string) (string, error) {
	if err := ctx.Err(); err != nil {
		return "", err
	}
	if name == "" || len(name) > 256 {
		return "", credentials.ErrUnavailable
	}
	store.mu.Lock()
	defer store.mu.Unlock()
	return store.values[name], nil
}

func (store *memoryCredentialStore) Put(ctx context.Context, name, value string) error {
	if err := ctx.Err(); err != nil {
		return err
	}
	if name == "" || len(name) > 256 || value == "" || len(value) > 131072 {
		return credentials.ErrUnavailable
	}
	store.mu.Lock()
	defer store.mu.Unlock()
	if _, exists := store.values[name]; !exists && len(store.values) >= 256 {
		return credentials.ErrUnavailable
	}
	store.values[name] = value
	return nil
}

func (store *memoryCredentialStore) Delete(ctx context.Context, name string) error {
	if err := ctx.Err(); err != nil {
		return err
	}
	store.mu.Lock()
	defer store.mu.Unlock()
	delete(store.values, name)
	return nil
}

type consoleFixture struct {
	address     string
	adminToken  string
	handler     *Handler
	trust       *trust.Service
	credentials *memoryCredentialStore
	catalog     *modelcatalog.Store
}

func newConsoleFixture(t *testing.T) *consoleFixture {
	t.Helper()
	directory := t.TempDir()
	key := make([]byte, 32)
	if _, err := rand.Read(key); err != nil {
		t.Fatal(err)
	}
	root, err := storage.NewFiles(directory, "dashboard-headless-qa", key, true)
	if err != nil {
		t.Fatalf("create encrypted test storage: %v", err)
	}
	t.Cleanup(root.Close)
	trustFiles, err := root.Scope("trust")
	if err != nil {
		t.Fatalf("open Trust storage scope: %v", err)
	}
	inferenceFiles, err := root.Scope("inference")
	if err != nil {
		t.Fatalf("open inference storage scope: %v", err)
	}
	trustRepository := storageadapter.NewTrustRepository(trustFiles)
	trustService, err := trust.Open(trustRepository, true)
	if err != nil {
		t.Fatalf("open Trust owner: %v", err)
	}
	adminToken, err := trustRepository.ReadAdministratorToken()
	if err != nil {
		t.Fatalf("read generated test administrator token: %v", err)
	}
	credentialsStore := newMemoryCredentialStore()
	inferenceService, err := inference.NewService(trustService)
	if err != nil {
		t.Fatalf("create inference owner: %v", err)
	}
	factory := providers.NewFactory(credentialsStore.Get, nil)
	configuration, err := inference.OpenConfiguration(context.Background(), inferenceFiles, inferenceService, trustService, credentialsStore, factory)
	if err != nil {
		t.Fatalf("open inference configuration owner: %v", err)
	}
	pairingOperations := pairing.NewOperations(trustService, credentialsStore, nil)
	catalog, err := modelcatalog.Open("")
	if err != nil {
		t.Fatalf("open embedded model catalog: %v", err)
	}
	return &consoleFixture{
		address:     dashboardTestAddress,
		adminToken:  adminToken,
		trust:       trustService,
		credentials: credentialsStore,
		catalog:     catalog,
		handler: &Handler{
			Address:       dashboardTestAddress,
			Trust:         trustService,
			Pairing:       pairingOperations,
			Configuration: configuration,
			ModelCatalog:  catalog,
			Inference:     &InferenceHandler{Service: inferenceService, Trust: trustService, Address: dashboardTestAddress},
		},
	}
}

func (fixture *consoleFixture) request(method, path string, body any, cookie *http.Cookie, origin, csrf, host string) *httptest.ResponseRecorder {
	var content bytes.Buffer
	if body != nil {
		if err := json.NewEncoder(&content).Encode(body); err != nil {
			panic(err)
		}
	}
	request := httptest.NewRequest(method, path, &content)
	if host == "" {
		host = fixture.address
	}
	request.Host = host
	if body != nil {
		request.Header.Set("Content-Type", "application/json")
	}
	if origin != "" {
		request.Header.Set("Origin", origin)
	}
	if csrf != "" {
		request.Header.Set("X-Floe-CSRF", csrf)
	}
	if cookie != nil {
		request.AddCookie(cookie)
	}
	response := httptest.NewRecorder()
	fixture.handler.ServeHTTP(response, request)
	return response
}

func TestConsoleServesDashboardModuleGraph(t *testing.T) {
	fixture := newConsoleFixture(t)

	index := fixture.request(http.MethodGet, "/manage/", nil, nil, "", "", "")
	if index.Code != http.StatusOK || !bytes.Contains(index.Body.Bytes(), []byte(`<script type="module" src="/manage/app.js"></script>`)) {
		t.Fatalf("dashboard index omitted its module entry point: status=%d body=%s", index.Code, index.Body.String())
	}

	app := fixture.request(http.MethodGet, "/manage/app.js", nil, nil, "", "", "")
	if app.Code != http.StatusOK || app.Header().Get("Content-Type") != "text/javascript; charset=utf-8" || !bytes.Contains(app.Body.Bytes(), []byte(`from './budget-override.mjs'`)) {
		t.Fatalf("dashboard app module was not served correctly: status=%d content_type=%q", app.Code, app.Header().Get("Content-Type"))
	}

	helper := fixture.request(http.MethodGet, "/manage/budget-override.mjs", nil, nil, "", "", "")
	if helper.Code != http.StatusOK || helper.Header().Get("Content-Type") != "text/javascript; charset=utf-8" || !bytes.Contains(helper.Body.Bytes(), []byte("export function budgetOverrideForTarget")) {
		t.Fatalf("dashboard helper module was not served correctly: status=%d content_type=%q", helper.Code, helper.Header().Get("Content-Type"))
	}
}

func (fixture *consoleFixture) login(t *testing.T) (*http.Cookie, string) {
	t.Helper()
	response := fixture.request(http.MethodPost, "/manage/api/login", map[string]string{"token": fixture.adminToken}, nil, "http://"+fixture.address, "", "")
	if response.Code != http.StatusOK {
		t.Fatalf("login returned %d: %s", response.Code, response.Body.String())
	}
	cookies := response.Result().Cookies()
	if len(cookies) != 1 {
		t.Fatalf("login set %d cookies, want one", len(cookies))
	}
	state := fixture.getState(t, cookies[0])
	if state.CSRF == "" {
		t.Fatal("state response omitted CSRF capability")
	}
	return cookies[0], state.CSRF
}

type consoleState struct {
	CSRF         string                  `json:"csrf"`
	ModelCatalog modelcatalog.Projection `json:"model_catalog"`
	Pairing      *struct {
		ID                string   `json:"id"`
		Phase             string   `json:"phase"`
		AllowedActions    []string `json:"allowed_actions"`
		IssuerFingerprint string   `json:"issuer_fingerprint"`
	} `json:"pairing"`
}

func TestConsoleProjectsReadOnlyModelCatalogOnlyToOperatorSession(t *testing.T) {
	fixture := newConsoleFixture(t)
	unauthenticated := fixture.request(http.MethodGet, "/manage/api/state", nil, nil, "", "", "")
	if unauthenticated.Code != http.StatusUnauthorized {
		t.Fatalf("unauthenticated dashboard state returned %d", unauthenticated.Code)
	}
	cookie, _ := fixture.login(t)
	state := fixture.getState(t, cookie)
	if state.ModelCatalog.Catalog.Revision != 1 || state.ModelCatalog.Status.Source != "bootstrap" {
		t.Fatalf("operator state omitted catalog projection: %#v", state.ModelCatalog)
	}
	if len(state.ModelCatalog.Catalog.Providers) != 1 || state.ModelCatalog.Catalog.Providers[0].ProviderID != "codex_oauth" {
		t.Fatalf("unexpected catalog provider projection: %#v", state.ModelCatalog.Catalog.Providers)
	}
	for _, model := range state.ModelCatalog.Catalog.Providers[0].Models {
		if model.Metadata != nil || model.Source != "migrated_repository_suggestions" {
			t.Fatalf("unverified metadata leaked into operator catalog: %#v", model)
		}
	}
}

func (fixture *consoleFixture) getState(t *testing.T, cookie *http.Cookie) consoleState {
	t.Helper()
	response := fixture.request(http.MethodGet, "/manage/api/state", nil, cookie, "", "", "")
	if response.Code != http.StatusOK {
		t.Fatalf("state returned %d: %s", response.Code, response.Body.String())
	}
	var state consoleState
	if err := json.Unmarshal(response.Body.Bytes(), &state); err != nil {
		t.Fatalf("decode state response: %v", err)
	}
	return state
}

func responseCode(t *testing.T, response *httptest.ResponseRecorder) string {
	t.Helper()
	var body struct {
		Error struct {
			Code string `json:"code"`
		} `json:"error"`
	}
	if err := json.Unmarshal(response.Body.Bytes(), &body); err != nil {
		t.Fatalf("decode error response: %v", err)
	}
	return body.Error.Code
}

func TestConsoleLoginCookieAndWrongToken(t *testing.T) {
	fixture := newConsoleFixture(t)
	wrong := fixture.request(http.MethodPost, "/manage/api/login", map[string]string{"token": "wrong-synthetic-token"}, nil, "http://"+fixture.address, "", "")
	if wrong.Code != http.StatusUnauthorized || responseCode(t, wrong) != "unauthorized" {
		t.Fatalf("wrong token returned %d with %q", wrong.Code, responseCode(t, wrong))
	}
	if len(wrong.Result().Cookies()) != 0 {
		t.Fatal("wrong token created an operator cookie")
	}

	response := fixture.request(http.MethodPost, "/manage/api/login", map[string]string{"token": fixture.adminToken}, nil, "http://"+fixture.address, "", "")
	if response.Code != http.StatusOK {
		t.Fatalf("valid login returned %d: %s", response.Code, response.Body.String())
	}
	cookies := response.Result().Cookies()
	if len(cookies) != 1 {
		t.Fatalf("valid login set %d cookies, want one", len(cookies))
	}
	cookie := cookies[0]
	if cookie.Name != "floe_management" || cookie.Value == "" || cookie.Path != "/" || !cookie.HttpOnly || cookie.SameSite != http.SameSiteStrictMode || cookie.MaxAge != 43200 {
		t.Fatalf("unexpected dashboard cookie flags: %#v", cookie)
	}
	state := fixture.getState(t, cookie)
	if state.CSRF == "" {
		t.Fatal("authenticated state omitted its CSRF token")
	}
}

func TestConsoleSessionGuardsAndLogoutInvalidation(t *testing.T) {
	fixture := newConsoleFixture(t)
	absent := fixture.request(http.MethodGet, "/manage/api/state", nil, nil, "", "", "")
	if absent.Code != http.StatusUnauthorized || responseCode(t, absent) != "unauthorized" {
		t.Fatalf("request without a session returned %d with %q", absent.Code, responseCode(t, absent))
	}
	fakeCookie := &http.Cookie{Name: "floe_management", Value: "stale-synthetic-session"}
	stale := fixture.request(http.MethodGet, "/manage/api/state", nil, fakeCookie, "", "", "")
	if stale.Code != http.StatusUnauthorized || responseCode(t, stale) != "unauthorized" {
		t.Fatalf("request with a stale session returned %d with %q", stale.Code, responseCode(t, stale))
	}

	badHost := fixture.request(http.MethodGet, "/manage/api/state", nil, nil, "", "", "localhost:18431")
	if badHost.Code != http.StatusForbidden || responseCode(t, badHost) != "invalid_host" {
		t.Fatalf("wrong host returned %d with %q", badHost.Code, responseCode(t, badHost))
	}
	badOrigin := fixture.request(http.MethodPost, "/manage/api/login", map[string]string{"token": fixture.adminToken}, nil, "https://127.0.0.1:18431", "", "")
	if badOrigin.Code != http.StatusForbidden || responseCode(t, badOrigin) != "invalid_origin" {
		t.Fatalf("wrong origin returned %d with %q", badOrigin.Code, responseCode(t, badOrigin))
	}
	missingOrigin := fixture.request(http.MethodPost, "/manage/api/login", map[string]string{"token": fixture.adminToken}, nil, "", "", "")
	if missingOrigin.Code != http.StatusForbidden || responseCode(t, missingOrigin) != "invalid_origin" {
		t.Fatalf("POST without an Origin returned %d with %q", missingOrigin.Code, responseCode(t, missingOrigin))
	}

	cookie, csrf := fixture.login(t)
	wrongCSRF := fixture.request(http.MethodPost, "/manage/api/logout", map[string]bool{}, cookie, "http://"+fixture.address, "stale-csrf", "")
	if wrongCSRF.Code != http.StatusUnauthorized || responseCode(t, wrongCSRF) != "unauthorized" {
		t.Fatalf("wrong CSRF returned %d with %q", wrongCSRF.Code, responseCode(t, wrongCSRF))
	}
	wrongMutationOrigin := fixture.request(http.MethodPost, "/manage/api/logout", map[string]bool{}, cookie, "http://localhost:18431", csrf, "")
	if wrongMutationOrigin.Code != http.StatusForbidden || responseCode(t, wrongMutationOrigin) != "invalid_origin" {
		t.Fatalf("wrong mutation origin returned %d with %q", wrongMutationOrigin.Code, responseCode(t, wrongMutationOrigin))
	}

	logout := fixture.request(http.MethodPost, "/manage/api/logout", map[string]bool{}, cookie, "http://"+fixture.address, csrf, "")
	if logout.Code != http.StatusOK {
		t.Fatalf("logout returned %d: %s", logout.Code, logout.Body.String())
	}
	clearCookies := logout.Result().Cookies()
	if len(clearCookies) != 1 || clearCookies[0].Name != "floe_management" || clearCookies[0].MaxAge != -1 {
		t.Fatalf("logout did not expire the dashboard cookie: %#v", clearCookies)
	}
	postLogout := fixture.request(http.MethodGet, "/manage/api/state", nil, cookie, "", "", "")
	if postLogout.Code != http.StatusUnauthorized || responseCode(t, postLogout) != "unauthorized" {
		t.Fatalf("logged-out session returned %d with %q", postLogout.Code, responseCode(t, postLogout))
	}
}

type pairingStartResponse struct {
	PairingID         string `json:"pairing_id"`
	Proof             string `json:"proof"`
	ChallengeID       string `json:"challenge_id"`
	ChallengeB64URL   string `json:"challenge_b64url"`
	ProducerSignature string `json:"producer_signature"`
	Issuer            struct {
		KeyID       string `json:"key_id"`
		PublicKey   string `json:"public_key"`
		Fingerprint string `json:"fingerprint"`
	} `json:"issuer"`
	Producer trust.ProducerMetadata `json:"producer"`
}

type testEnrollment struct {
	start      pairingStartResponse
	privateKey ed25519.PrivateKey
}

func (fixture *consoleFixture) startPairing(t *testing.T) testEnrollment {
	t.Helper()
	publicKey, privateKey, err := ed25519.GenerateKey(rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	proofBytes := make([]byte, 32)
	if _, err = rand.Read(proofBytes); err != nil {
		t.Fatal(err)
	}
	input := pairing.Request{
		SchemaVersion:   1,
		OperationID:     trust.NewID(),
		Proof:           base64.RawURLEncoding.EncodeToString(proofBytes),
		PersonID:        trust.NewID(),
		DeviceID:        "synthetic-device",
		IssuerKeyID:     trust.NewID(),
		IssuerPublicKey: base64.RawURLEncoding.EncodeToString(publicKey),
	}
	response := fixture.request(http.MethodPost, "/pair/start", input, nil, "", "", "")
	if response.Code != http.StatusOK {
		t.Fatalf("pair start returned %d: %s", response.Code, response.Body.String())
	}
	var start pairingStartResponse
	if err := json.Unmarshal(response.Body.Bytes(), &start); err != nil {
		t.Fatalf("decode pair start: %v", err)
	}
	challenge, err := base64.RawURLEncoding.DecodeString(start.ChallengeB64URL)
	if err != nil || len(challenge) == 0 {
		t.Fatalf("pair start returned invalid challenge: %v", err)
	}
	producerKey, err := base64.RawURLEncoding.DecodeString(start.Producer.PublicKey)
	if err != nil || len(producerKey) != ed25519.PublicKeySize {
		t.Fatalf("pair start returned invalid producer key: %v", err)
	}
	producerSignature, err := base64.RawURLEncoding.DecodeString(start.ProducerSignature)
	if err != nil || len(producerSignature) != ed25519.SignatureSize {
		t.Fatalf("pair start returned invalid producer signature: %v", err)
	}
	producerMessage := append([]byte("floe.remote.producer.v1\x00"), challenge...)
	if !ed25519.Verify(ed25519.PublicKey(producerKey), producerMessage, producerSignature) {
		t.Fatal("pair start producer signature did not verify against the exact challenge")
	}
	if start.Issuer.KeyID != input.IssuerKeyID || start.Issuer.PublicKey != input.IssuerPublicKey || start.Proof != input.Proof {
		t.Fatal("pair start changed the synthetic issuer identity or proof")
	}

	signature := ed25519.Sign(privateKey, append([]byte(trust.SignatureDomain), challenge...))
	confirm := pairing.Request{
		SchemaVersion: 1,
		PairingID:     start.PairingID,
		Proof:         start.Proof,
		ChallengeID:   start.ChallengeID,
		KeyID:         input.IssuerKeyID,
		Signature:     base64.RawURLEncoding.EncodeToString(signature),
	}
	confirmed := fixture.request(http.MethodPost, "/pair/confirm", confirm, nil, "", "", "")
	if confirmed.Code != http.StatusOK {
		t.Fatalf("pair confirm returned %d: %s", confirmed.Code, confirmed.Body.String())
	}
	var confirmation struct {
		PairingID string `json:"pairing_id"`
		Status    string `json:"status"`
	}
	if err = json.Unmarshal(confirmed.Body.Bytes(), &confirmation); err != nil || confirmation.PairingID != start.PairingID || confirmation.Status != "local_confirmed" {
		t.Fatalf("unexpected confirmation response %#v (decode error %v)", confirmation, err)
	}
	return testEnrollment{start: start, privateKey: privateKey}
}

func TestConsolePairApprovalUsesSignedEnrollmentAndExactFingerprint(t *testing.T) {
	fixture := newConsoleFixture(t)
	cookie, csrf := fixture.login(t)
	enrollment := fixture.startPairing(t)
	state := fixture.getState(t, cookie)
	if state.Pairing == nil || state.Pairing.ID != enrollment.start.PairingID || state.Pairing.Phase != "local_confirmed" {
		t.Fatalf("state did not expose the confirmed pairing: %#v", state.Pairing)
	}
	if !containsAction(state.Pairing.AllowedActions, "approve") || !containsAction(state.Pairing.AllowedActions, "reject") {
		t.Fatalf("confirmed pairing exposed actions %v", state.Pairing.AllowedActions)
	}

	wrongFingerprint := trust.Digest("stale synthetic issuer fingerprint")
	if wrongFingerprint == enrollment.start.Issuer.Fingerprint {
		wrongFingerprint = trust.Digest("another stale synthetic issuer fingerprint")
	}
	wrong := fixture.request(http.MethodPost, "/manage/api/pair/approve", pairing.ApprovalRequest{
		SchemaVersion: 1,
		PairingID:     enrollment.start.PairingID,
		Fingerprint:   wrongFingerprint,
	}, cookie, "http://"+fixture.address, csrf, "")
	if wrong.Code != http.StatusConflict || responseCode(t, wrong) != "pairing_not_confirmed" {
		t.Fatalf("wrong fingerprint returned %d with %q", wrong.Code, responseCode(t, wrong))
	}

	approved := fixture.request(http.MethodPost, "/manage/api/pair/approve", pairing.ApprovalRequest{
		SchemaVersion: 1,
		PairingID:     enrollment.start.PairingID,
		Fingerprint:   enrollment.start.Issuer.Fingerprint,
	}, cookie, "http://"+fixture.address, csrf, "")
	if approved.Code != http.StatusOK {
		t.Fatalf("approval with the exact issuer fingerprint returned %d: %s", approved.Code, approved.Body.String())
	}
	var approval struct {
		PairingID string `json:"pairing_id"`
		Status    string `json:"status"`
	}
	if err := json.Unmarshal(approved.Body.Bytes(), &approval); err != nil || approval.PairingID != enrollment.start.PairingID || approval.Status != "approved" {
		t.Fatalf("unexpected approval response %#v (decode error %v)", approval, err)
	}

	poll := fixture.request(http.MethodPost, "/pair/poll", pairing.Request{
		SchemaVersion: 1,
		PairingID:     enrollment.start.PairingID,
		Proof:         enrollment.start.Proof,
	}, nil, "", "", "")
	if poll.Code != http.StatusOK {
		t.Fatalf("poll after approval returned %d: %s", poll.Code, poll.Body.String())
	}
	var delivery struct {
		Status string `json:"status"`
		Token  string `json:"token"`
	}
	if err := json.Unmarshal(poll.Body.Bytes(), &delivery); err != nil || delivery.Status != "approved" || delivery.Token == "" {
		t.Fatalf("poll did not release the committed synthetic credential: %#v (decode error %v)", delivery, err)
	}
	principal, err := fixture.trust.AuthenticateBearer(context.Background(), delivery.Token)
	if err != nil || principal.ClientID() != enrollment.start.PairingID {
		t.Fatalf("committed credential did not authenticate its exact pairing: client=%q err=%v", principal.ClientID(), err)
	}
	issuer, err := fixture.trust.ActiveIssuer(principal)
	if err != nil || issuer.KeyID != enrollment.start.Issuer.KeyID || !bytes.Equal(issuer.PublicKey, enrollment.privateKey.Public().(ed25519.PublicKey)) {
		t.Fatalf("committed issuer did not match the confirmed synthetic key: key=%q err=%v", issuer.KeyID, err)
	}
}

func TestConsolePendingPairingCannotBeApproved(t *testing.T) {
	fixture := newConsoleFixture(t)
	cookie, csrf := fixture.login(t)
	publicKey, _, err := ed25519.GenerateKey(rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	proof := make([]byte, 32)
	if _, err = rand.Read(proof); err != nil {
		t.Fatal(err)
	}
	input := pairing.Request{
		SchemaVersion:   1,
		OperationID:     trust.NewID(),
		Proof:           base64.RawURLEncoding.EncodeToString(proof),
		PersonID:        trust.NewID(),
		DeviceID:        "synthetic-device",
		IssuerKeyID:     trust.NewID(),
		IssuerPublicKey: base64.RawURLEncoding.EncodeToString(publicKey),
	}
	started := fixture.request(http.MethodPost, "/pair/start", input, nil, "", "", "")
	if started.Code != http.StatusOK {
		t.Fatalf("pair start returned %d: %s", started.Code, started.Body.String())
	}
	var pending struct {
		PairingID string `json:"pairing_id"`
		Issuer    struct {
			Fingerprint string `json:"fingerprint"`
		} `json:"issuer"`
	}
	if err = json.Unmarshal(started.Body.Bytes(), &pending); err != nil {
		t.Fatalf("decode pair start: %v", err)
	}
	state := fixture.getState(t, cookie)
	if state.Pairing == nil || state.Pairing.ID != pending.PairingID || state.Pairing.Phase != "pending" {
		t.Fatalf("state did not preserve pending phase: %#v", state.Pairing)
	}
	if containsAction(state.Pairing.AllowedActions, "approve") || !containsAction(state.Pairing.AllowedActions, "reject") {
		t.Fatalf("pending pairing exposed actions %v", state.Pairing.AllowedActions)
	}

	approval := fixture.request(http.MethodPost, "/manage/api/pair/approve", pairing.ApprovalRequest{
		SchemaVersion: 1,
		PairingID:     pending.PairingID,
		Fingerprint:   pending.Issuer.Fingerprint,
	}, cookie, "http://"+fixture.address, csrf, "")
	if approval.Code != http.StatusConflict || responseCode(t, approval) != "pairing_expired" {
		t.Fatalf("approval before local confirmation returned %d with %q", approval.Code, responseCode(t, approval))
	}
}

func containsAction(actions []string, target string) bool {
	for _, action := range actions {
		if action == target {
			return true
		}
	}
	return false
}
