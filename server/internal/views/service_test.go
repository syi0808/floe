package views_test

import (
	"bytes"
	"context"
	"crypto/ed25519"
	"crypto/rand"
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"reflect"
	"runtime"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"floe/server/internal/adapters/credentials"
	storageadapter "floe/server/internal/adapters/storage"
	"floe/server/internal/adapters/storage/privatefiles"
	"floe/server/internal/authority"
	sourcecontract "floe/server/internal/contracts/source"
	"floe/server/internal/pairing"
	"floe/server/internal/trust"
	"floe/server/internal/views"
	viewcontracts "floe/server/internal/views/contracts"
)

const calendarQuery = `{"limit":1,"cursor":"","range_end_unix_ms":2000,"range_start_unix_ms":1000}`

type testClock struct {
	mu        sync.Mutex
	now       time.Time
	monotonic time.Duration
}

func (clock *testClock) Now() time.Time {
	clock.mu.Lock()
	defer clock.mu.Unlock()
	return clock.now
}

func (clock *testClock) Monotonic() time.Duration {
	clock.mu.Lock()
	defer clock.mu.Unlock()
	return clock.monotonic
}

func (clock *testClock) Advance(duration time.Duration) {
	clock.mu.Lock()
	clock.now = clock.now.Add(duration)
	clock.monotonic += duration
	clock.mu.Unlock()
}

type memoryCredentials struct {
	mu     sync.Mutex
	values map[string]string
}

func newMemoryCredentials() *memoryCredentials {
	return &memoryCredentials{values: map[string]string{}}
}

func (store *memoryCredentials) Get(ctx context.Context, name string) (string, error) {
	if err := ctx.Err(); err != nil {
		return "", err
	}
	store.mu.Lock()
	defer store.mu.Unlock()
	return store.values[name], nil
}

func (store *memoryCredentials) Put(ctx context.Context, name, value string) error {
	if err := ctx.Err(); err != nil {
		return err
	}
	store.mu.Lock()
	store.values[name] = value
	store.mu.Unlock()
	return nil
}

func (store *memoryCredentials) Delete(ctx context.Context, name string) error {
	if err := ctx.Err(); err != nil {
		return err
	}
	store.mu.Lock()
	delete(store.values, name)
	store.mu.Unlock()
	return nil
}

type enrollmentStart struct {
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

type enrolledClient struct {
	bearer     string
	privateKey ed25519.PrivateKey
	personID   string
	start      enrollmentStart
}

func enrollTestClient(t *testing.T, service *trust.Service, store credentials.Store, personID, deviceID string) enrolledClient {
	t.Helper()
	publicKey, privateKey, err := ed25519.GenerateKey(rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	proofBytes := make([]byte, 32)
	if _, err := rand.Read(proofBytes); err != nil {
		t.Fatal(err)
	}
	request := pairing.Request{
		SchemaVersion:   1,
		OperationID:     trust.NewID(),
		Proof:           base64.RawURLEncoding.EncodeToString(proofBytes),
		PersonID:        personID,
		DeviceID:        deviceID,
		IssuerKeyID:     trust.NewID(),
		IssuerPublicKey: base64.RawURLEncoding.EncodeToString(publicKey),
	}
	operations := pairing.NewOperations(service, credentials.NewPairingAccess(store), nil)
	started := operations.Execute(context.Background(), "start", request)
	if started.Code != "" {
		t.Fatalf("synthetic pairing start failed: %s", started.Code)
	}
	encoded, err := json.Marshal(started.Value)
	if err != nil {
		t.Fatal(err)
	}
	var start enrollmentStart
	if err = json.Unmarshal(encoded, &start); err != nil {
		t.Fatal(err)
	}
	challenge, err := base64.RawURLEncoding.DecodeString(start.ChallengeB64URL)
	if err != nil {
		t.Fatalf("decode enrollment challenge: %v", err)
	}
	signature := ed25519.Sign(privateKey, append([]byte(trust.SignatureDomain), challenge...))
	confirmed := operations.Execute(context.Background(), "confirm", pairing.Request{
		SchemaVersion: 1, PairingID: start.PairingID, Proof: start.Proof,
		ChallengeID: start.ChallengeID, KeyID: request.IssuerKeyID,
		Signature: base64.RawURLEncoding.EncodeToString(signature),
	})
	if confirmed.Code != "" {
		t.Fatalf("synthetic pairing confirmation failed: %s", confirmed.Code)
	}
	return enrolledClient{privateKey: privateKey, personID: personID, start: start}
}

func approveTestClient(t *testing.T, service *trust.Service, store credentials.Store, adminToken string, client enrolledClient) enrolledClient {
	t.Helper()
	operations := pairing.NewOperations(service, credentials.NewPairingAccess(store), nil)
	session, login := service.LoginOperator(adminToken)
	if login.Code != "" {
		t.Fatalf("synthetic administrator login failed: %s", login.Code)
	}
	sessionData, ok := service.OperatorSession(session)
	if !ok {
		t.Fatal("synthetic administrator session was not created")
	}
	operator, err := service.AuthenticateOperatorSession(context.Background(), session, sessionData.CSRF, true)
	if err != nil {
		t.Fatalf("authenticate synthetic administrator: %v", err)
	}
	start := client.start
	approved := operations.Approve(context.Background(), operator, pairing.ApprovalRequest{SchemaVersion: 1, PairingID: start.PairingID, Fingerprint: start.Issuer.Fingerprint})
	if approved.Code != "" {
		t.Fatalf("synthetic pairing approval failed: %s", approved.Code)
	}
	poll := operations.Execute(context.Background(), "poll", pairing.Request{SchemaVersion: 1, PairingID: start.PairingID, Proof: start.Proof})
	if poll.Code != "" {
		t.Fatalf("synthetic pairing readback failed: %s", poll.Code)
	}
	encoded, err := json.Marshal(poll.Value)
	if err != nil {
		t.Fatal(err)
	}
	var delivery struct {
		Token string `json:"token"`
	}
	if err = json.Unmarshal(encoded, &delivery); err != nil || delivery.Token == "" {
		t.Fatalf("synthetic pairing did not return its credential: %v", err)
	}
	client.bearer = delivery.Token
	return client
}

func newServiceFixture(t *testing.T, provider *scriptedReader, producer *failingProducerTrust) *serviceFixture {
	t.Helper()
	directory := t.TempDir()
	key := make([]byte, 32)
	if _, err := rand.Read(key); err != nil {
		t.Fatal(err)
	}
	root, err := storage.NewFiles(directory, "views-service-test", key, true)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(root.Close)
	trustFiles, err := root.Scope("trust")
	if err != nil {
		t.Fatal(err)
	}
	trustRepository := storageadapter.NewTrustRepository(trustFiles)
	trustService, err := trust.Open(trustRepository, true)
	if err != nil {
		t.Fatal(err)
	}
	credentialsStore := newMemoryCredentials()
	adminToken, err := trustRepository.ReadAdministratorToken()
	if err != nil {
		t.Fatal(err)
	}
	personID := trust.NewID()
	clientOne := enrollTestClient(t, trustService, credentialsStore, personID, "view-client-one")
	clientOne = approveTestClient(t, trustService, credentialsStore, adminToken, clientOne)
	clientTwo := enrollTestClient(t, trustService, credentialsStore, personID, "view-client-two")
	clientTwo = approveTestClient(t, trustService, credentialsStore, adminToken, clientTwo)
	principalOne, err := trustService.AuthenticateBearer(context.Background(), clientOne.bearer)
	if err != nil {
		t.Fatalf("authenticate first synthetic client: %v", err)
	}
	principalTwo, err := trustService.AuthenticateBearer(context.Background(), clientTwo.bearer)
	if err != nil {
		t.Fatalf("authenticate second synthetic client: %v", err)
	}
	if producer == nil {
		producer = &failingProducerTrust{Service: trustService}
	} else {
		producer.Service = trustService
	}
	clock := &testClock{now: time.Now()}
	engine, err := authority.New(authority.Options{Trust: trustService, Clock: clock})
	if err != nil {
		t.Fatal(err)
	}
	snapshot := sourcecontract.Snapshot{
		SourceReference:    sourcecontract.SourceReference{ConnectorID: "google_calendar", ConnectionID: trust.NewID(), ExecutionOwner: "test-execution-owner", Incarnation: trust.NewID(), Epoch: 1},
		ConnectionRevision: 1, PersonID: personID, ProviderIdentity: "google:synthetic-account", IdentityGeneration: 1,
		Resources: []string{}, Active: true,
		Descriptor: sourcecontract.Descriptor{SchemaVersion: 1, ID: string(views.Calendar), Version: "1.0.0", DataClass: "personal", Retention: "ephemeral", FreshnessTTLMS: 60000, MaxItems: 128, MaxBytes: 1 << 20, ProvenanceRequired: true},
	}
	snapshot.Resources = []string{string(views.Calendar) + ":" + snapshot.ConnectionID}
	sources := &scriptedSources{snapshot: snapshot, reader: provider}
	enforcement, err := authority.NewViewEnforcer(engine, producer, sources)
	if err != nil {
		t.Fatal(err)
	}
	service, err := views.NewServiceWithClock(enforcement, producer, sources, clock)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(service.Close)
	return &serviceFixture{service: service, enforcement: enforcement, engine: engine, trustService: trustService, principalOne: principalOne, principalTwo: principalTwo, keyOne: clientOne.privateKey, keyTwo: clientTwo.privateKey, snapshot: snapshot, sources: sources, provider: provider, clock: clock, producer: producer}
}

type serviceFixture struct {
	service        *views.Service
	enforcement    *authority.ViewEnforcer
	mirror         *views.CalendarMirrorService
	mirrorCounting *countingCalendarMirrorEnforcement
	engine         *authority.Engine
	trustService   *trust.Service
	principalOne   trust.Principal
	principalTwo   trust.Principal
	keyOne         ed25519.PrivateKey
	keyTwo         ed25519.PrivateKey
	snapshot       sourcecontract.Snapshot
	sources        *scriptedSources
	provider       *scriptedReader
	clock          *testClock
	producer       *failingProducerTrust
}

type scriptedSources struct {
	mu         sync.Mutex
	snapshot   sourcecontract.Snapshot
	reader     *scriptedReader
	preflights int
}

func (sources *scriptedSources) ResolveSource(ctx context.Context, principal trust.Principal, target views.SourceTarget) (views.ResolvedSource, error) {
	if err := ctx.Err(); err != nil {
		return views.ResolvedSource{}, err
	}
	sources.mu.Lock()
	defer sources.mu.Unlock()
	resourceMatches := target.ResourceID == sources.snapshot.Resources[0]
	if target.ViewID == views.CalendarMirror {
		resourceMatches = target.ResourceID == string(views.CalendarMirror)+":"+sources.snapshot.ConnectionID
	}
	if target.ViewID != views.ID(sources.snapshot.Descriptor.ID) || target.ConnectorID != sources.snapshot.ConnectorID || target.ConnectionID != sources.snapshot.ConnectionID || !resourceMatches || target.ConnectionRevision != 0 && target.ConnectionRevision != sources.snapshot.ConnectionRevision || principal.PersonID() != sources.snapshot.PersonID {
		return views.ResolvedSource{}, errors.New("source target changed")
	}
	return views.ResolvedSource{Snapshot: sourcecontract.Clone(sources.snapshot), Reader: sources.reader, Limits: views.Bounds{MaxItems: uint32(sources.snapshot.Descriptor.MaxItems), MaxBytes: uint32(sources.snapshot.Descriptor.MaxBytes)}}, nil
}

func (sources *scriptedSources) PreflightSource(ctx context.Context, principal trust.Principal, expected views.SourceSnapshot) error {
	if err := ctx.Err(); err != nil {
		return err
	}
	sources.mu.Lock()
	defer sources.mu.Unlock()
	sources.preflights++
	if principal.PersonID() != sources.snapshot.PersonID || sources.snapshot.DeviceID != "" && principal.DeviceID() != sources.snapshot.DeviceID || !reflect.DeepEqual(sources.snapshot, expected) {
		return errors.New("source changed")
	}
	return nil
}

func (sources *scriptedSources) WithCurrentSource(principal trust.Principal, expected sourcecontract.Snapshot, consume func(sourcecontract.Snapshot) error) error {
	sources.mu.Lock()
	defer sources.mu.Unlock()
	if !principal.Valid() || principal.PersonID() != sources.snapshot.PersonID || sources.snapshot.DeviceID != "" && principal.DeviceID() != sources.snapshot.DeviceID || !reflect.DeepEqual(sources.snapshot, expected) || consume == nil {
		return errors.New("source fence rejected stale identity")
	}
	return consume(sourcecontract.Clone(sources.snapshot))
}

func (sources *scriptedSources) Drift() {
	sources.mu.Lock()
	sources.snapshot.Epoch++
	sources.mu.Unlock()
}

type blockingSourceResolver struct {
	delegate *scriptedSources
	block    atomic.Bool
	started  chan struct{}
	resume   chan struct{}
	once     sync.Once
}

func newBlockingSourceResolver(delegate *scriptedSources) *blockingSourceResolver {
	return &blockingSourceResolver{delegate: delegate, started: make(chan struct{}), resume: make(chan struct{})}
}

func (resolver *blockingSourceResolver) ResolveSource(ctx context.Context, principal trust.Principal, target views.SourceTarget) (views.ResolvedSource, error) {
	return resolver.delegate.ResolveSource(ctx, principal, target)
}

func (resolver *blockingSourceResolver) PreflightSource(ctx context.Context, principal trust.Principal, expected views.SourceSnapshot) error {
	if resolver.block.Swap(false) {
		resolver.once.Do(func() { close(resolver.started) })
		select {
		case <-resolver.resume:
		case <-ctx.Done():
			return ctx.Err()
		}
	}
	return resolver.delegate.PreflightSource(ctx, principal, expected)
}

func (resolver *blockingSourceResolver) Resume() {
	resolver.once.Do(func() { close(resolver.started) })
	close(resolver.resume)
}

type scriptedReader struct {
	calls         atomic.Int32
	blockNext     atomic.Bool
	started       chan struct{}
	startedOne    sync.Once
	finish        chan struct{}
	finishOne     sync.Once
	afterRead     func()
	mirrorMu      sync.Mutex
	mirrorNext    string
	mirrorCursors []string
}

func newScriptedReader() *scriptedReader {
	return &scriptedReader{started: make(chan struct{}, 8), finish: make(chan struct{})}
}

func (reader *scriptedReader) FinishBlockedRead() {
	reader.finishOne.Do(func() { close(reader.finish) })
}

func (reader *scriptedReader) Read(ctx context.Context, request views.ReadRequest) (views.Result, error) {
	call := reader.calls.Add(1)
	if reader.blockNext.Swap(false) {
		reader.startedOne.Do(func() { reader.started <- struct{}{} })
		select {
		case <-ctx.Done():
			return views.Result{}, ctx.Err()
		case <-reader.finish:
		}
	}
	if reader.afterRead != nil {
		reader.afterRead()
	}
	if query := request.Query.Mirror; query != nil {
		reader.mirrorMu.Lock()
		reader.mirrorCursors = append(reader.mirrorCursors, request.ProviderCursor)
		next := reader.mirrorNext
		reader.mirrorMu.Unlock()
		recordID := fmt.Sprintf("mirror-event-%d", call)
		return views.Result{ViewID: views.CalendarMirror, Mirror: &viewcontracts.CalendarMirrorResult{
			CalendarID: query.CalendarID, RangeStartUnixMS: query.RangeStartUnixMS, RangeEndUnixMS: query.RangeEndUnixMS,
			ObservedAtUnixMS: time.Now().UnixMilli(), ExpiresAtUnixMS: time.Now().Add(time.Minute).UnixMilli(),
			Records:    []viewcontracts.CalendarRecord{{CalendarID: query.CalendarID, ExternalID: recordID, ExternalRevision: viewcontracts.CalendarExternalRevision{Kind: "provider_opaque", Value: "revision-" + recordID}, Title: "synthetic event", Schedule: viewcontracts.CalendarSchedule{Kind: "all_day", StartDate: "1970-01-01", EndDateExclusive: "1970-01-02"}}},
			NextCursor: next,
		}}, nil
	}
	query := request.Query.Calendar
	if query == nil {
		return views.Result{}, errors.New("unexpected scripted View")
	}
	now := time.Now()
	return views.Result{ViewID: views.Calendar, Calendar: &views.CalendarView{
		SchemaVersion: 1, ViewID: string(views.Calendar), SourceHandle: "calendar:synthetic-source",
		ObservedAtUnixMS: now.UnixMilli(), ExpiresAtUnixMS: now.Add(time.Minute).UnixMilli(),
		RangeStartUnixMS: query.RangeStartUnixMS, RangeEndUnixMS: query.RangeEndUnixMS,
		CoverageComplete: true, Items: []views.CalendarItem{{EvidenceHandle: "calendar:event-1", UntrustedTitle: "synthetic event", StartsAtUnixMS: query.RangeStartUnixMS, EndsAtUnixMS: query.RangeStartUnixMS + 1000}},
	}}, nil
}

func newMirrorServiceFixture(t *testing.T, provider *scriptedReader) *serviceFixture {
	t.Helper()
	fixture := newServiceFixture(t, provider, nil)
	snapshot := sourcecontract.Clone(fixture.snapshot)
	snapshot.Descriptor.ID = string(views.CalendarMirror)
	snapshot.Resources = []string{"calendar-primary", "calendar-secondary"}
	fixture.sources.mu.Lock()
	fixture.sources.snapshot = snapshot
	fixture.sources.mu.Unlock()
	fixture.snapshot = snapshot
	enforcement, err := authority.NewCalendarMirrorEnforcer(fixture.engine, fixture.producer, fixture.sources)
	if err != nil {
		t.Fatal(err)
	}
	fixture.mirrorCounting = &countingCalendarMirrorEnforcement{CalendarMirrorEnforcement: enforcement}
	fixture.mirror, err = views.NewCalendarMirrorServiceWithClock(fixture.mirrorCounting, fixture.producer, fixture.sources, fixture.clock)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(fixture.mirror.Close)
	return fixture
}

func productCalendarAdmission(t *testing.T, fixture *serviceFixture, cursor string) viewcontracts.ProductCalendarAdmissionRequest {
	t.Helper()
	metadata, err := fixture.producer.ProducerMetadata()
	if err != nil {
		t.Fatal(err)
	}
	issuer, err := fixture.trustService.ActiveIssuer(fixture.principalOne)
	if err != nil {
		t.Fatal(err)
	}
	query, err := json.Marshal(viewcontracts.CalendarMirrorQuery{CalendarID: "calendar-primary", RangeStartUnixMS: 1000, RangeEndUnixMS: 2000, Cursor: cursor, Limit: 1})
	if err != nil {
		t.Fatal(err)
	}
	digest := sha256.Sum256(query)
	claims := viewcontracts.ProductCalendarClaims{
		PersonID: fixture.principalOne.PersonID(), ClientID: fixture.principalOne.ClientID(), DeviceID: fixture.principalOne.DeviceID(),
		Audience: metadata.Audience, ProducerInstance: metadata.InstanceID, ProducerKeyFingerprint: metadata.Fingerprint,
		EnrollmentID: issuer.EnrollmentID, CredentialGeneration: 1, Purpose: "day_refresh", ResultKind: "calendar.mirror",
		RefreshOperationID: trust.NewID(), ReadOperationID: trust.NewID(), PageID: trust.NewID(),
		Source: viewcontracts.CalendarSourceClaims(fixture.snapshot, 1), Resources: append([]string(nil), fixture.snapshot.Resources...),
		Query: query, QuerySHA256: hex.EncodeToString(digest[:]),
		Limits: viewcontracts.CalendarReadLimits{MaxRecords: 10, MaxBytes: 1 << 20, MaxPageRecords: 1, MaxPageBytes: 1 << 20},
	}
	return viewcontracts.ProductCalendarAdmissionRequest{SchemaVersion: 1, Claims: claims, ExpiresAtUnixMS: fixture.clock.Now().Add(50 * time.Second).UnixMilli()}
}

func mirrorProof(t *testing.T, challenge views.CalendarMirrorChallengeResult, key ed25519.PrivateKey) (trust.Proof, []byte) {
	t.Helper()
	raw, err := base64.RawURLEncoding.DecodeString(challenge.Challenge)
	if err != nil {
		t.Fatal(err)
	}
	return signProof(raw, key), raw
}

type failingProducerTrust struct {
	Service  *trust.Service
	failNext atomic.Bool
}

func (producer *failingProducerTrust) ProducerMetadata() (trust.ProducerMetadata, error) {
	return producer.Service.ProducerMetadata()
}

func (producer *failingProducerTrust) SignProducerChallenge(data []byte) ([]byte, error) {
	if producer.failNext.Swap(false) {
		return nil, errors.New("scripted producer signing failure")
	}
	return producer.Service.SignProducerChallenge(data)
}

func (producer *failingProducerTrust) FailNextSignature() { producer.failNext.Store(true) }

type stageFailingEnforcement struct {
	views.Enforcement
	failNext atomic.Bool
}

type countingEnforcement struct {
	views.Enforcement
	admissionCancels atomic.Int32
	releaseCancels   atomic.Int32
}

type countingCalendarMirrorEnforcement struct {
	viewcontracts.CalendarMirrorEnforcement
	admissionCancels atomic.Int32
	releaseCancels   atomic.Int32
}

func (enforcement *countingCalendarMirrorEnforcement) CancelCalendarMirrorAdmission(id string) {
	enforcement.admissionCancels.Add(1)
	enforcement.CalendarMirrorEnforcement.CancelCalendarMirrorAdmission(id)
}

func (enforcement *countingCalendarMirrorEnforcement) CancelCalendarMirrorRelease(id string) {
	enforcement.releaseCancels.Add(1)
	enforcement.CalendarMirrorEnforcement.CancelCalendarMirrorRelease(id)
}

func (enforcement *countingEnforcement) CancelViewAdmission(id string) {
	enforcement.admissionCancels.Add(1)
	enforcement.Enforcement.CancelViewAdmission(id)
}

func (enforcement *countingEnforcement) CancelViewRelease(id string) {
	enforcement.releaseCancels.Add(1)
	enforcement.Enforcement.CancelViewRelease(id)
}

func (enforcement *stageFailingEnforcement) StageViewResult(id string, principal trust.Principal, result []byte, count uint32) (string, []byte, time.Time, error) {
	if enforcement.failNext.Swap(false) {
		return "", nil, time.Time{}, errors.New("scripted staging failure")
	}
	return enforcement.Enforcement.StageViewResult(id, principal, result, count)
}

type alteredAdmissionBinding struct {
	views.Enforcement
	wrongDigest bool
}

type operationGate struct {
	started    chan string
	resume     chan struct{}
	blockNext  atomic.Bool
	resumeOnce sync.Once
}

func newOperationGate() *operationGate {
	gate := &operationGate{started: make(chan string, 1), resume: make(chan struct{})}
	gate.blockNext.Store(true)
	return gate
}

func (gate *operationGate) block(id string) {
	if gate.blockNext.Swap(false) {
		gate.started <- id
		<-gate.resume
	}
}

func (gate *operationGate) open() { gate.resumeOnce.Do(func() { close(gate.resume) }) }

type blockingIssuerTrust struct {
	authority.Trust
	gate *operationGate
}

func (trustService *blockingIssuerTrust) WithActiveIssuer(principal trust.Principal, keyID string, consume func(trust.IssuerSnapshot) error) error {
	trustService.gate.block(keyID)
	return trustService.Trust.WithActiveIssuer(principal, keyID, consume)
}

type blockingIssueEnforcement struct {
	views.Enforcement
	gate *operationGate
}

func (enforcement *blockingIssueEnforcement) IssueViewAdmission(principal trust.Principal, snapshot sourcecontract.Snapshot, purpose, consumer, grantID, grantIncarnation string, grantEpoch uint64, resources []string, digest [32]byte, bounds sourcecontract.Bounds) (string, []byte, time.Time, error) {
	id, challenge, expires, err := enforcement.Enforcement.IssueViewAdmission(principal, snapshot, purpose, consumer, grantID, grantIncarnation, grantEpoch, resources, digest, bounds)
	if err == nil {
		enforcement.gate.block(id)
	}
	return id, challenge, expires, err
}

type blockingStageEnforcement struct {
	views.Enforcement
	gate *operationGate
}

func (enforcement *blockingStageEnforcement) StageViewResult(id string, principal trust.Principal, result []byte, count uint32) (string, []byte, time.Time, error) {
	releaseID, challenge, expires, err := enforcement.Enforcement.StageViewResult(id, principal, result, count)
	if err == nil {
		enforcement.gate.block(releaseID)
	}
	return releaseID, challenge, expires, err
}

type blockingMirrorIssueEnforcement struct {
	viewcontracts.CalendarMirrorEnforcement
	gate *operationGate
}

func (enforcement *blockingMirrorIssueEnforcement) IssueCalendarMirrorAdmission(principal trust.Principal, snapshot sourcecontract.Snapshot, claims viewcontracts.ProductCalendarClaims, expires time.Time) (string, []byte, time.Time, error) {
	id, challenge, challengeExpires, err := enforcement.CalendarMirrorEnforcement.IssueCalendarMirrorAdmission(principal, snapshot, claims, expires)
	if err == nil {
		enforcement.gate.block(id)
	}
	return id, challenge, challengeExpires, err
}

func installFixtureAuthority(t *testing.T, fixture *serviceFixture, issuer authority.Trust, random func([]byte) error) {
	t.Helper()
	engine, err := authority.New(authority.Options{Trust: issuer, Clock: fixture.clock, Random: random})
	if err != nil {
		t.Fatal(err)
	}
	enforcement, err := authority.NewViewEnforcer(engine, fixture.producer, fixture.sources)
	if err != nil {
		t.Fatal(err)
	}
	installFixtureService(t, fixture, enforcement)
	fixture.engine = engine
	fixture.enforcement = enforcement
}

func installFixtureService(t *testing.T, fixture *serviceFixture, enforcement views.Enforcement) {
	t.Helper()
	fixture.service.Close()
	service, err := views.NewServiceWithClock(enforcement, fixture.producer, fixture.sources, fixture.clock)
	if err != nil {
		t.Fatal(err)
	}
	fixture.service = service
	t.Cleanup(service.Close)
}

func sequenceChallengeIDs(ids ...string) func([]byte) error {
	var mu sync.Mutex
	index := 0
	return func(dst []byte) error {
		if len(dst) != 16 {
			_, err := rand.Read(dst)
			return err
		}
		mu.Lock()
		defer mu.Unlock()
		if index >= len(ids) {
			return errors.New("test challenge ID sequence exhausted")
		}
		raw, err := hex.DecodeString(strings.ReplaceAll(ids[index], "-", ""))
		index++
		if err != nil || len(raw) != len(dst) {
			return errors.New("test challenge ID is not a UUID")
		}
		copy(dst, raw)
		return nil
	}
}

func awaitOperationGate(t *testing.T, gate *operationGate) string {
	t.Helper()
	select {
	case id := <-gate.started:
		return id
	case <-time.After(3 * time.Second):
		t.Fatal("scripted operation did not reach its barrier")
		return ""
	}
}

func awaitViewServiceClosed(t *testing.T, service *views.Service, principal trust.Principal) {
	t.Helper()
	deadline := time.NewTimer(3 * time.Second)
	defer deadline.Stop()
	for {
		_, err := service.Preview(context.Background(), principal, string(views.Calendar), views.PreviewRequest{})
		var viewError views.Error
		if errors.As(err, &viewError) && viewError.Code == "view_unavailable" {
			return
		}
		select {
		case <-deadline.C:
			t.Fatalf("Close did not fence new View work; last error: %v", err)
		default:
			runtime.Gosched()
		}
	}
}

func awaitMirrorServiceClosed(t *testing.T, service *views.CalendarMirrorService, principal trust.Principal) {
	t.Helper()
	deadline := time.NewTimer(3 * time.Second)
	defer deadline.Stop()
	for {
		_, err := service.Preview(context.Background(), principal, viewcontracts.ProductCalendarPreviewRequest{})
		var viewError views.Error
		if errors.As(err, &viewError) && viewError.Code == "view_unavailable" {
			return
		}
		select {
		case <-deadline.C:
			t.Fatalf("Mirror Close did not fence new work; last error: %v", err)
		default:
			runtime.Gosched()
		}
	}
}

func (enforcement *alteredAdmissionBinding) ClaimViewAdmission(principal trust.Principal, viewID sourcecontract.ID, proof trust.Proof) (string, sourcecontract.Snapshot, [32]byte, sourcecontract.Bounds, error) {
	id, snapshot, digest, bounds, err := enforcement.Enforcement.ClaimViewAdmission(principal, viewID, proof)
	if err == nil && enforcement.wrongDigest {
		digest[0] ^= 0xff
	}
	return id, snapshot, digest, bounds, err
}

func requestAdmission(fixture *serviceFixture, principal trust.Principal, query []byte) (views.ChallengeResult, trust.Proof, []byte, error) {
	input := views.AdmissionRequest{
		SchemaVersion: 1, ConnectorID: fixture.snapshot.ConnectorID, ConnectionID: fixture.snapshot.ConnectionID,
		ConnectionRevision: fixture.snapshot.ConnectionRevision, Resources: append([]string(nil), fixture.snapshot.Resources...),
		Grant:   views.GrantReference{ID: trust.NewID(), Incarnation: trust.NewID(), Epoch: 1},
		Purpose: "everyday_assistance", Consumer: "manager", MaxItems: 1, MaxBytes: 16_384, Query: append([]byte(nil), query...),
	}
	challenge, err := fixture.service.Admit(context.Background(), principal, string(views.Calendar), input)
	if err != nil {
		return views.ChallengeResult{}, trust.Proof{}, nil, err
	}
	raw, err := base64.RawURLEncoding.DecodeString(challenge.Challenge)
	if err != nil {
		return views.ChallengeResult{}, trust.Proof{}, nil, err
	}
	key := fixture.keyOne
	if principal.ClientID() == fixture.principalTwo.ClientID() {
		key = fixture.keyTwo
	}
	return challenge, signProof(raw, key), raw, nil
}

func signProof(raw []byte, privateKey ed25519.PrivateKey) trust.Proof {
	var challenge struct {
		ChallengeID string `json:"challenge_id"`
		KeyID       string `json:"key_id"`
	}
	_ = json.Unmarshal(raw, &challenge)
	signature := ed25519.Sign(privateKey, append([]byte(trust.SignatureDomain), raw...))
	return trust.Proof{ChallengeID: challenge.ChallengeID, KeyID: challenge.KeyID, Signature: base64.RawURLEncoding.EncodeToString(signature)}
}

func failure(t *testing.T, err error) views.Error {
	t.Helper()
	if err == nil {
		t.Fatal("expected View failure")
	}
	var viewFailure views.Error
	if !errors.As(err, &viewFailure) {
		t.Fatalf("expected typed View error, got %T: %v", err, err)
	}
	return viewFailure
}

func TestViewPreviewAdmissionReadStageReleaseAndReplay(t *testing.T) {
	provider := newScriptedReader()
	fixture := newServiceFixture(t, provider, nil)
	preview, err := fixture.service.Preview(context.Background(), fixture.principalOne, string(views.Calendar), views.PreviewRequest{
		ConnectorID: fixture.snapshot.ConnectorID, ConnectionID: fixture.snapshot.ConnectionID, Resource: fixture.snapshot.Resources[0],
	})
	if err != nil || preview.Descriptor == "" || preview.Signature == "" {
		t.Fatalf("source preview failed: %#v, %v", preview, err)
	}

	challenge, proof, raw, err := requestAdmission(fixture, fixture.principalOne, []byte(calendarQuery))
	if err != nil {
		t.Fatal(err)
	}
	var signed struct {
		QuerySHA256 string `json:"query_sha256"`
	}
	if err = json.Unmarshal(raw, &signed); err != nil {
		t.Fatal(err)
	}
	digest := sha256.Sum256([]byte(calendarQuery))
	if signed.QuerySHA256 != hex.EncodeToString(digest[:]) {
		t.Fatalf("authority signed a digest other than the exact validated Rust query bytes: %q", signed.QuerySHA256)
	}
	if challenge.Operation != "admission" {
		t.Fatalf("unexpected admission operation %q", challenge.Operation)
	}

	release, err := fixture.service.Read(context.Background(), fixture.principalOne, string(views.Calendar), proof)
	if err != nil || release.Operation != "release" || provider.calls.Load() != 1 {
		t.Fatalf("bounded read/stage failed: release=%#v calls=%d err=%v", release, provider.calls.Load(), err)
	}
	fixture.sources.mu.Lock()
	preflightsAfterRead := fixture.sources.preflights
	fixture.sources.mu.Unlock()
	if preflightsAfterRead != 5 {
		t.Fatalf("Preview/Admit/Read source preflights=%d, want 5 including both sides of the read", preflightsAfterRead)
	}
	if _, err = fixture.service.Read(context.Background(), fixture.principalOne, string(views.Calendar), proof); failure(t, err).Code != "admission_denied" {
		t.Fatal("admission replay was not rejected")
	}
	if provider.calls.Load() != 1 {
		t.Fatalf("admission replay dispatched a second provider read: %d", provider.calls.Load())
	}
	releaseBytes, err := base64.RawURLEncoding.DecodeString(release.Challenge)
	if err != nil {
		t.Fatal(err)
	}
	released, err := fixture.service.Release(context.Background(), fixture.principalOne, string(views.Calendar), signProof(releaseBytes, fixture.keyOne))
	if err != nil || released.View.Calendar == nil || len(released.View.Calendar.Items) != 1 || released.View.Calendar.Items[0].UntrustedTitle != "synthetic event" {
		t.Fatalf("one-use release failed: %#v, %v", released, err)
	}
	fixture.sources.mu.Lock()
	preflightsAfterRelease := fixture.sources.preflights
	fixture.sources.mu.Unlock()
	if preflightsAfterRelease != 6 {
		t.Fatalf("source preflights after Release=%d, want 6", preflightsAfterRelease)
	}
	if _, err = fixture.service.Release(context.Background(), fixture.principalOne, string(views.Calendar), signProof(releaseBytes, fixture.keyOne)); failure(t, err).Code != "release_denied" {
		t.Fatal("release replay was not rejected")
	}
	if provider.calls.Load() != 1 {
		t.Fatalf("release replay dispatched another read: %d", provider.calls.Load())
	}
}

func TestViewWrongPrincipalDoesNotConsumeAdmission(t *testing.T) {
	provider := newScriptedReader()
	fixture := newServiceFixture(t, provider, nil)
	_, proof, _, err := requestAdmission(fixture, fixture.principalOne, []byte(calendarQuery))
	if err != nil {
		t.Fatal(err)
	}
	if _, err = fixture.service.Read(context.Background(), fixture.principalTwo, string(views.Calendar), proof); failure(t, err).Code != "admission_denied" {
		t.Fatal("wrong principal was not rejected")
	}
	release, err := fixture.service.Read(context.Background(), fixture.principalOne, string(views.Calendar), proof)
	if err != nil || release.Operation != "release" || provider.calls.Load() != 1 {
		t.Fatalf("wrong principal consumed the valid admission: release=%#v calls=%d err=%v", release, provider.calls.Load(), err)
	}
	releaseBytes, err := base64.RawURLEncoding.DecodeString(release.Challenge)
	if err != nil {
		t.Fatal(err)
	}
	if _, err = fixture.service.Release(context.Background(), fixture.principalTwo, string(views.Calendar), signProof(releaseBytes, fixture.keyTwo)); failure(t, err).Code != "release_denied" {
		t.Fatal("wrong principal consumed the valid release")
	}
	if _, err = fixture.service.Release(context.Background(), fixture.principalOne, string(views.Calendar), signProof(releaseBytes, fixture.keyOne)); err != nil {
		t.Fatalf("wrong principal consumed the valid release: %v", err)
	}
}

func TestViewWrongSourceIsFencedBeforeRead(t *testing.T) {
	provider := newScriptedReader()
	fixture := newServiceFixture(t, provider, nil)
	_, proof, _, err := requestAdmission(fixture, fixture.principalOne, []byte(calendarQuery))
	if err != nil {
		t.Fatal(err)
	}
	fixture.sources.Drift()
	if _, err = fixture.service.Read(context.Background(), fixture.principalOne, string(views.Calendar), proof); failure(t, err).Code != "admission_denied" {
		t.Fatal("source drift did not reject the admission")
	}
	if provider.calls.Load() != 0 {
		t.Fatalf("stale source dispatched a provider read: %d", provider.calls.Load())
	}
}

func TestViewWrongQueryProofIsRejected(t *testing.T) {
	provider := newScriptedReader()
	fixture := newServiceFixture(t, provider, nil)
	_, _, raw, err := requestAdmission(fixture, fixture.principalOne, []byte(calendarQuery))
	if err != nil {
		t.Fatal(err)
	}
	var signed struct {
		QuerySHA256 string `json:"query_sha256"`
	}
	if err = json.Unmarshal(raw, &signed); err != nil {
		t.Fatal(err)
	}
	wrong := bytes.Replace(raw, []byte(signed.QuerySHA256), []byte(fmt.Sprintf("%064x", 7)), 1)
	wrongProof := signProof(wrong, fixture.keyOne)
	if _, err = fixture.service.Read(context.Background(), fixture.principalOne, string(views.Calendar), wrongProof); failure(t, err).Code != "admission_denied" {
		t.Fatal("proof with a changed query digest was not rejected")
	}
	if provider.calls.Load() != 0 {
		t.Fatalf("wrong-query proof dispatched a provider read: %d", provider.calls.Load())
	}
}

func TestViewInvalidProofFromCorrectPrincipalCanRetry(t *testing.T) {
	provider := newScriptedReader()
	fixture := newServiceFixture(t, provider, nil)
	_, admissionProof, _, err := requestAdmission(fixture, fixture.principalOne, []byte(calendarQuery))
	if err != nil {
		t.Fatal(err)
	}
	invalidAdmissionProof := admissionProof
	invalidAdmissionProof.Signature = "malformed-signature"
	if _, err = fixture.service.Read(context.Background(), fixture.principalOne, string(views.Calendar), invalidAdmissionProof); failure(t, err).Code != "admission_denied" {
		t.Fatal("malformed admission proof was not denied")
	}
	if provider.calls.Load() != 0 {
		t.Fatalf("invalid admission proof dispatched a provider read: %d", provider.calls.Load())
	}
	release, err := fixture.service.Read(context.Background(), fixture.principalOne, string(views.Calendar), admissionProof)
	if err != nil || release.Operation != "release" || provider.calls.Load() != 1 {
		t.Fatalf("valid admission retry failed after bad proof: release=%#v calls=%d err=%v", release, provider.calls.Load(), err)
	}
	releaseBytes, err := base64.RawURLEncoding.DecodeString(release.Challenge)
	if err != nil {
		t.Fatal(err)
	}
	validReleaseProof := signProof(releaseBytes, fixture.keyOne)
	invalidReleaseProof := validReleaseProof
	invalidReleaseProof.Signature = "malformed-signature"
	if _, err = fixture.service.Release(context.Background(), fixture.principalOne, string(views.Calendar), invalidReleaseProof); failure(t, err).Code != "release_denied" {
		t.Fatal("malformed release proof was not denied")
	}
	if _, err = fixture.service.Release(context.Background(), fixture.principalOne, string(views.Calendar), validReleaseProof); err != nil {
		t.Fatalf("valid release retry failed after bad proof: %v", err)
	}
}

func TestViewAdmissionStateMatchesAuthorityBindingBeforeDispatch(t *testing.T) {
	provider := newScriptedReader()
	fixture := newServiceFixture(t, provider, nil)
	wrapped := &alteredAdmissionBinding{Enforcement: fixture.enforcement, wrongDigest: true}
	service, err := views.NewServiceWithClock(wrapped, fixture.producer, fixture.sources, fixture.clock)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(service.Close)
	fixture.service = service
	_, proof, _, err := requestAdmission(fixture, fixture.principalOne, []byte(calendarQuery))
	if err != nil {
		t.Fatal(err)
	}
	if _, err = fixture.service.Read(context.Background(), fixture.principalOne, string(views.Calendar), proof); failure(t, err).Code != "admission_unavailable" {
		t.Fatal("mismatched Authority query binding reached the provider read")
	}
	if provider.calls.Load() != 0 {
		t.Fatalf("mismatched Authority binding dispatched a provider read: %d", provider.calls.Load())
	}
}

func TestViewAdmissionCapacityIsBoundedPerClient(t *testing.T) {
	fixture := newServiceFixture(t, newScriptedReader(), nil)
	for admission := 0; admission < authority.MaxPendingPerClient; admission++ {
		if _, err := fixture.service.Admit(context.Background(), fixture.principalOne, string(views.Calendar), admissionInput(fixture, []byte(calendarQuery))); err != nil {
			t.Fatalf("admission %d unexpectedly failed before per-client capacity: %v", admission+1, err)
		}
	}
	if _, err := fixture.service.Admit(context.Background(), fixture.principalOne, string(views.Calendar), admissionInput(fixture, []byte(calendarQuery))); failure(t, err).Code != "admission_capacity" {
		t.Fatalf("admission beyond per-client bound was not rejected with capacity: %v", err)
	}
}

func TestViewInvalidProofDoesNotLeakAuthorityCapacity(t *testing.T) {
	fixture := newServiceFixture(t, newScriptedReader(), nil)
	for admission := 0; admission < authority.MaxPendingPerClient-1; admission++ {
		if _, err := fixture.service.Admit(context.Background(), fixture.principalOne, string(views.Calendar), admissionInput(fixture, []byte(calendarQuery))); err != nil {
			t.Fatalf("admission %d unexpectedly failed: %v", admission+1, err)
		}
	}
	_, validProof, _, err := requestAdmission(fixture, fixture.principalOne, []byte(calendarQuery))
	if err != nil {
		t.Fatal(err)
	}
	invalidProof := validProof
	invalidProof.Signature = "malformed-signature"
	if _, err = fixture.service.Read(context.Background(), fixture.principalOne, string(views.Calendar), invalidProof); failure(t, err).Code != "admission_denied" {
		t.Fatal("malformed proof was not denied")
	}
	if _, err = fixture.service.Admit(context.Background(), fixture.principalOne, string(views.Calendar), admissionInput(fixture, []byte(calendarQuery))); failure(t, err).Code != "admission_capacity" {
		t.Fatal("invalid proof unexpectedly freed a still-live Authority slot")
	}
	release, err := fixture.service.Read(context.Background(), fixture.principalOne, string(views.Calendar), validProof)
	if err != nil {
		t.Fatalf("valid proof retry failed at the 128-admission boundary: %v", err)
	}
	releaseBytes, err := base64.RawURLEncoding.DecodeString(release.Challenge)
	if err != nil {
		t.Fatal(err)
	}
	if _, err = fixture.service.Release(context.Background(), fixture.principalOne, string(views.Calendar), signProof(releaseBytes, fixture.keyOne)); err != nil {
		t.Fatalf("release after proof retry failed: %v", err)
	}
	if _, err = fixture.service.Admit(context.Background(), fixture.principalOne, string(views.Calendar), admissionInput(fixture, []byte(calendarQuery))); err != nil {
		t.Fatalf("valid retry/release did not free exactly one Authority slot: %v", err)
	}
}

func TestViewExpirySweeperCancelsAbandonedEntries(t *testing.T) {
	fixture := newServiceFixture(t, newScriptedReader(), nil)
	wrapped := &countingEnforcement{Enforcement: fixture.enforcement}
	service, err := views.NewServiceWithClock(wrapped, fixture.producer, fixture.sources, fixture.clock)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(service.Close)
	fixture.service = service
	if _, err = service.Admit(context.Background(), fixture.principalOne, string(views.Calendar), admissionInput(fixture, []byte(calendarQuery))); err != nil {
		t.Fatal(err)
	}
	fixture.clock.Advance(31 * time.Second)
	waitForCount(t, func() int32 { return wrapped.admissionCancels.Load() }, 1)

	_, proof, _, err := requestAdmission(fixture, fixture.principalOne, []byte(calendarQuery))
	if err != nil {
		t.Fatal(err)
	}
	release, err := service.Read(context.Background(), fixture.principalOne, string(views.Calendar), proof)
	if err != nil {
		t.Fatal(err)
	}
	if release.Operation != "release" {
		t.Fatalf("expected staged release, got %q", release.Operation)
	}
	fixture.clock.Advance(31 * time.Second)
	waitForCount(t, func() int32 { return wrapped.releaseCancels.Load() }, 1)
}

func waitForCount(t *testing.T, count func() int32, want int32) {
	t.Helper()
	deadline := time.After(3 * time.Second)
	ticker := time.NewTicker(10 * time.Millisecond)
	defer ticker.Stop()
	for {
		if count() >= want {
			return
		}
		select {
		case <-deadline:
			t.Fatalf("cleanup count=%d, want at least %d", count(), want)
		case <-ticker.C:
		}
	}
}

func TestViewAdmissionExpiryPreventsRead(t *testing.T) {
	provider := newScriptedReader()
	fixture := newServiceFixture(t, provider, nil)
	_, proof, _, err := requestAdmission(fixture, fixture.principalOne, []byte(calendarQuery))
	if err != nil {
		t.Fatal(err)
	}
	fixture.clock.Advance(31 * time.Second)
	if _, err = fixture.service.Read(context.Background(), fixture.principalOne, string(views.Calendar), proof); failure(t, err).Code != "admission_denied" {
		t.Fatal("expired admission was not rejected")
	}
	if provider.calls.Load() != 0 {
		t.Fatalf("expired admission dispatched a provider read: %d", provider.calls.Load())
	}
}

func TestViewReleaseExpiryPreventsRelease(t *testing.T) {
	provider := newScriptedReader()
	fixture := newServiceFixture(t, provider, nil)
	_, admissionProof, _, err := requestAdmission(fixture, fixture.principalOne, []byte(calendarQuery))
	if err != nil {
		t.Fatal(err)
	}
	release, err := fixture.service.Read(context.Background(), fixture.principalOne, string(views.Calendar), admissionProof)
	if err != nil {
		t.Fatal(err)
	}
	fixture.clock.Advance(31 * time.Second)
	releaseBytes, err := base64.RawURLEncoding.DecodeString(release.Challenge)
	if err != nil {
		t.Fatal(err)
	}
	if _, err = fixture.service.Release(context.Background(), fixture.principalOne, string(views.Calendar), signProof(releaseBytes, fixture.keyOne)); failure(t, err).Code != "release_denied" {
		t.Fatal("expired release was accepted")
	}
	if provider.calls.Load() != 1 {
		t.Fatalf("release expiry triggered another provider read: %d", provider.calls.Load())
	}
}

func TestViewSourceDriftAfterReadPreventsStaging(t *testing.T) {
	provider := newScriptedReader()
	fixture := newServiceFixture(t, provider, nil)
	_, proof, _, err := requestAdmission(fixture, fixture.principalOne, []byte(calendarQuery))
	if err != nil {
		t.Fatal(err)
	}
	provider.afterRead = fixture.sources.Drift
	if _, err = fixture.service.Read(context.Background(), fixture.principalOne, string(views.Calendar), proof); failure(t, err).Code != "source_changed" {
		t.Fatal("source drift after provider read was not rejected")
	}
	if provider.calls.Load() != 1 {
		t.Fatalf("unexpected provider call count %d", provider.calls.Load())
	}
}

func TestViewCancellationCleansAdmissionWithoutRedispatch(t *testing.T) {
	provider := newScriptedReader()
	provider.blockNext.Store(true)
	fixture := newServiceFixture(t, provider, nil)
	_, proof, _, err := requestAdmission(fixture, fixture.principalOne, []byte(calendarQuery))
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	result := make(chan error, 1)
	go func() {
		_, readErr := fixture.service.Read(ctx, fixture.principalOne, string(views.Calendar), proof)
		result <- readErr
	}()
	select {
	case <-provider.started:
	case <-time.After(2 * time.Second):
		t.Fatal("scripted provider did not start")
	}
	cancel()
	select {
	case err = <-result:
	case <-time.After(2 * time.Second):
		t.Fatal("cancelled View read did not finish")
	}
	if failure(t, err).Code != "cancelled" {
		t.Fatal("cancelled View read returned the wrong error")
	}
	if _, err = fixture.service.Read(context.Background(), fixture.principalOne, string(views.Calendar), proof); failure(t, err).Code != "admission_denied" {
		t.Fatal("cancelled admission was replayable")
	}
	if provider.calls.Load() != 1 {
		t.Fatalf("cancelled read redispatched: %d", provider.calls.Load())
	}
}

func TestViewConcurrentValidReadsDispatchAtMostOnce(t *testing.T) {
	provider := newScriptedReader()
	provider.blockNext.Store(true)
	fixture := newServiceFixture(t, provider, nil)
	_, proof, _, err := requestAdmission(fixture, fixture.principalOne, []byte(calendarQuery))
	if err != nil {
		t.Fatal(err)
	}
	type readResponse struct {
		challenge views.ChallengeResult
		err       error
	}
	first := make(chan readResponse, 1)
	go func() {
		challenge, readErr := fixture.service.Read(context.Background(), fixture.principalOne, string(views.Calendar), proof)
		first <- readResponse{challenge: challenge, err: readErr}
	}()
	select {
	case <-provider.started:
	case <-time.After(2 * time.Second):
		t.Fatal("first valid read did not reach the scripted provider")
	}
	if _, err = fixture.service.Read(context.Background(), fixture.principalOne, string(views.Calendar), proof); err == nil {
		t.Fatal("concurrent read unexpectedly claimed the same admission")
	}
	if provider.calls.Load() != 1 {
		t.Fatalf("concurrent read dispatched %d provider calls before releasing the first", provider.calls.Load())
	}
	provider.FinishBlockedRead()
	select {
	case response := <-first:
		if response.err != nil || response.challenge.Operation != "release" {
			t.Fatalf("first valid read did not finish once: %#v", response)
		}
	case <-time.After(2 * time.Second):
		t.Fatal("first valid read did not finish after provider release")
	}
	if provider.calls.Load() != 1 {
		t.Fatalf("concurrent read dispatched %d provider calls", provider.calls.Load())
	}
}

func TestViewConcurrentValidReleaseClaimsAreOneUse(t *testing.T) {
	provider := newScriptedReader()
	fixture := newServiceFixture(t, provider, nil)
	blocking := newBlockingSourceResolver(fixture.sources)
	service, err := views.NewServiceWithClock(fixture.enforcement, fixture.producer, blocking, fixture.clock)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(service.Close)
	fixture.service = service
	_, admissionProof, _, err := requestAdmission(fixture, fixture.principalOne, []byte(calendarQuery))
	if err != nil {
		t.Fatal(err)
	}
	challenge, err := service.Read(context.Background(), fixture.principalOne, string(views.Calendar), admissionProof)
	if err != nil {
		t.Fatal(err)
	}
	releaseBytes, err := base64.RawURLEncoding.DecodeString(challenge.Challenge)
	if err != nil {
		t.Fatal(err)
	}
	proof := signProof(releaseBytes, fixture.keyOne)
	blocking.block.Store(true)
	type releaseResponse struct {
		result views.ReleaseResult
		err    error
	}
	first := make(chan releaseResponse, 1)
	go func() {
		result, releaseErr := service.Release(context.Background(), fixture.principalOne, string(views.Calendar), proof)
		first <- releaseResponse{result: result, err: releaseErr}
	}()
	select {
	case <-blocking.started:
	case <-time.After(2 * time.Second):
		t.Fatal("first release did not reach the blocking source preflight")
	}
	if _, err = service.Release(context.Background(), fixture.principalOne, string(views.Calendar), proof); failure(t, err).Code != "release_in_progress" {
		t.Fatal("concurrent valid release was not fenced as in progress")
	}
	blocking.Resume()
	select {
	case response := <-first:
		if response.err != nil || response.result.View.Calendar == nil {
			t.Fatalf("first release did not finish once: %#v", response)
		}
	case <-time.After(2 * time.Second):
		t.Fatal("first release did not finish after source preflight resumed")
	}
	if provider.calls.Load() != 1 {
		t.Fatalf("concurrent release changed provider call count: %d", provider.calls.Load())
	}
}

func TestCalendarMirrorPagesBindContinuationAndReleaseOnce(t *testing.T) {
	provider := newScriptedReader()
	provider.mirrorNext = "provider-page-2"
	fixture := newMirrorServiceFixture(t, provider)

	page1Input := productCalendarAdmission(t, fixture, "")
	admission1, err := fixture.mirror.Admit(context.Background(), fixture.principalOne, page1Input)
	if err != nil {
		t.Fatal(err)
	}
	proof1, _ := mirrorProof(t, admission1, fixture.keyOne)
	if _, err = fixture.mirror.Read(context.Background(), fixture.principalTwo, proof1); failure(t, err).Code != "admission_denied" {
		t.Fatal("wrong principal consumed Calendar Mirror admission")
	}
	if provider.calls.Load() != 0 {
		t.Fatal("wrong principal dispatched Calendar Mirror read")
	}
	release1, err := fixture.mirror.Read(context.Background(), fixture.principalOne, proof1)
	if err != nil || provider.calls.Load() != 1 {
		t.Fatalf("first Mirror page read failed: calls=%d err=%v", provider.calls.Load(), err)
	}
	releaseProof1, _ := mirrorProof(t, release1, fixture.keyOne)
	page1, err := fixture.mirror.Release(context.Background(), fixture.principalOne, releaseProof1)
	if err != nil {
		t.Fatal(err)
	}
	var page1Wire struct {
		Outcome struct {
			State  string `json:"state"`
			Cursor string `json:"cursor"`
		} `json:"outcome"`
	}
	if err = json.Unmarshal(page1.Page, &page1Wire); err != nil || page1Wire.Outcome.State != "more" || !strings.HasPrefix(page1Wire.Outcome.Cursor, "mirror_") {
		t.Fatalf("first page did not return an opaque client cursor: %#v err=%v", page1Wire, err)
	}
	if _, err = fixture.mirror.Read(context.Background(), fixture.principalOne, proof1); failure(t, err).Code != "admission_denied" {
		t.Fatal("Calendar Mirror admission replay was accepted")
	}
	if _, err = fixture.mirror.Release(context.Background(), fixture.principalOne, releaseProof1); failure(t, err).Code != "release_denied" {
		t.Fatal("Calendar Mirror release replay was accepted")
	}

	provider.mirrorMu.Lock()
	provider.mirrorNext = ""
	provider.mirrorMu.Unlock()
	page2Input := page1Input
	page2Input.Claims = viewcontracts.CloneProductCalendarClaims(page1Input.Claims)
	page2Input.Claims.PageID = trust.NewID()
	page2Input.Claims.Query, err = json.Marshal(viewcontracts.CalendarMirrorQuery{CalendarID: "calendar-primary", RangeStartUnixMS: 1000, RangeEndUnixMS: 2000, Cursor: page1Wire.Outcome.Cursor, Limit: 1})
	if err != nil {
		t.Fatal(err)
	}
	digest := sha256.Sum256(page2Input.Claims.Query)
	page2Input.Claims.QuerySHA256 = hex.EncodeToString(digest[:])
	admission2, err := fixture.mirror.Admit(context.Background(), fixture.principalOne, page2Input)
	if err != nil {
		t.Fatalf("continuation admission failed: %v", err)
	}
	proof2, _ := mirrorProof(t, admission2, fixture.keyOne)
	release2, err := fixture.mirror.Read(context.Background(), fixture.principalOne, proof2)
	if err != nil || provider.calls.Load() != 2 {
		t.Fatalf("second Mirror page read failed: calls=%d err=%v", provider.calls.Load(), err)
	}
	provider.mirrorMu.Lock()
	cursors := append([]string(nil), provider.mirrorCursors...)
	provider.mirrorMu.Unlock()
	if !reflect.DeepEqual(cursors, []string{"", "provider-page-2"}) {
		t.Fatalf("provider continuation was not privately preserved: %#v", cursors)
	}
	releaseProof2, _ := mirrorProof(t, release2, fixture.keyOne)
	page2, err := fixture.mirror.Release(context.Background(), fixture.principalOne, releaseProof2)
	if err != nil {
		t.Fatal(err)
	}
	var page2Wire struct {
		Outcome struct {
			State string `json:"state"`
		} `json:"outcome"`
	}
	if err = json.Unmarshal(page2.Page, &page2Wire); err != nil || page2Wire.Outcome.State != "complete" {
		t.Fatalf("second page did not complete the resource: %#v err=%v", page2Wire, err)
	}
	if provider.calls.Load() != 2 {
		t.Fatalf("replay or release dispatched a second provider read: %d", provider.calls.Load())
	}
}

func TestCalendarMirrorQueryProofRetryAndSourceDrift(t *testing.T) {
	provider := newScriptedReader()
	fixture := newMirrorServiceFixture(t, provider)
	input := productCalendarAdmission(t, fixture, "")
	challenge, err := fixture.mirror.Admit(context.Background(), fixture.principalOne, input)
	if err != nil {
		t.Fatal(err)
	}
	validProof, raw := mirrorProof(t, challenge, fixture.keyOne)
	wrongRaw := bytes.Replace(raw, []byte(input.Claims.QuerySHA256), []byte(fmt.Sprintf("%064x", 7)), 1)
	wrongProof := signProof(wrongRaw, fixture.keyOne)
	if _, err = fixture.mirror.Read(context.Background(), fixture.principalOne, wrongProof); failure(t, err).Code != "admission_denied" {
		t.Fatal("proof for a changed query binding was accepted")
	}
	release, err := fixture.mirror.Read(context.Background(), fixture.principalOne, validProof)
	if err != nil || provider.calls.Load() != 1 {
		t.Fatalf("valid admission retry after wrong query proof failed: calls=%d err=%v", provider.calls.Load(), err)
	}
	releaseProof, _ := mirrorProof(t, release, fixture.keyOne)
	if _, err = fixture.mirror.Release(context.Background(), fixture.principalOne, releaseProof); err != nil {
		t.Fatal(err)
	}

	provider2 := newScriptedReader()
	fixture2 := newMirrorServiceFixture(t, provider2)
	provider2.afterRead = fixture2.sources.Drift
	challenge2, err := fixture2.mirror.Admit(context.Background(), fixture2.principalOne, productCalendarAdmission(t, fixture2, ""))
	if err != nil {
		t.Fatal(err)
	}
	proof2, _ := mirrorProof(t, challenge2, fixture2.keyOne)
	if _, err = fixture2.mirror.Read(context.Background(), fixture2.principalOne, proof2); failure(t, err).Code != "source_changed" {
		t.Fatalf("source drift after provider read was not rejected: %v", err)
	}
	if provider2.calls.Load() != 1 {
		t.Fatalf("source drift caused an unexpected dispatch count: %d", provider2.calls.Load())
	}
}

func TestCalendarMirrorSweeperCancelsAbandonedAdmissionAndRelease(t *testing.T) {
	fixture := newMirrorServiceFixture(t, newScriptedReader())
	challenge, err := fixture.mirror.Admit(context.Background(), fixture.principalOne, productCalendarAdmission(t, fixture, ""))
	if err != nil {
		t.Fatal(err)
	}
	fixture.clock.Advance(31 * time.Second)
	waitForCount(t, func() int32 { return fixture.mirrorCounting.admissionCancels.Load() }, 1)

	challenge, err = fixture.mirror.Admit(context.Background(), fixture.principalOne, productCalendarAdmission(t, fixture, ""))
	if err != nil {
		t.Fatal(err)
	}
	proof, _ := mirrorProof(t, challenge, fixture.keyOne)
	_, err = fixture.mirror.Read(context.Background(), fixture.principalOne, proof)
	if err != nil {
		t.Fatal(err)
	}
	fixture.clock.Advance(31 * time.Second)
	waitForCount(t, func() int32 { return fixture.mirrorCounting.releaseCancels.Load() }, 1)
}

func TestViewSigningFailuresCleanAuthorityCapacity(t *testing.T) {
	provider := newScriptedReader()
	producer := &failingProducerTrust{}
	fixture := newServiceFixture(t, provider, producer)
	for attempt := 0; attempt < authority.MaxPendingPerClient+1; attempt++ {
		fixture.producer.FailNextSignature()
		_, err := fixture.service.Admit(context.Background(), fixture.principalOne, string(views.Calendar), admissionInput(fixture, []byte(calendarQuery)))
		if failure(t, err).Code != "producer_unavailable" {
			t.Fatalf("admission signing failure leaked pending authority state by attempt %d", attempt+1)
		}
	}
	for attempt := 0; attempt < authority.MaxPendingPerClient+1; attempt++ {
		challenge, proof, _, err := requestAdmission(fixture, fixture.principalOne, []byte(calendarQuery))
		if err != nil || challenge.Operation != "admission" {
			t.Fatalf("admission failed before cleanup check %d: %v", attempt+1, err)
		}
		fixture.producer.FailNextSignature()
		if _, err = fixture.service.Read(context.Background(), fixture.principalOne, string(views.Calendar), proof); failure(t, err).Code != "producer_unavailable" {
			t.Fatalf("release signing failure leaked staged authority state by attempt %d", attempt+1)
		}
	}
}

func TestViewStagingFailureCleansAdmissionCapacity(t *testing.T) {
	provider := newScriptedReader()
	fixture := newServiceFixture(t, provider, nil)
	wrapped := &stageFailingEnforcement{Enforcement: fixture.enforcement}
	service, err := views.NewServiceWithClock(wrapped, fixture.producer, fixture.sources, fixture.clock)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(service.Close)
	fixture.service = service
	for admission := 0; admission < authority.MaxPendingPerClient-1; admission++ {
		if _, err = fixture.service.Admit(context.Background(), fixture.principalOne, string(views.Calendar), admissionInput(fixture, []byte(calendarQuery))); err != nil {
			t.Fatalf("admission %d unexpectedly failed before staging cleanup case: %v", admission+1, err)
		}
	}
	_, proof, _, err := requestAdmission(fixture, fixture.principalOne, []byte(calendarQuery))
	if err != nil {
		t.Fatal(err)
	}
	wrapped.failNext.Store(true)
	if _, err = fixture.service.Read(context.Background(), fixture.principalOne, string(views.Calendar), proof); failure(t, err).Code != "release_denied" {
		t.Fatal("scripted staging failure returned the wrong View error")
	}
	if _, err = fixture.service.Admit(context.Background(), fixture.principalOne, string(views.Calendar), admissionInput(fixture, []byte(calendarQuery))); err != nil {
		t.Fatalf("failed staging leaked its admission capacity: %v", err)
	}
}

func admissionInput(fixture *serviceFixture, query []byte) views.AdmissionRequest {
	return views.AdmissionRequest{
		SchemaVersion: 1, ConnectorID: fixture.snapshot.ConnectorID, ConnectionID: fixture.snapshot.ConnectionID,
		ConnectionRevision: fixture.snapshot.ConnectionRevision, Resources: append([]string(nil), fixture.snapshot.Resources...),
		Grant: views.GrantReference{ID: trust.NewID(), Incarnation: trust.NewID(), Epoch: 1}, Purpose: "everyday_assistance",
		Consumer: "manager", MaxItems: 1, MaxBytes: 16_384, Query: append([]byte(nil), query...),
	}
}

func TestAuthorityChallengeIDsCannotReplaceLiveAdmissionOrRelease(t *testing.T) {
	fixture := newServiceFixture(t, newScriptedReader(), nil)
	firstID, releaseID, lastID := trust.NewID(), trust.NewID(), trust.NewID()
	installFixtureAuthority(t, fixture, fixture.trustService, sequenceChallengeIDs(firstID, firstID, releaseID, releaseID, lastID))

	first, proof, _, err := requestAdmission(fixture, fixture.principalOne, []byte(calendarQuery))
	if err != nil || first.ChallengeID != firstID {
		t.Fatalf("first challenge issuance failed: id=%q err=%v", first.ChallengeID, err)
	}
	if _, err = fixture.service.Admit(context.Background(), fixture.principalOne, string(views.Calendar), admissionInput(fixture, []byte(calendarQuery))); failure(t, err).Code != "admission_denied" {
		t.Fatal("duplicate live admission ID was not rejected")
	}
	release, err := fixture.service.Read(context.Background(), fixture.principalOne, string(views.Calendar), proof)
	if err != nil {
		t.Fatalf("original admission was overwritten by collision: %v", err)
	}
	if release.ChallengeID != releaseID {
		t.Fatalf("release challenge ID=%q, want %q", release.ChallengeID, releaseID)
	}
	if _, err = fixture.service.Admit(context.Background(), fixture.principalOne, string(views.Calendar), admissionInput(fixture, []byte(calendarQuery))); failure(t, err).Code != "admission_denied" {
		t.Fatal("live release ID was reused for a new admission")
	}
	releaseRaw, err := base64.RawURLEncoding.DecodeString(release.Challenge)
	if err != nil {
		t.Fatal(err)
	}
	if _, err = fixture.service.Release(context.Background(), fixture.principalOne, string(views.Calendar), signProof(releaseRaw, fixture.keyOne)); err != nil {
		t.Fatalf("original release was overwritten by collision: %v", err)
	}
	last, err := fixture.service.Admit(context.Background(), fixture.principalOne, string(views.Calendar), admissionInput(fixture, []byte(calendarQuery)))
	if err != nil || last.ChallengeID != lastID {
		t.Fatalf("admission after exact release cleanup failed: id=%q err=%v", last.ChallengeID, err)
	}
	if calls := fixture.provider.calls.Load(); calls != 1 {
		t.Fatalf("ID collision or replay dispatched %d provider reads, want 1", calls)
	}
}

func TestAuthorityStageIDCannotReplaceLiveAdmission(t *testing.T) {
	fixture := newServiceFixture(t, newScriptedReader(), nil)
	admissionID, nextID := trust.NewID(), trust.NewID()
	installFixtureAuthority(t, fixture, fixture.trustService, sequenceChallengeIDs(admissionID, admissionID, nextID))
	_, proof, _, err := requestAdmission(fixture, fixture.principalOne, []byte(calendarQuery))
	if err != nil {
		t.Fatal(err)
	}
	if _, err = fixture.service.Read(context.Background(), fixture.principalOne, string(views.Calendar), proof); failure(t, err).Code != "release_denied" {
		t.Fatal("stage challenge reused its live admission ID")
	}
	if _, err = fixture.engine.ClaimRelease(context.Background(), fixture.principalOne, sourcecontract.ID(views.Calendar), trust.Proof{ChallengeID: admissionID}, fixture.sources); !errors.Is(err, authority.ErrReplay) {
		t.Fatalf("colliding stage overwrote the live admission ID: %v", err)
	}
	if calls := fixture.provider.calls.Load(); calls != 1 {
		t.Fatalf("stage collision dispatched %d provider reads, want 1", calls)
	}
	if _, err = fixture.service.Admit(context.Background(), fixture.principalOne, string(views.Calendar), admissionInput(fixture, []byte(calendarQuery))); err != nil {
		t.Fatalf("stage collision leaked capacity: %v", err)
	}
}

func TestViewExpiryDuringProofVerificationPreventsRead(t *testing.T) {
	fixture := newServiceFixture(t, newScriptedReader(), nil)
	gate := newOperationGate()
	issuer := &blockingIssuerTrust{Trust: fixture.trustService, gate: gate}
	installFixtureAuthority(t, fixture, issuer, nil)
	t.Cleanup(gate.open)
	_, proof, _, err := requestAdmission(fixture, fixture.principalOne, []byte(calendarQuery))
	if err != nil {
		t.Fatal(err)
	}
	result := make(chan error, 1)
	go func() {
		_, readErr := fixture.service.Read(context.Background(), fixture.principalOne, string(views.Calendar), proof)
		result <- readErr
	}()
	awaitOperationGate(t, gate)
	fixture.clock.Advance(authority.ChallengeTTL + time.Second)
	gate.open()
	select {
	case err = <-result:
	case <-time.After(3 * time.Second):
		t.Fatal("expired proof claim did not finish after the verification barrier opened")
	}
	if failure(t, err).Code != "admission_denied" {
		t.Fatalf("proof that expired during verification returned %v", err)
	}
	if calls := fixture.provider.calls.Load(); calls != 0 {
		t.Fatalf("expired proof dispatched %d provider reads", calls)
	}
	if _, _, err = fixture.engine.ClaimAdmission(fixture.principalOne, sourcecontract.ID(views.Calendar), proof, fixture.sources); !errors.Is(err, authority.ErrReplay) {
		t.Fatalf("expired admission remained claimable: %v", err)
	}
}

func TestViewCloseDrainsCancellationWhileAuthorityClaimIsChecking(t *testing.T) {
	fixture := newServiceFixture(t, newScriptedReader(), nil)
	gate := newOperationGate()
	issuer := &blockingIssuerTrust{Trust: fixture.trustService, gate: gate}
	installFixtureAuthority(t, fixture, issuer, nil)
	t.Cleanup(gate.open)
	_, proof, _, err := requestAdmission(fixture, fixture.principalOne, []byte(calendarQuery))
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	result := make(chan error, 1)
	go func() {
		_, readErr := fixture.service.Read(ctx, fixture.principalOne, string(views.Calendar), proof)
		result <- readErr
	}()
	awaitOperationGate(t, gate)
	cancel()
	closeDone := make(chan struct{})
	service := fixture.service
	go func() { service.Close(); close(closeDone) }()
	awaitViewServiceClosed(t, service, fixture.principalOne)
	select {
	case <-closeDone:
		t.Fatal("Close returned while the authority claim was still checking")
	default:
	}
	gate.open()
	select {
	case err = <-result:
	case <-time.After(3 * time.Second):
		t.Fatal("cancelled authority claim did not finish after the verification barrier opened")
	}
	if failure(t, err).Code != "cancelled" {
		t.Fatalf("cancelled claim returned %v", err)
	}
	select {
	case <-closeDone:
	case <-time.After(3 * time.Second):
		t.Fatal("Close did not drain the cancelled claim")
	}
	if calls := fixture.provider.calls.Load(); calls != 0 {
		t.Fatalf("close/cancel race dispatched %d provider reads", calls)
	}
	if _, _, err = fixture.engine.ClaimAdmission(fixture.principalOne, sourcecontract.ID(views.Calendar), proof, fixture.sources); !errors.Is(err, authority.ErrReplay) {
		t.Fatalf("closed claim left Authority admission state: %v", err)
	}
}

func TestViewClosePreventsInFlightAdmissionFromRepopulatingState(t *testing.T) {
	fixture := newServiceFixture(t, newScriptedReader(), nil)
	gate := newOperationGate()
	wrapped := &blockingIssueEnforcement{Enforcement: fixture.enforcement, gate: gate}
	installFixtureService(t, fixture, wrapped)
	t.Cleanup(gate.open)
	result := make(chan error, 1)
	input := admissionInput(fixture, []byte(calendarQuery))
	go func() {
		_, admitErr := fixture.service.Admit(context.Background(), fixture.principalOne, string(views.Calendar), input)
		result <- admitErr
	}()
	id := awaitOperationGate(t, gate)
	closeDone := make(chan struct{})
	service := fixture.service
	go func() { service.Close(); close(closeDone) }()
	awaitViewServiceClosed(t, service, fixture.principalOne)
	select {
	case <-closeDone:
		t.Fatal("Close returned before its in-flight admission completed")
	default:
	}
	gate.open()
	select {
	case err := <-result:
		if failure(t, err).Code != "cancelled" {
			t.Fatalf("in-flight admission returned %v", err)
		}
	case <-time.After(3 * time.Second):
		t.Fatal("in-flight admission did not stop after close")
	}
	select {
	case <-closeDone:
	case <-time.After(3 * time.Second):
		t.Fatal("Close did not drain the in-flight admission")
	}
	if _, _, err := fixture.engine.ClaimAdmission(fixture.principalOne, sourcecontract.ID(views.Calendar), trust.Proof{ChallengeID: id}, fixture.sources); !errors.Is(err, authority.ErrReplay) {
		t.Fatalf("closed admission remained in Authority: %v", err)
	}
}

func TestViewCloseCancelsInFlightStageWithoutPublishingRelease(t *testing.T) {
	fixture := newServiceFixture(t, newScriptedReader(), nil)
	gate := newOperationGate()
	wrapped := &blockingStageEnforcement{Enforcement: fixture.enforcement, gate: gate}
	installFixtureService(t, fixture, wrapped)
	t.Cleanup(gate.open)
	_, proof, _, err := requestAdmission(fixture, fixture.principalOne, []byte(calendarQuery))
	if err != nil {
		t.Fatal(err)
	}
	result := make(chan error, 1)
	go func() {
		_, readErr := fixture.service.Read(context.Background(), fixture.principalOne, string(views.Calendar), proof)
		result <- readErr
	}()
	releaseID := awaitOperationGate(t, gate)
	closeDone := make(chan struct{})
	service := fixture.service
	go func() { service.Close(); close(closeDone) }()
	awaitViewServiceClosed(t, service, fixture.principalOne)
	select {
	case <-closeDone:
		t.Fatal("Close returned before in-flight staging completed")
	default:
	}
	gate.open()
	select {
	case err = <-result:
		if failure(t, err).Code != "cancelled" {
			t.Fatalf("in-flight staged read returned %v", err)
		}
	case <-time.After(3 * time.Second):
		t.Fatal("in-flight stage did not stop after close")
	}
	select {
	case <-closeDone:
	case <-time.After(3 * time.Second):
		t.Fatal("Close did not drain the in-flight stage")
	}
	if _, err = fixture.engine.ClaimRelease(context.Background(), fixture.principalOne, sourcecontract.ID(views.Calendar), trust.Proof{ChallengeID: releaseID}, fixture.sources); !errors.Is(err, authority.ErrReplay) {
		t.Fatalf("closed stage remained releasable: %v", err)
	}
	if calls := fixture.provider.calls.Load(); calls != 1 {
		t.Fatalf("close stage race dispatched %d provider reads, want 1", calls)
	}
}

func TestCalendarMirrorClosePreventsInFlightAdmissionFromRepopulatingState(t *testing.T) {
	fixture := newMirrorServiceFixture(t, newScriptedReader())
	gate := newOperationGate()
	wrapped := &blockingMirrorIssueEnforcement{CalendarMirrorEnforcement: fixture.mirrorCounting, gate: gate}
	fixture.mirror.Close()
	mirror, err := views.NewCalendarMirrorServiceWithClock(wrapped, fixture.producer, fixture.sources, fixture.clock)
	if err != nil {
		t.Fatal(err)
	}
	fixture.mirror = mirror
	t.Cleanup(mirror.Close)
	t.Cleanup(gate.open)
	result := make(chan error, 1)
	input := productCalendarAdmission(t, fixture, "")
	go func() {
		_, admitErr := fixture.mirror.Admit(context.Background(), fixture.principalOne, input)
		result <- admitErr
	}()
	id := awaitOperationGate(t, gate)
	closeDone := make(chan struct{})
	service := fixture.mirror
	go func() { service.Close(); close(closeDone) }()
	awaitMirrorServiceClosed(t, service, fixture.principalOne)
	select {
	case <-closeDone:
		t.Fatal("Mirror Close returned before in-flight admission completed")
	default:
	}
	gate.open()
	select {
	case err = <-result:
		if failure(t, err).Code != "cancelled" {
			t.Fatalf("in-flight Mirror admission returned %v", err)
		}
	case <-time.After(3 * time.Second):
		t.Fatal("in-flight Mirror admission did not stop after close")
	}
	select {
	case <-closeDone:
	case <-time.After(3 * time.Second):
		t.Fatal("Mirror Close did not drain the in-flight admission")
	}
	if _, _, err = fixture.engine.ClaimAdmission(fixture.principalOne, views.CalendarMirror, trust.Proof{ChallengeID: id}, fixture.sources); !errors.Is(err, authority.ErrReplay) {
		t.Fatalf("closed Mirror admission remained in Authority: %v", err)
	}
	if calls := fixture.provider.calls.Load(); calls != 0 {
		t.Fatalf("closed Mirror admission dispatched %d provider reads", calls)
	}
}
