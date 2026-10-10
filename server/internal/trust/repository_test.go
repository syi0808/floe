package trust

import (
	"errors"
	"testing"
)

type memoryTrustRepository struct {
	snapshot           RepositorySnapshot
	adminToken         string
	health             RepositoryHealth
	nextState          *WriteOutcome
	commitUnknownState bool
	producerWrites     int
	adminWrites        int
	stateWrites        int
}

func newMemoryTrustRepository() *memoryTrustRepository {
	return &memoryTrustRepository{
		snapshot: RepositorySnapshot{
			State:                   StateReadOutcome{Disposition: ReadAbsent},
			Producer:                ProducerReadOutcome{Disposition: ReadAbsent},
			AdministratorCredential: AdministratorCredentialReadOutcome{Disposition: ReadAbsent},
		},
		health: RepositoryReady,
	}
}

func (repository *memoryTrustRepository) Load() RepositorySnapshot { return repository.snapshot }
func (repository *memoryTrustRepository) Health() RepositoryHealth { return repository.health }
func (repository *memoryTrustRepository) SaveProducerIdentity(identity *ProducerIdentity) WriteOutcome {
	repository.producerWrites++
	repository.snapshot.Producer = ProducerReadOutcome{Disposition: ReadPresent, Identity: identity}
	return WriteOutcome{Disposition: WriteCommitted}
}
func (repository *memoryTrustRepository) SaveAdministratorCredential(token string) WriteOutcome {
	repository.adminWrites++
	repository.adminToken = token
	repository.snapshot.AdministratorCredential = AdministratorCredentialReadOutcome{Disposition: ReadPresent, Fingerprint: Digest(token)}
	return WriteOutcome{Disposition: WriteCommitted}
}
func (repository *memoryTrustRepository) SaveState(state StateSnapshot) WriteOutcome {
	repository.stateWrites++
	outcome := WriteOutcome{Disposition: WriteCommitted}
	if repository.nextState != nil {
		outcome = *repository.nextState
		repository.nextState = nil
	}
	if outcome.Disposition == WriteCommitted || repository.commitUnknownState && outcome.Disposition == WriteIndeterminate {
		repository.snapshot.State = StateReadOutcome{Disposition: ReadPresent, State: clone(state)}
	}
	return outcome
}

func TestOpenRejectsPartialBootstrapWithoutReplacingIdentityOrToken(t *testing.T) {
	identity, err := GenerateProducerIdentity(NewID())
	if err != nil {
		t.Fatal(err)
	}
	for _, test := range []struct {
		name     string
		producer ProducerReadOutcome
		admin    AdministratorCredentialReadOutcome
		state    StateReadOutcome
	}{
		{name: "producer only", producer: ProducerReadOutcome{Disposition: ReadPresent, Identity: identity}, admin: AdministratorCredentialReadOutcome{Disposition: ReadAbsent}, state: StateReadOutcome{Disposition: ReadAbsent}},
		{name: "administrator token only", producer: ProducerReadOutcome{Disposition: ReadAbsent}, admin: AdministratorCredentialReadOutcome{Disposition: ReadPresent, Fingerprint: Digest(Token())}, state: StateReadOutcome{Disposition: ReadAbsent}},
		{name: "damaged producer", producer: ProducerReadOutcome{Disposition: ReadInvalid}, admin: AdministratorCredentialReadOutcome{Disposition: ReadAbsent}, state: StateReadOutcome{Disposition: ReadAbsent}},
		{name: "damaged trust state", producer: ProducerReadOutcome{Disposition: ReadAbsent}, admin: AdministratorCredentialReadOutcome{Disposition: ReadAbsent}, state: StateReadOutcome{Disposition: ReadInvalid}},
	} {
		t.Run(test.name, func(t *testing.T) {
			repository := newMemoryTrustRepository()
			repository.snapshot = RepositorySnapshot{State: test.state, Producer: test.producer, AdministratorCredential: test.admin}
			if _, err := Open(repository, true); err == nil {
				t.Fatal("damaged profile unexpectedly initialized")
			}
			if repository.producerWrites != 0 || repository.adminWrites != 0 || repository.stateWrites != 0 {
				t.Fatalf("partial initialization was replaced: producer=%d admin=%d state=%d", repository.producerWrites, repository.adminWrites, repository.stateWrites)
			}
		})
	}
}

func TestRepositoryCommitOutcomesFenceAdoptionAndRecoverAfterReopen(t *testing.T) {
	repository := newMemoryTrustRepository()
	service, err := Open(repository, true)
	if err != nil {
		t.Fatal(err)
	}
	before := clone(service.state)
	adminToken := repository.adminToken
	originalMetadata, err := service.ProducerMetadata()
	if err != nil {
		t.Fatal(err)
	}

	precommit := clone(before)
	precommit.Revision++
	precommit.InstanceID = NewID()
	repository.nextState = &WriteOutcome{Disposition: WriteRejected, Cause: errors.New("synthetic precommit failure")}
	if err := service.persist(precommit); err == nil {
		t.Fatal("known precommit failure unexpectedly succeeded")
	}
	if service.state.Revision != before.Revision || service.state.InstanceID != before.InstanceID {
		t.Fatal("Trust adopted a snapshot that was not committed")
	}
	if err := service.RequiredSecurityError(); err != nil {
		t.Fatalf("determinate precommit failure disabled Trust: %v", err)
	}
	if repository.snapshot.State.State.Revision != before.Revision {
		t.Fatal("repository changed after a known precommit failure")
	}

	uncertain := clone(before)
	uncertain.Revision++
	uncertain.InstanceID = NewID()
	repository.commitUnknownState = true
	repository.nextState = &WriteOutcome{Disposition: WriteIndeterminate, Cause: errors.New("synthetic post-rename ambiguity")}
	if err := service.persist(uncertain); err == nil {
		t.Fatal("ambiguous commit unexpectedly reported success")
	}
	if service.state.Revision != before.Revision || service.state.InstanceID != before.InstanceID {
		t.Fatal("Trust adopted an indeterminate snapshot in the live process")
	}
	if err := service.RequiredSecurityError(); err == nil {
		t.Fatal("indeterminate persistence did not disable Trust authority")
	}
	if repository.snapshot.State.Disposition != ReadPresent || repository.snapshot.State.State.InstanceID != uncertain.InstanceID {
		t.Fatal("synthetic post-rename commit was not retained by the repository")
	}

	reopened, err := Open(repository, false)
	if err != nil {
		t.Fatalf("reopen committed snapshot after lost acknowledgement: %v", err)
	}
	if reopened.state.Revision != uncertain.Revision || reopened.state.InstanceID != uncertain.InstanceID {
		t.Fatal("reopen did not adopt the durable snapshot")
	}
	reopenedMetadata, err := reopened.ProducerMetadata()
	if err != nil || reopenedMetadata.KeyID != originalMetadata.KeyID || reopenedMetadata.PublicKey != originalMetadata.PublicKey || reopenedMetadata.Fingerprint != originalMetadata.Fingerprint {
		t.Fatalf("producer identity changed after reopen: got=%#v err=%v", reopenedMetadata, err)
	}
	if _, login := reopened.LoginOperator(adminToken); login != nil {
		t.Fatalf("administrator credential did not read back: %v", login)
	}
}
