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
	"os"
)

const producerIdentitySchema = 1

var producerSignatureDomain = []byte("floe.remote.producer.v1\x00")

type producerIdentityRecord struct {
	SchemaVersion int    `json:"schema_version"`
	KeyID         string `json:"key_id"`
	PrivateKey    string `json:"private_key"`
	PublicKey     string `json:"public_key"`
}

type producerIdentity struct {
	keyID      string
	privateKey ed25519.PrivateKey
	publicKey  ed25519.PublicKey
}

func loadProducerIdentity(path string, allowCreate bool) (*producerIdentity, error) {
	file, err := os.Open(path)
	if err == nil {
		defer file.Close()
		data, readError := io.ReadAll(io.LimitReader(file, 4097))
		if readError != nil {
			return nil, readError
		}
		return decodeProducerIdentity(data)
	}
	if os.IsNotExist(err) {
		if !allowCreate {
			return nil, errors.New("producer identity missing")
		}
		publicKey, privateKey, generateError := ed25519.GenerateKey(cryptorand.Reader)
		if generateError != nil {
			return nil, generateError
		}
		keyID, idError := newConnectionID()
		if idError != nil {
			return nil, idError
		}
		record := producerIdentityRecord{SchemaVersion: producerIdentitySchema, KeyID: keyID, PrivateKey: base64.RawURLEncoding.EncodeToString(privateKey), PublicKey: base64.RawURLEncoding.EncodeToString(publicKey)}
		encoded, marshalError := json.Marshal(record)
		if marshalError != nil {
			return nil, marshalError
		}
		if writeError := writePrivate(path, encoded); writeError != nil {
			return nil, writeError
		}
		return &producerIdentity{keyID: keyID, privateKey: privateKey, publicKey: publicKey}, nil
	}
	return nil, err
}

func decodeProducerIdentity(data []byte) (*producerIdentity, error) {
	if len(data) == 0 || len(data) > 4096 || !strictAuthorityJSON(data) {
		return nil, errors.New("invalid producer identity")
	}
	decoder := json.NewDecoder(bytes.NewReader(data))
	decoder.DisallowUnknownFields()
	var record producerIdentityRecord
	if err := decoder.Decode(&record); err != nil || decoder.Decode(new(any)) != io.EOF {
		return nil, errors.New("invalid producer identity")
	}
	if record.SchemaVersion != producerIdentitySchema || !validConnectionID(record.KeyID) {
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
	return &producerIdentity{keyID: record.KeyID, privateKey: recomputedKey, publicKey: append(ed25519.PublicKey(nil), publicKey...)}, nil
}

func (identity *producerIdentity) fingerprint() string {
	digest := sha256.Sum256(identity.publicKey)
	return hex.EncodeToString(digest[:])
}

func (identity *producerIdentity) signChallenge(challenge []byte) []byte {
	message := make([]byte, 0, len(producerSignatureDomain)+len(challenge))
	message = append(message, producerSignatureDomain...)
	message = append(message, challenge...)
	return ed25519.Sign(identity.privateKey, message)
}

func (console *Console) producerMetadata() (map[string]any, error) {
	if console.producerUnavailable.Load() || console.producer == nil {
		return nil, errors.New("producer identity unavailable")
	}
	console.mu.Lock()
	instanceID := console.state.InstanceID
	executionOwner := console.state.ExecutionOwnerID
	console.mu.Unlock()
	audience := "floe.server:" + instanceID
	return map[string]any{
		"schema_version":  1,
		"instance_id":     instanceID,
		"execution_owner": executionOwner,
		"audience":        audience,
		"key_id":          console.producer.keyID,
		"public_key":      base64.RawURLEncoding.EncodeToString(console.producer.publicKey),
		"fingerprint":     console.producer.fingerprint(),
	}, nil
}
