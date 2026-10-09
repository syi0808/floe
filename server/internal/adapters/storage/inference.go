package storage

import (
	"encoding/json"
	"errors"
	"os"

	"floe/server/internal/inference"
	privatefiles "floe/server/internal/storage"
	"floe/server/internal/trust"
)

const inferenceConfigurationFile = "inference.json"

// InferenceConfigRepository owns the inference.json codec and encrypted file
// replacement. It intentionally performs no Inference semantic validation.
type InferenceConfigRepository struct{ files *privatefiles.Files }

func NewInferenceConfigRepository(files *privatefiles.Files) *InferenceConfigRepository {
	return &InferenceConfigRepository{files: files}
}

func (repository *InferenceConfigRepository) LoadConfig() inference.ConfigReadOutcome {
	if repository == nil || repository.files == nil {
		return inference.ConfigReadOutcome{Disposition: inference.ConfigReadUnavailable}
	}
	data, err := repository.files.Read(inferenceConfigurationFile, inference.MaxConfigSnapshotBytes)
	switch {
	case errors.Is(err, os.ErrNotExist):
		return inference.ConfigReadOutcome{Disposition: inference.ConfigReadAbsent}
	case errors.Is(err, privatefiles.ErrIntegrity):
		return inference.ConfigReadOutcome{Disposition: inference.ConfigReadInvalid, Cause: err}
	case err != nil:
		return inference.ConfigReadOutcome{Disposition: inference.ConfigReadUnavailable, Cause: err}
	}
	var state inference.ConfigState
	if trust.DecodeStrict(data, &state, inference.MaxConfigSnapshotBytes, 32) != nil {
		return inference.ConfigReadOutcome{Disposition: inference.ConfigReadInvalid}
	}
	return inference.ConfigReadOutcome{Disposition: inference.ConfigReadPresent, State: state}
}

func (repository *InferenceConfigRepository) SaveConfig(state inference.ConfigState) inference.ConfigWriteOutcome {
	if repository == nil || repository.files == nil {
		return inference.ConfigWriteOutcome{Disposition: inference.ConfigWriteRejected, Cause: privatefiles.ErrUnavailable}
	}
	data, err := json.Marshal(state)
	if err == nil && len(data) > inference.MaxConfigSnapshotBytes {
		err = inference.ErrConfigSnapshotCapacity
	}
	if err == nil {
		err = repository.files.Write(inferenceConfigurationFile, data)
	}
	return inferenceConfigWriteOutcome(err)
}

func (repository *InferenceConfigRepository) Health() inference.ConfigRepositoryHealth {
	if repository == nil || repository.files == nil || repository.files.Available() != nil {
		return inference.ConfigRepositoryUnavailable
	}
	return inference.ConfigRepositoryReady
}

func inferenceConfigWriteOutcome(err error) inference.ConfigWriteOutcome {
	switch {
	case err == nil:
		return inference.ConfigWriteOutcome{Disposition: inference.ConfigWriteCommitted}
	case privatefiles.IsIndeterminate(err):
		return inference.ConfigWriteOutcome{Disposition: inference.ConfigWriteIndeterminate, Cause: err}
	case errors.Is(err, privatefiles.ErrIntegrity):
		return inference.ConfigWriteOutcome{Disposition: inference.ConfigWriteIntegrityFailure, Cause: err}
	default:
		return inference.ConfigWriteOutcome{Disposition: inference.ConfigWriteRejected, Cause: err}
	}
}

var _ inference.ConfigRepository = (*InferenceConfigRepository)(nil)
