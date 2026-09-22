package authorization

import (
	"bytes"
	"crypto/ed25519"
	cryptorand "crypto/rand"
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"errors"
	"io"
)

const producerIdentitySchema = 1

var producerSignatureDomain = []byte("floe.remote.producer.v1\x00")

type producerIdentityRecord struct {
	SchemaVersion int    `json:"schema_version"`
	KeyID         string `json:"key_id"`
	PrivateKey    string `json:"private_key"`
	PublicKey     string `json:"public_key"`
}

type ProducerIdentity struct {
	keyID      string
	privateKey ed25519.PrivateKey
	publicKey  ed25519.PublicKey
}

func DecodeProducerIdentity(data []byte) (*ProducerIdentity, error) {
	if len(data) == 0 || len(data) > 4096 || rejectDuplicateJSON(data) != nil {
		return nil, errors.New("invalid producer identity")
	}
	decoder := json.NewDecoder(bytes.NewReader(data))
	decoder.DisallowUnknownFields()
	var record producerIdentityRecord
	if err := decoder.Decode(&record); err != nil || decoder.Decode(new(any)) != io.EOF {
		return nil, errors.New("invalid producer identity")
	}
	if record.SchemaVersion != producerIdentitySchema || validateUUID(record.KeyID) != nil {
		return nil, errors.New("invalid producer identity")
	}
	privateKey, err := base64.RawURLEncoding.DecodeString(record.PrivateKey)
	if err != nil || len(privateKey) != ed25519.PrivateKeySize || base64.RawURLEncoding.EncodeToString(privateKey) != record.PrivateKey {
		return nil, errors.New("invalid producer key")
	}
	key := ed25519.PrivateKey(append([]byte(nil), privateKey...))
	recomputedKey := ed25519.NewKeyFromSeed(key.Seed())
	if !bytes.Equal(recomputedKey, key) {
		return nil, errors.New("invalid producer key")
	}
	publicKey := recomputedKey.Public().(ed25519.PublicKey)
	recordedPublicKey, publicError := base64.RawURLEncoding.DecodeString(record.PublicKey)
	if len(publicKey) != ed25519.PublicKeySize || publicError != nil || len(recordedPublicKey) != ed25519.PublicKeySize || base64.RawURLEncoding.EncodeToString(recordedPublicKey) != record.PublicKey || !ed25519.PublicKey(recordedPublicKey).Equal(publicKey) {
		return nil, errors.New("invalid producer key")
	}
	return &ProducerIdentity{keyID: record.KeyID, privateKey: recomputedKey, publicKey: append(ed25519.PublicKey(nil), publicKey...)}, nil
}

func (identity *ProducerIdentity) Fingerprint() string {
	digest := sha256.Sum256(identity.publicKey)
	return hex.EncodeToString(digest[:])
}

func (identity *ProducerIdentity) SignChallenge(challenge []byte) []byte {
	message := make([]byte, 0, len(producerSignatureDomain)+len(challenge))
	message = append(message, producerSignatureDomain...)
	message = append(message, challenge...)
	return ed25519.Sign(identity.privateKey, message)
}

func (identity *ProducerIdentity) KeyID() string { return identity.keyID }
func (identity *ProducerIdentity) PublicKey() ed25519.PublicKey {
	return append(ed25519.PublicKey(nil), identity.publicKey...)
}

func GenerateProducerIdentity(keyID string) (*ProducerIdentity, []byte, error) {
	publicKey, privateKey, err := ed25519.GenerateKey(cryptorand.Reader)
	if err != nil {
		return nil, nil, err
	}
	record := producerIdentityRecord{SchemaVersion: producerIdentitySchema, KeyID: keyID, PrivateKey: base64.RawURLEncoding.EncodeToString(privateKey), PublicKey: base64.RawURLEncoding.EncodeToString(publicKey)}
	encoded, err := json.Marshal(record)
	if err != nil {
		return nil, nil, err
	}
	identity, err := DecodeProducerIdentity(encoded)
	return identity, encoded, err
}
