// Package storage adapts the encrypted private-file capability to semantic
// owner repositories. Owners never import this package or storage.Files.
package storage

import (
	"encoding/json"
	"errors"
	"os"

	"floe/server/internal/integrations"
	privatefiles "floe/server/internal/storage"
	"floe/server/internal/trust"
)

const (
	trustStateFile       = "trust.json"
	producerIdentityFile = "producer-identity.json"
	administratorFile    = "admin-token"
	integrationsFile     = "integrations.json"
)

type TrustRepository struct{ files *privatefiles.Files }

func NewTrustRepository(files *privatefiles.Files) *TrustRepository {
	return &TrustRepository{files: files}
}

func (repository *TrustRepository) Load() trust.RepositorySnapshot {
	if repository == nil || repository.files == nil {
		return trust.RepositorySnapshot{
			State:                   trust.StateReadOutcome{Disposition: trust.ReadUnavailable},
			Producer:                trust.ProducerReadOutcome{Disposition: trust.ReadUnavailable},
			AdministratorCredential: trust.AdministratorCredentialReadOutcome{Disposition: trust.ReadUnavailable},
		}
	}
	out := trust.RepositorySnapshot{}
	data, err := repository.files.Read(trustStateFile, 1<<20)
	switch {
	case errors.Is(err, os.ErrNotExist):
		out.State.Disposition = trust.ReadAbsent
	case err != nil:
		out.State.Disposition = readDisposition(err)
		out.State.Cause = err
	default:
		if trust.DecodeStrict(data, &out.State.State, 1<<20, 32) != nil {
			out.State.Disposition = trust.ReadInvalid
		} else {
			out.State.Disposition = trust.ReadPresent
		}
	}

	data, err = repository.files.Read(producerIdentityFile, 4096)
	switch {
	case errors.Is(err, os.ErrNotExist):
		out.Producer.Disposition = trust.ReadAbsent
	case err != nil:
		out.Producer.Disposition = readDisposition(err)
		out.Producer.Cause = err
	default:
		var identity trust.ProducerIdentity
		if json.Unmarshal(data, &identity) != nil {
			out.Producer.Disposition = trust.ReadInvalid
		} else {
			out.Producer.Disposition = trust.ReadPresent
			out.Producer.Identity = &identity
		}
	}

	token, err := repository.files.Read(administratorFile, 1024)
	switch {
	case errors.Is(err, os.ErrNotExist):
		out.AdministratorCredential.Disposition = trust.ReadAbsent
	case err != nil:
		out.AdministratorCredential.Disposition = readDisposition(err)
		out.AdministratorCredential.Cause = err
	default:
		if len(token) < 32 {
			out.AdministratorCredential.Disposition = trust.ReadInvalid
		} else {
			out.AdministratorCredential.Disposition = trust.ReadPresent
			out.AdministratorCredential.Fingerprint = trust.Digest(string(token))
		}
	}
	return out
}

func (repository *TrustRepository) SaveState(state trust.StateSnapshot) trust.WriteOutcome {
	if repository == nil || repository.files == nil {
		return trust.WriteOutcome{Disposition: trust.WriteRejected, Cause: privatefiles.ErrUnavailable}
	}
	data, err := json.Marshal(state)
	if err == nil {
		err = repository.files.Write(trustStateFile, data)
	}
	return trustWriteOutcome(err)
}

func (repository *TrustRepository) SaveProducerIdentity(identity *trust.ProducerIdentity) trust.WriteOutcome {
	if repository == nil || repository.files == nil || identity == nil {
		return trust.WriteOutcome{Disposition: trust.WriteRejected, Cause: privatefiles.ErrUnavailable}
	}
	data, err := json.Marshal(identity)
	if err == nil {
		err = repository.files.Write(producerIdentityFile, data)
	}
	return trustWriteOutcome(err)
}

func (repository *TrustRepository) SaveAdministratorCredential(token string) trust.WriteOutcome {
	var err error
	if repository == nil || repository.files == nil {
		err = privatefiles.ErrUnavailable
	} else {
		err = repository.files.Write(administratorFile, []byte(token))
	}
	return trustWriteOutcome(err)
}

func (repository *TrustRepository) Health() trust.RepositoryHealth {
	if repository == nil || repository.files == nil || repository.files.Available() != nil {
		return trust.RepositoryUnavailable
	}
	return trust.RepositoryReady
}

// ReadAdministratorToken is the explicit local operator retrieval path. It
// is intentionally separate from owner startup and performs no initialization.
func (repository *TrustRepository) ReadAdministratorToken() (string, error) {
	if repository == nil || repository.files == nil {
		return "", privatefiles.ErrUnavailable
	}
	data, err := repository.files.Read(administratorFile, 1024)
	return string(data), err
}

type IntegrationsRepository struct{ files *privatefiles.Files }

func NewIntegrationsRepository(files *privatefiles.Files) *IntegrationsRepository {
	return &IntegrationsRepository{files: files}
}

func (repository *IntegrationsRepository) LoadState() integrations.LoadOutcome {
	if repository == nil || repository.files == nil {
		return integrations.LoadOutcome{Disposition: integrations.LoadUnavailable}
	}
	data, err := repository.files.Read(integrationsFile, 2<<20)
	switch {
	case errors.Is(err, os.ErrNotExist):
		return integrations.LoadOutcome{Disposition: integrations.LoadAbsent}
	case err != nil:
		return integrations.LoadOutcome{Disposition: integrationReadDisposition(err), Cause: err}
	}
	var state integrations.StateSnapshot
	if trust.DecodeStrict(data, &state, 2<<20, 32) != nil {
		return integrations.LoadOutcome{Disposition: integrations.LoadInvalid}
	}
	return integrations.LoadOutcome{Disposition: integrations.LoadPresent, Snapshot: state}
}

func (repository *IntegrationsRepository) SaveState(state integrations.StateSnapshot) integrations.WriteOutcome {
	if repository == nil || repository.files == nil {
		return integrations.WriteOutcome{Disposition: integrations.WriteRejected, Cause: privatefiles.ErrUnavailable}
	}
	data, err := json.Marshal(state)
	if err == nil {
		err = repository.files.Write(integrationsFile, data)
	}
	return integrationsWriteOutcome(err)
}

func (repository *IntegrationsRepository) Health() integrations.RepositoryHealth {
	if repository == nil || repository.files == nil || repository.files.Available() != nil {
		return integrations.RepositoryUnavailable
	}
	return integrations.RepositoryReady
}

func readDisposition(err error) trust.ReadDisposition {
	if errors.Is(err, privatefiles.ErrIntegrity) {
		return trust.ReadInvalid
	}
	return trust.ReadUnavailable
}

func integrationReadDisposition(err error) integrations.LoadDisposition {
	if errors.Is(err, privatefiles.ErrIntegrity) {
		return integrations.LoadInvalid
	}
	return integrations.LoadUnavailable
}

func trustWriteOutcome(err error) trust.WriteOutcome {
	switch {
	case err == nil:
		return trust.WriteOutcome{Disposition: trust.WriteCommitted}
	case privatefiles.IsIndeterminate(err):
		return trust.WriteOutcome{Disposition: trust.WriteIndeterminate, Cause: err}
	case errors.Is(err, privatefiles.ErrIntegrity):
		return trust.WriteOutcome{Disposition: trust.WriteIntegrityFailure, Cause: err}
	default:
		return trust.WriteOutcome{Disposition: trust.WriteRejected, Cause: err}
	}
}

func integrationsWriteOutcome(err error) integrations.WriteOutcome {
	switch {
	case err == nil:
		return integrations.WriteOutcome{Disposition: integrations.WriteCommitted}
	case privatefiles.IsIndeterminate(err):
		return integrations.WriteOutcome{Disposition: integrations.WriteIndeterminate, Cause: err}
	case errors.Is(err, privatefiles.ErrIntegrity):
		return integrations.WriteOutcome{Disposition: integrations.WriteIntegrityFailure, Cause: err}
	default:
		return integrations.WriteOutcome{Disposition: integrations.WriteRejected, Cause: err}
	}
}
