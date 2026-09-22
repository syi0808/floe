package pairing

import (
	"crypto/ed25519"
	"errors"
	"testing"
	"time"

	"floe/server/internal/authorization"
	"floe/server/internal/operation"
)

func TestPairingSettlementRequiresCurrentConfirmedIdentity(test *testing.T) {
	privateKey := ed25519.NewKeyFromSeed(make([]byte, ed25519.SeedSize))
	publicKey := privateKey.Public().(ed25519.PublicKey)
	pending := Pending{ID: "pair", LocalConfirmed: true, IssuerKeyID: "issuer", IssuerFingerprint: issuerFingerprint(publicKey), ProducerFingerprint: "producer", enrollmentID: "enrollment"}
	record := authorization.IssuerRecord{KeyID: pending.IssuerKeyID, EnrollmentID: pending.enrollmentID, PublicKey: publicKey}
	commits := 0
	writeFailure := errors.New("durable commit unavailable")
	operations := NewOperations(Host{
		Fingerprint: func() string { return "producer" },
		Commit:      func(authorization.IssuerRecord, string, Pending) error { commits++; return writeFailure },
	}, false, nil)
	operations.pending = &pending
	if err := operations.commit(record, "hash", pending); !errors.Is(err, writeFailure) || commits != 1 || operations.pending.token != "" {
		test.Fatal("uncertain persistence issued a credential")
	}
	operations.pending = nil
	if err := operations.commit(record, "hash", pending); !errors.Is(err, authorization.ErrConflict) || commits != 1 {
		test.Fatal("cancelled pairing reached durable settlement")
	}
	for _, mutation := range []func(*Pending){
		func(value *Pending) { value.ID = "foreign" },
		func(value *Pending) { value.LocalConfirmed = false },
		func(value *Pending) { value.IssuerFingerprint = "foreign" },
	} {
		changed := pending
		mutation(&changed)
		operations.pending = &changed
		if err := operations.commit(record, "hash", pending); !errors.Is(err, authorization.ErrConflict) || commits != 1 {
			test.Fatal("changed pairing reached durable settlement")
		}
	}
}

func TestPairingPollBindsProofAndExactPendingIdentity(test *testing.T) {
	now := time.Now()
	operations := NewOperations(Host{}, false, func() time.Time { return now })
	operations.pending = &Pending{ID: "pair", IssuerKeyID: "issuer", proof: "polling-proof", Expires: now.Add(time.Minute)}
	for _, input := range []Request{
		{SchemaVersion: 1, PairingID: "foreign", Proof: "polling-proof"},
		{SchemaVersion: 1, PairingID: "pair", Proof: "foreign"},
	} {
		if result := operations.Execute("poll", input); result.Code == "" {
			test.Fatal("foreign identity or proof observed pairing")
		}
	}
	input := Request{SchemaVersion: 1, PairingID: "pair", Proof: "polling-proof"}
	if result := operations.Execute("poll", input); result.Category != operation.Ready || result.Value.(map[string]any)["status"] != "pending" {
		test.Fatal("exact pending pairing unavailable")
	}
	now = now.Add(2 * time.Minute)
	if result := operations.Execute("poll", input); result.Value.(map[string]any)["status"] != "expired" {
		test.Fatal("expired pairing retained pending status")
	}
}
