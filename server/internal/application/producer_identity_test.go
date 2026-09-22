package application

import (
	"crypto/ed25519"
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"net/http"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"

	"floe/server/internal/authorization"
)

func TestProducerIdentitySignsExactEnrollmentChallenge(t *testing.T) {
	fixture := setup(t)
	_, token := fixture.pair()
	identityResponse := fixture.call(http.MethodGet, "/v1/authority/producer", nil, token)
	if identityResponse.Code != http.StatusOK {
		t.Fatalf("producer identity: %d %s", identityResponse.Code, identityResponse.Body.String())
	}
	var identity map[string]any
	if err := json.Unmarshal(identityResponse.Body.Bytes(), &identity); err != nil {
		t.Fatal(err)
	}
	publicKey, err := base64.RawURLEncoding.DecodeString(identity["public_key"].(string))
	if err != nil || len(publicKey) != ed25519.PublicKeySize {
		t.Fatal("invalid producer public key")
	}
	digest := sha256.Sum256(publicKey)
	if identity["fingerprint"] != hex.EncodeToString(digest[:]) {
		t.Fatal("producer fingerprint mismatch")
	}
	ownerPrivate := ed25519.NewKeyFromSeed([]byte("01234567890123456789012345678901"))
	begin := fixture.call(http.MethodPost, "/v1/authority/enrollment/begin", map[string]any{
		"key_id":     "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
		"public_key": base64.RawURLEncoding.EncodeToString(ownerPrivate.Public().(ed25519.PublicKey)),
		"audience":   "caller-controlled-value",
	}, token)
	if begin.Code != http.StatusOK {
		t.Fatalf("begin: %d %s", begin.Code, begin.Body.String())
	}
	var enrollment map[string]any
	if err := json.Unmarshal(begin.Body.Bytes(), &enrollment); err != nil {
		t.Fatal(err)
	}
	if enrollment["audience"] != identity["audience"] || enrollment["producer_key_id"] != identity["key_id"] || enrollment["producer_fingerprint"] != identity["fingerprint"] {
		t.Fatal("enrollment was not bound to producer identity")
	}
	challenge, err := base64.RawURLEncoding.DecodeString(enrollment["challenge_b64url"].(string))
	if err != nil {
		t.Fatal(err)
	}
	producerSignature, err := base64.RawURLEncoding.DecodeString(enrollment["producer_signature"].(string))
	if err != nil || !ed25519.Verify(ed25519.PublicKey(publicKey), append([]byte("floe.remote.producer.v1\x00"), challenge...), producerSignature) {
		t.Fatal("producer signature did not verify exact challenge bytes")
	}
	if err := authorization.ParseChallengeBytes(challenge); err != nil {
		t.Fatalf("challenge was not strict authorization wire: %v", err)
	}
}

func TestProducerIdentityRejectsSeedCorruption(t *testing.T) {
	path := filepath.Join(t.TempDir(), "producer.json")
	if _, err := loadProducerIdentity(path, true); err != nil {
		t.Fatal(err)
	}
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	var record map[string]json.RawMessage
	if err := json.Unmarshal(data, &record); err != nil {
		t.Fatal(err)
	}
	var encodedKey string
	if err := json.Unmarshal(record["private_key"], &encodedKey); err != nil {
		t.Fatal(err)
	}
	privateKey, err := base64.RawURLEncoding.DecodeString(encodedKey)
	if err != nil {
		t.Fatal(err)
	}
	privateKey[0] ^= 1
	record["private_key"], err = json.Marshal(base64.RawURLEncoding.EncodeToString(privateKey))
	if err != nil {
		t.Fatal(err)
	}
	corrupt, err := json.Marshal(record)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, corrupt, 0o600); err != nil {
		t.Fatal(err)
	}
	if _, err := loadProducerIdentity(path, false); err == nil {
		t.Fatal("seed-corrupted producer identity was accepted")
	}
}

func TestSharedProducerFixtureRejectsNegativeCases(t *testing.T) {
	_, filename, _, _ := runtime.Caller(0)
	fixturePath := filepath.Join(filepath.Dir(filename), "../../..", "fixtures/remote-authorization/producer-v1.json")
	data, err := os.ReadFile(fixturePath)
	if err != nil {
		t.Fatal(err)
	}
	var fixture struct {
		Challenge string `json:"challenge_bytes"`
		PublicKey string `json:"public_key_b64url"`
		Signature string `json:"signature_b64url"`
		Negative  []struct {
			Name      string `json:"name"`
			Signature string `json:"signature_b64url"`
			Suffix    string `json:"challenge_suffix"`
			Mutation  string `json:"mutation"`
			Replace   string `json:"replacement"`
		} `json:"negative"`
	}
	if err := json.Unmarshal(data, &fixture); err != nil {
		t.Fatal(err)
	}
	publicKey, _ := base64.RawURLEncoding.DecodeString(fixture.PublicKey)
	positiveSignature, _ := base64.RawURLEncoding.DecodeString(fixture.Signature)
	challenge := []byte(fixture.Challenge)
	if err := authorization.ParseChallengeBytes(challenge); err != nil {
		t.Fatalf("shared positive producer fixture wire failed: %v", err)
	}
	if !ed25519.Verify(publicKey, append([]byte("floe.remote.producer.v1\x00"), challenge...), positiveSignature) {
		t.Fatal("shared positive producer fixture signature failed")
	}
	for _, negative := range fixture.Negative {
		t.Run(negative.Name, func(t *testing.T) {
			candidate := append([]byte(nil), challenge...)
			signature := positiveSignature
			if negative.Signature != "" {
				signature, _ = base64.RawURLEncoding.DecodeString(negative.Signature)
			}
			if negative.Suffix != "" {
				candidate = append(candidate, []byte(negative.Suffix)...)
			}
			if negative.Mutation != "" {
				candidate = []byte(strings.Replace(string(candidate), negative.Mutation, negative.Replace, 1))
			}
			validWire := authorization.ParseChallengeBytes(candidate) == nil
			validSignature := ed25519.Verify(publicKey, append([]byte("floe.remote.producer.v1\x00"), candidate...), signature)
			if validWire && validSignature {
				t.Fatal("negative producer fixture accepted")
			}
		})
	}
}
