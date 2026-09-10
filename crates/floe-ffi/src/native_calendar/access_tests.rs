use super::*;
use floe_core::{CalendarReadAccess, CalendarReadAccessRequest};

fn request() -> CalendarReadAccessRequest {
    CalendarReadAccessRequest {
        person_id: floe_domain::PersonId(uuid::Uuid::parse_str(LOCAL_PERSON).unwrap()),
        provider: floe_domain::CalendarProvider::EventKit,
        calendar_ids: vec!["allowed".into()],
        deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(2),
        cancellation: floe_agent::Cancellation::default(),
    }
}

#[tokio::test]
async fn native_observation_forwards_the_requested_range_and_typed_coverage() {
    let starts_at = chrono::Utc::now() + chrono::Duration::days(5);
    let ends_at = starts_at + chrono::Duration::days(7);
    let observation = observe_calendar(
        CalendarObserveRequest {
            person_id: floe_domain::PersonId(uuid::Uuid::parse_str(LOCAL_PERSON).unwrap()),
            provider: floe_domain::CalendarProvider::EventKit,
            calendar_ids: vec!["allowed".into()],
            starts_at,
            ends_at,
            deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(2),
            cancellation: floe_agent::Cancellation::default(),
        },
        &["allowed".into()],
        move |input| {
            assert_eq!(input["operation"], "observe");
            assert_eq!(input["starts_at"], serde_json::json!(starts_at));
            assert_eq!(input["ends_at"], serde_json::json!(ends_at));
            serde_json::from_value(serde_json::json!({
                "stamp": {
                    "schema_version": 1,
                    "person_id": LOCAL_PERSON,
                    "provider": "event_kit",
                    "calendar_ids": ["allowed"],
                    "generation": "live"
                },
                "observed_at": chrono::Utc::now(),
                "batches": [{
                    "calendar_id": "allowed",
                    "records": []
                }]
            }))
            .map_err(|_| ActionFailure::UncertainResult)
        },
    )
    .await
    .unwrap();
    assert_eq!(observation.stamp.generation, "live");
    assert!(observation.batches[0].records.is_empty());
}

fn response(input: Value) -> Result<floe_core::CalendarReadAccessStamp, ActionFailure> {
    assert_eq!(input["operation"], "view_access");
    serde_json::from_value(json!({"schema_version": 1, "person_id": input["person_id"],
        "provider": input["provider"], "calendar_ids": input["calendar_ids"], "generation": "fixture"}))
        .map_err(|_| ActionFailure::UncertainResult)
}

#[tokio::test]
async fn cancelled_or_dropped_waiter_retains_native_worker_ownership_until_completion() {
    use std::sync::{Arc, Condvar, Mutex};
    struct Release(Arc<(Mutex<bool>, Condvar)>);
    impl Drop for Release {
        fn drop(&mut self) {
            *self.0.0.lock().unwrap() = true;
            self.0.1.notify_all();
        }
    }
    let included = vec!["allowed".to_string()];
    for stop in [false, true] {
        let gate = Arc::new((Mutex::new(false), Condvar::new()));
        let release = Release(gate.clone());
        let started = Arc::new(tokio::sync::Notify::new());
        let notified = started.clone();
        let read = request();
        let cancellation = read.cancellation.clone();
        {
            let operation = check_calendar_read(read, &included, move |input| {
                notified.notify_one();
                let mut done = gate.0.lock().unwrap();
                while !*done {
                    done = gate.1.wait(done).unwrap();
                }
                response(input)
            });
            tokio::pin!(operation);
            tokio::select! {
                _ = &mut operation => panic!("native fixture should wait"),
                result = tokio::time::timeout(std::time::Duration::from_secs(1), started.notified()) => result.unwrap(),
            }
            if stop {
                cancellation.cancel();
                assert_eq!(operation.await, Err(AgentFailure::Cancelled));
            }
        }
        let attempts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let attempted = attempts.clone();
        assert_eq!(
            check_calendar_read(request(), &included, move |input| {
                attempted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                response(input)
            })
            .await,
            Err(AgentFailure::CapabilityUnavailable)
        );
        assert_eq!(attempts.load(std::sync::atomic::Ordering::SeqCst), 0);
        drop(release);
        let recovered = tokio::time::timeout(std::time::Duration::from_secs(1), async {
            loop {
                match check_calendar_read(request(), &included, response).await {
                    Ok(stamp) => break stamp,
                    Err(AgentFailure::CapabilityUnavailable) => {
                        tokio::time::sleep(std::time::Duration::from_millis(5)).await
                    }
                    Err(failure) => panic!("unexpected access failure: {failure:?}"),
                }
            }
        })
        .await
        .unwrap();
        assert_eq!(recovered.generation, "fixture");
    }
}

#[tokio::test]
async fn invalid_or_cancelled_access_never_dispatches_to_native_calendar() {
    let native = NativeCalendar::new(vec!["allowed".into()]);
    for mode in 0..7 {
        let mut request = CalendarReadAccessRequest {
            person_id: floe_domain::PersonId(uuid::Uuid::parse_str(LOCAL_PERSON).unwrap()),
            provider: floe_domain::CalendarProvider::EventKit,
            calendar_ids: vec!["allowed".into()],
            deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(1),
            cancellation: floe_agent::Cancellation::default(),
        };
        match mode {
            0 => request.person_id = floe_domain::PersonId::new(),
            1 => request.provider = floe_domain::CalendarProvider::Fixture,
            2 => request.calendar_ids = vec!["outside-native-scope".into()],
            3 => request.calendar_ids.push("allowed".into()),
            4 => request.calendar_ids.clear(),
            5 => request.cancellation.cancel(),
            _ => request.deadline = tokio::time::Instant::now(),
        }
        assert_eq!(
            native.check(request).await,
            Err(match mode {
                5 => AgentFailure::Cancelled,
                6 => AgentFailure::DeadlineExceeded,
                _ => AgentFailure::CapabilityDenied,
            })
        );
    }
}
