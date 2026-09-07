use floe_agent::{AgentSessionScope, CalendarExpertSetup, DataClass};
use floe_domain::CalendarProvider;

use super::*;

#[test]
fn calendar_session_jobs_isolate_resume_and_preserve_scope_across_unlock_and_duplicate_submit() {
    let directory = tempfile::tempdir().unwrap();
    let person = PersonId::new();
    let worker = Worker::new(directory.path().join("vaults"), Keys::default()).unwrap();
    let setup_id = Uuid::new_v4();
    let start = AgentVaultActionDto::CalendarSession {
        operation: AgentCalendarSessionOperationDto::Start {
            setup_id: setup_id.to_string(),
        },
    };
    assert_eq!(
        perform(&worker, person, start.clone()).failure,
        Some(AgentFailure::VaultUnavailable)
    );
    perform(&worker, person, AgentVaultActionDto::Create {});
    assert_eq!(
        perform(&worker, person, start.clone()).failure,
        Some(AgentFailure::NotFound)
    );
    let sample = perform(
        &worker,
        person,
        AgentVaultActionDto::Session {
            operation: AgentFixtureOperationDto::Start {},
        },
    )
    .session
    .unwrap();
    let overview = perform(
        &worker,
        person,
        AgentVaultActionDto::CalendarExperts { setup: None },
    )
    .calendar_experts
    .unwrap();
    let installed = perform(
        &worker,
        person,
        AgentVaultActionDto::CalendarExperts {
            setup: Some(CalendarExpertSetup {
                instance_id: overview.registry.instance_id,
                expected_revision: overview.registry.revision,
                setup_id,
                provider: CalendarProvider::EventKit,
                calendar_ids: vec!["synthetic-no-os-read".into()],
            }),
        },
    )
    .calendar_experts
    .unwrap();
    let job = Uuid::new_v4();
    let operation = AgentVaultOperationDto::Submit { action: start };
    worker.request(person, job, operation.clone()).unwrap();
    let created = wait(&worker, person, job);
    assert!(created.failure.is_none() && created.events.is_empty());
    assert_eq!(worker.request(person, job, operation).unwrap(), created);
    let session = created.session.unwrap();
    assert_eq!(
        session.scope,
        Some(AgentSessionScope::Calendar {
            setup_id,
            provider: CalendarProvider::EventKit
        })
    );
    assert_eq!(session.data_classes, [DataClass::Personal]);
    assert!(session.messages.is_empty());
    worker
        .request(person, job, AgentVaultOperationDto::Release {})
        .unwrap();
    assert_eq!(
        perform(
            &worker,
            person,
            AgentVaultActionDto::Session {
                operation: AgentFixtureOperationDto::Get {
                    session_id: session.id.to_string()
                }
            }
        )
        .failure,
        Some(AgentFailure::PolicyDenied)
    );
    assert_eq!(
        perform(
            &worker,
            person,
            AgentVaultActionDto::CalendarSession {
                operation: AgentCalendarSessionOperationDto::Get {
                    session_id: sample.id.to_string()
                }
            }
        )
        .failure,
        Some(AgentFailure::PolicyDenied)
    );
    assert_eq!(
        perform(
            &worker,
            person,
            AgentVaultActionDto::Session {
                operation: AgentFixtureOperationDto::Resume {}
            }
        )
        .session
        .unwrap(),
        sample
    );
    perform(&worker, person, AgentVaultActionDto::Lock {});
    perform(&worker, person, AgentVaultActionDto::Unlock {});
    let resume = AgentVaultActionDto::CalendarSession {
        operation: AgentCalendarSessionOperationDto::Resume {
            setup_id: setup_id.to_string(),
        },
    };
    assert_eq!(
        perform(&worker, person, resume.clone()).session.unwrap(),
        session
    );
    assert_eq!(
        perform(&worker, PersonId::new(), resume).failure,
        Some(AgentFailure::NotFound)
    );
    assert_eq!(
        perform(
            &worker,
            person,
            AgentVaultActionDto::CalendarSession {
                operation: AgentCalendarSessionOperationDto::Recover {
                    session_id: session.id.to_string(),
                    expected_revision: 0
                }
            }
        )
        .session
        .unwrap(),
        session
    );
    assert_eq!(
        perform(
            &worker,
            person,
            AgentVaultActionDto::CalendarExperts { setup: None }
        )
        .calendar_experts
        .unwrap(),
        installed
    );
}
