package pairing

import (
	"context"
	"crypto/ed25519"
	"crypto/rand"
	"encoding/base64"
	"encoding/json"
	"errors"
	"reflect"
	"sync"
	"testing"
	"time"

	"floe/server/internal/operation"
	"floe/server/internal/trust"
)

type reservationStore struct {
	mu                                      sync.Mutex
	index                                   string
	attempts                                map[OperationID]string
	indexWrites, attemptWrites, tokenWrites int
	failAttempt                             bool
	failIndexAt                             int
	commitIndexOnFailure                    bool
	commitAttemptOnFailure                  bool
	afterIndex                              func()
}

func (s *reservationStore) ReadReceiptIndex(ctx context.Context) (string, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.index, ctx.Err()
}
func (s *reservationStore) StoreReceiptIndex(ctx context.Context, v string) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	if err := ctx.Err(); err != nil {
		return err
	}
	s.indexWrites++
	if s.indexWrites == s.failIndexAt && !s.commitIndexOnFailure {
		return errors.New("synthetic index write failure")
	}
	s.index = v
	if s.afterIndex != nil {
		s.afterIndex()
	}
	if s.indexWrites == s.failIndexAt {
		return errors.New("synthetic index acknowledgement loss")
	}
	return nil
}
func (s *reservationStore) ReadPairingAttempt(ctx context.Context, id OperationID) (string, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.attempts[id], ctx.Err()
}
func (s *reservationStore) StorePairingAttempt(ctx context.Context, id OperationID, v string) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	if err := ctx.Err(); err != nil {
		return err
	}
	s.attemptWrites++
	if s.failAttempt && !s.commitAttemptOnFailure {
		return errors.New("synthetic attempt write failure")
	}
	if s.attempts == nil {
		s.attempts = map[OperationID]string{}
	}
	s.attempts[id] = v
	if s.failAttempt {
		return errors.New("synthetic attempt acknowledgement loss")
	}
	return nil
}

func TestCanceledReservationRequestReplaysWithoutNewChallenge(t *testing.T) {
	service, store, input, clock := reservationFixture(t)
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	store.afterIndex = cancel
	owner := NewOperations(service, store, clock)
	if _, err := owner.Execute(ctx, "start", input); err == nil {
		t.Fatal("canceled attempt publication succeeded")
	}
	index, err := owner.index(context.Background())
	if err != nil || index.Reservation == nil {
		t.Fatal("cancellation lost complete reservation")
	}
	want := startResult(func() *Pending {
		p, err := pendingFromRecord(*index.Reservation, index.Entries[0])
		if err != nil {
			t.Fatal(err)
		}
		return p
	}())
	store.afterIndex = nil
	got, err := NewOperations(service, store, clock).Execute(context.Background(), "start", input)
	if err != nil || !reflect.DeepEqual(got, want) || service.signs != 1 {
		t.Fatal("cancelled reservation did not replay exactly")
	}
}

func TestMissingUnreservedReceiptIsNeverFabricated(t *testing.T) {
	service, store, input, clock := reservationFixture(t)
	encoded, err := json.Marshal(receiptIndex{Version: 1, Entries: []receiptIndexEntry{{OperationID: input.OperationID, PairingID: trust.NewID()}}})
	if err != nil {
		t.Fatal(err)
	}
	store.index = string(encoded)
	if _, err := NewOperations(service, store, clock).Execute(context.Background(), "start", input); err == nil {
		t.Fatal("missing immutable evidence fabricated")
	}
	if store.index != string(encoded) || store.indexWrites != 0 || service.signs != 0 {
		t.Fatal("unrecoverable old evidence was erased or replaced")
	}
}

func TestReservationPublicationFaultsPreserveExactChallenge(t *testing.T) {
	for _, test := range []struct {
		name                                    string
		indexAt                                 int
		commitIndex, failAttempt, commitAttempt bool
	}{
		{"index_ack_loss", 1, true, false, false},
		{"attempt_ack_loss", 0, false, true, true},
		{"clear_rejected", 2, false, false, false},
		{"clear_ack_loss", 2, true, false, false},
	} {
		t.Run(test.name, func(t *testing.T) {
			service, store, input, clock := reservationFixture(t)
			store.failIndexAt = test.indexAt
			store.commitIndexOnFailure = test.commitIndex
			store.failAttempt = test.failAttempt
			store.commitAttemptOnFailure = test.commitAttempt
			owner := NewOperations(service, store, clock)
			if _, err := owner.Execute(context.Background(), "start", input); err == nil {
				t.Fatal("faulted publication reported success")
			}
			index, err := owner.index(context.Background())
			if err != nil || len(index.Entries) != 1 {
				t.Fatal("published reservation lost")
			}
			p, err := owner.load(context.Background(), index, index.Entries[0])
			if err != nil {
				t.Fatal(err)
			}
			want := startResult(p)
			store.failAttempt = false
			store.failIndexAt = 0
			reopened := NewOperations(service, store, clock)
			got, err := reopened.Execute(context.Background(), "start", input)
			if err != nil || !reflect.DeepEqual(got, want) || service.signs != 1 {
				t.Fatalf("recovery changed the admitted challenge: %v", err)
			}
			changed := input
			changed.DeviceID = "different-device"
			if _, err := reopened.Execute(context.Background(), "start", changed); err == nil {
				t.Fatal("same command accepted changed identity")
			}
			if service.activations != 0 || store.tokenWrites != 0 {
				t.Fatal("reservation activated credentials")
			}
		})
	}
}

func TestOperatorCanRejectReservedOnlyPairingWithoutStartReplay(t *testing.T) {
	service, store, input, clock := reservationFixture(t)
	store.failAttempt = true
	owner := NewOperations(service, store, clock)
	if _, err := owner.Execute(context.Background(), "start", input); err == nil {
		t.Fatal("missing fixture fault")
	}
	index, err := owner.index(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	store.failAttempt = false
	reopened := NewOperations(service, store, clock)
	if _, err := reopened.Reject(context.Background(), trust.OperatorPrincipal{}, RejectionRequest{SchemaVersion: 1, PairingID: index.Entries[0].PairingID}); err != nil {
		t.Fatal(err)
	}
	if p, err := reopened.Pending(context.Background()); err != nil || p != nil {
		t.Fatal("rejected reservation remained pending")
	}
	if _, err := reopened.Execute(context.Background(), "start", input); err != nil {
		t.Fatal(err)
	}
	if p, err := reopened.Pending(context.Background()); err != nil || p != nil {
		t.Fatal("old start replay resurrected rejected reservation")
	}
}

func TestMismatchedAttemptCannotReplaceReservedChallenge(t *testing.T) {
	service, store, input, clock := reservationFixture(t)
	store.failAttempt = true
	owner := NewOperations(service, store, clock)
	if _, err := owner.Execute(context.Background(), "start", input); err == nil {
		t.Fatal("missing fixture fault")
	}
	index, err := owner.index(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	record := *index.Reservation
	record.Pending.Code = "changed"
	encoded, err := json.Marshal(record)
	if err != nil {
		t.Fatal(err)
	}
	store.failAttempt = false
	store.attempts = map[OperationID]string{OperationID(input.OperationID): string(encoded)}
	reopened := NewOperations(service, store, clock)
	if _, err := reopened.Execute(context.Background(), "start", input); err == nil {
		t.Fatal("mismatched attempt overwrote immutable reservation")
	}
	if service.signs != 1 || service.activations != 0 {
		t.Fatal("mismatch caused another challenge or activation")
	}
}
func (s *reservationStore) ReadPairingToken(context.Context, PairingID) (string, error) {
	return "", nil
}
func (s *reservationStore) StorePairingToken(context.Context, PairingID, string) error {
	s.tokenWrites++
	return errors.New("unexpected token write")
}
func (s *reservationStore) DeletePairingToken(context.Context, PairingID) error {
	return errors.New("unexpected token deletion")
}

type reservationTrust struct {
	key                ed25519.PrivateKey
	metadata           trust.ProducerMetadata
	signs, activations int
}

func (s *reservationTrust) PreparePairing(string) (uint64, error)             { return 1, nil }
func (s *reservationTrust) ProducerMetadata() (trust.ProducerMetadata, error) { return s.metadata, nil }
func (s *reservationTrust) SignProducerChallenge(data []byte) ([]byte, error) {
	s.signs++
	return ed25519.Sign(s.key, data), nil
}
func (s *reservationTrust) ReadPairing(context.Context, string, string) (trust.PairingReadback, error) {
	return trust.PairingReadback{}, errors.New("not activated")
}
func (s *reservationTrust) InspectPairing(context.Context, string, string) (trust.PairingReadback, bool, error) {
	return trust.PairingReadback{}, false, nil
}
func (s *reservationTrust) ActivatePairing(context.Context, trust.PairingActivation) (trust.PairingCommit, error) {
	s.activations++
	return trust.PairingCommit{}, errors.New("unexpected activation")
}
func (s *reservationTrust) WithCurrentOperator(_ trust.OperatorPrincipal, f func() error) error {
	return f()
}

func reservationFixture(t *testing.T) (*reservationTrust, *reservationStore, Request, func() time.Time) {
	t.Helper()
	public, key, err := ed25519.GenerateKey(rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	issuer, _, err := ed25519.GenerateKey(rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	service := &reservationTrust{key: key, metadata: trust.ProducerMetadata{SchemaVersion: 1, InstanceID: trust.NewID(), ExecutionOwner: "synthetic-gateway", Audience: "synthetic-audience", KeyID: trust.NewID(), PublicKey: base64.RawURLEncoding.EncodeToString(public), Fingerprint: trust.Digest(string(public))}}
	input := Request{SchemaVersion: 1, OperationID: trust.NewID(), PersonID: trust.NewID(), DeviceID: "synthetic-device", IssuerKeyID: trust.NewID(), IssuerPublicKey: base64.RawURLEncoding.EncodeToString(issuer), Proof: base64.RawURLEncoding.EncodeToString(make([]byte, 32))}
	now := time.Now()
	return service, &reservationStore{}, input, func() time.Time { return now }
}

func TestReservedPairingSurvivesMissingAttemptWrite(t *testing.T) {
	service, store, input, clock := reservationFixture(t)
	store.failAttempt = true
	owner := NewOperations(service, store, clock)
	if _, err := owner.Execute(context.Background(), "start", input); err == nil {
		t.Fatal("failed attempt write returned success")
	}
	index, err := owner.index(context.Background())
	if err != nil || len(index.Entries) != 1 {
		t.Fatal("reservation was not retained")
	}
	pairID := index.Entries[0].PairingID
	store.failAttempt = false
	reopened := NewOperations(service, store, clock)
	writes := store.indexWrites + store.attemptWrites
	pending, err := reopened.Pending(context.Background())
	if err != nil || pending == nil || pending.ID != pairID {
		t.Fatalf("reserved-only pairing cannot be inspected: %v", err)
	}
	if store.indexWrites+store.attemptWrites != writes {
		t.Fatal("read-only pending query mutated custody")
	}
	result, err := reopened.Execute(context.Background(), "start", input)
	if err != nil {
		t.Fatalf("same operation could not recover reservation: %v", err)
	}
	start, ok := result.(StartResult)
	if !ok || start.PairingID != pairID || service.signs != 1 {
		t.Fatal("recovery generated a different challenge")
	}
	again, err := reopened.Execute(context.Background(), "start", input)
	if err != nil || !reflect.DeepEqual(again, result) {
		t.Fatal("exact start replay changed")
	}
	other := input
	other.OperationID = trust.NewID()
	_, err = reopened.Execute(context.Background(), "start", other)
	failure, ok := operation.ErrorOf(err)
	if !ok || failure.Category != operation.Limited || failure.Code != "pairing_in_progress" {
		t.Fatalf("new operation did not preserve pending reservation: %v", err)
	}
	if _, err := reopened.Execute(context.Background(), "cancel", Request{SchemaVersion: 1, PairingID: pairID, Proof: input.Proof}); err != nil {
		t.Fatal(err)
	}
	if _, err := reopened.Execute(context.Background(), "start", other); err != nil {
		t.Fatalf("cancelled reservation blocked later pairing: %v", err)
	}
	if service.activations != 0 || store.tokenWrites != 0 {
		t.Fatal("reservation recovery activated credentials")
	}
}
