package storage

import (
	"encoding/json"
	"errors"
	"os"
	"reflect"

	privatefiles "floe/server/internal/adapters/storage/privatefiles"
	"floe/server/internal/inference"
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
	state, decodeErr := inference.DecodeConfigState(data)
	if decodeErr != nil {
		return inference.ConfigReadOutcome{Disposition: inference.ConfigReadInvalid, Cause: decodeErr}
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

func receiptPageName(digest string) (string, error) {
	if len(digest) != 64 {
		return "", errors.New("invalid receipt page identity")
	}
	for _, c := range digest {
		if !(c >= '0' && c <= '9' || c >= 'a' && c <= 'f') {
			return "", errors.New("invalid receipt page identity")
		}
	}
	return "inference-receipts-" + digest + ".json", nil
}

func (repository *InferenceConfigRepository) LoadReceiptPage(digest string) inference.ConfigReceiptPageRead {
	name, err := receiptPageName(digest)
	if err != nil {
		return inference.ConfigReceiptPageRead{Disposition: inference.ConfigReadInvalid, Cause: err}
	}
	if repository == nil || repository.files == nil {
		return inference.ConfigReceiptPageRead{Disposition: inference.ConfigReadUnavailable}
	}
	data, err := repository.files.Read(name, inference.MaxConfigReceiptPageBytes)
	if errors.Is(err, os.ErrNotExist) {
		return inference.ConfigReceiptPageRead{Disposition: inference.ConfigReadAbsent}
	}
	if errors.Is(err, privatefiles.ErrIntegrity) {
		return inference.ConfigReceiptPageRead{Disposition: inference.ConfigReadInvalid, Cause: err}
	}
	if err != nil {
		return inference.ConfigReceiptPageRead{Disposition: inference.ConfigReadUnavailable, Cause: err}
	}
	var page inference.ConfigReceiptPage
	if err := trust.DecodeStrict(data, &page, inference.MaxConfigReceiptPageBytes, 8); err != nil {
		return inference.ConfigReceiptPageRead{Disposition: inference.ConfigReadInvalid, Cause: err}
	}
	actual, err := inference.ConfigReceiptPageDigest(page)
	if err != nil || actual != digest {
		return inference.ConfigReceiptPageRead{Disposition: inference.ConfigReadInvalid, Cause: errors.New("receipt page identity mismatch")}
	}
	return inference.ConfigReceiptPageRead{Disposition: inference.ConfigReadPresent, Page: page}
}

func (repository *InferenceConfigRepository) SaveReceiptPage(digest string, page inference.ConfigReceiptPage) inference.ConfigWriteOutcome {
	actual, err := inference.ConfigReceiptPageDigest(page)
	if err != nil || actual != digest {
		return inference.ConfigWriteOutcome{Disposition: inference.ConfigWriteRejected, Cause: errors.New("invalid receipt page")}
	}
	read := repository.LoadReceiptPage(digest)
	switch read.Disposition {
	case inference.ConfigReadPresent:
		if !reflect.DeepEqual(read.Page, page) {
			return inference.ConfigWriteOutcome{Disposition: inference.ConfigWriteIntegrityFailure}
		}
		return inference.ConfigWriteOutcome{Disposition: inference.ConfigWriteCommitted}
	case inference.ConfigReadInvalid:
		return inference.ConfigWriteOutcome{Disposition: inference.ConfigWriteIntegrityFailure, Cause: read.Cause}
	case inference.ConfigReadAbsent:
		name, _ := receiptPageName(digest)
		data, err := json.Marshal(page)
		if err == nil {
			err = repository.files.Write(name, data)
		}
		return inferenceConfigWriteOutcome(err)
	default:
		return inference.ConfigWriteOutcome{Disposition: inference.ConfigWriteRejected, Cause: read.Cause}
	}
}
