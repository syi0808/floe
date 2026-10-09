package inference

// ConfigReadDisposition distinguishes an absent first-run snapshot from
// unavailable storage and malformed persisted configuration.
type ConfigReadDisposition uint8

const (
	ConfigReadPresent ConfigReadDisposition = iota + 1
	ConfigReadAbsent
	ConfigReadUnavailable
	ConfigReadInvalid
)

type ConfigReadOutcome struct {
	Disposition ConfigReadDisposition
	State       ConfigState
	Cause       error
}

type ConfigWriteDisposition uint8

const (
	ConfigWriteCommitted ConfigWriteDisposition = iota + 1
	ConfigWriteRejected
	ConfigWriteIndeterminate
	ConfigWriteIntegrityFailure
)

// ConfigWriteOutcome lets Inference preserve its fail-closed behavior for
// ambiguous durable replacement and integrity failures.
type ConfigWriteOutcome struct {
	Disposition ConfigWriteDisposition
	Cause       error
}

type ConfigRepositoryHealth uint8

const (
	ConfigRepositoryReady ConfigRepositoryHealth = iota + 1
	ConfigRepositoryUnavailable
)

// ConfigRepository persists complete Inference-owned snapshots. Implementors
// own filenames, strict encoding and encrypted atomic replacement; Inference
// owns validation, state transitions and live engine adoption.
type ConfigRepository interface {
	LoadConfig() ConfigReadOutcome
	SaveConfig(ConfigState) ConfigWriteOutcome
	Health() ConfigRepositoryHealth
}
