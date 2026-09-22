package application

import (
	"encoding/base64"
	"errors"
	"io"
	"os"

	"floe/server/internal/authorization"
)

func loadProducerIdentity(path string, allowCreate bool) (*authorization.ProducerIdentity, error) {
	file, err := os.Open(path)
	if err == nil {
		defer file.Close()
		data, readError := io.ReadAll(io.LimitReader(file, 4097))
		if readError != nil {
			return nil, readError
		}
		return authorization.DecodeProducerIdentity(data)
	}
	if os.IsNotExist(err) {
		if !allowCreate {
			return nil, errors.New("producer identity missing")
		}
		keyID, err := newConnectionID()
		if err != nil {
			return nil, err
		}
		identity, encoded, err := authorization.GenerateProducerIdentity(keyID)
		if err != nil {
			return nil, err
		}
		if writeError := writePrivate(path, encoded); writeError != nil {
			return nil, writeError
		}
		return identity, nil
	}
	return nil, err
}

func (console *Console) producerMetadata() (map[string]any, error) {
	if console.admissions.ProducerUnavailable() || console.admissions.Producer() == nil {
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
		"key_id":          console.admissions.Producer().KeyID(),
		"public_key":      base64.RawURLEncoding.EncodeToString(console.admissions.Producer().PublicKey()),
		"fingerprint":     console.admissions.Producer().Fingerprint(),
	}, nil
}
