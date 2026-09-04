use chrono::{TimeZone, Utc};
use floe_core::{ErrorCode, FloeCore};
use floe_domain::{DomainRef, Event, PersonId, SourceRef, TimelineItem};
use serde_json::json;

#[tokio::test]
async fn existing_external_records_load_without_rewriting_provenance() {
    let path = std::env::temp_dir().join(format!("floe-legacy-{}.db", uuid::Uuid::new_v4()));
    let person = PersonId::new();
    let identifier = uuid::Uuid::new_v4().to_string();
    let payload = json!({
        "id": identifier, "person_id": person.to_string(), "title": "Legacy fixture",
        "schedule": {"Timed": {"starts_at": "2026-09-04T00:00:00Z", "ends_at": "2026-09-04T01:00:00Z", "timezone": "Asia/Seoul"}},
        "source": {"External": {"connection_id": "old-connection", "provider": "fixture", "resource_type": "calendar_event", "external_id": "old-event", "external_revision": "v1"}},
        "created_at": "2026-09-04T00:00:00Z", "updated_at": "2026-09-04T00:00:00Z",
        "revision": 3, "deleted_at": null
    });
    {
        let database = turso::Builder::new_local(path.to_str().unwrap())
            .build()
            .await
            .unwrap();
        let connection = database.connect().unwrap();
        connection.execute("CREATE TABLE events (id TEXT PRIMARY KEY, person_id TEXT NOT NULL, payload TEXT NOT NULL)", ()).await.unwrap();
        connection
            .execute(
                "INSERT INTO events VALUES (?, ?, ?)",
                (identifier, person.to_string(), payload.to_string()),
            )
            .await
            .unwrap();
    }
    let now = Utc.with_ymd_and_hms(2026, 9, 4, 0, 30, 0).unwrap();
    let core = FloeCore::open(&path).await.unwrap();
    let snapshot = core
        .day_snapshot(person, now.date_naive(), 32400, now)
        .await
        .unwrap();
    let TimelineItem::Event(event) = &snapshot.items[0] else {
        panic!()
    };
    assert_eq!(serde_json::to_value(event).unwrap(), payload);
    assert!(
        matches!(&event.source, SourceRef::External(source) if source.connection_id == "old-connection")
    );
    assert_eq!(
        core.update_event(
            event.id,
            event.revision,
            "Changed",
            event.schedule.clone(),
            now
        )
        .await
        .unwrap_err()
        .code,
        ErrorCode::Validation
    );
    assert_eq!(
        core.delete_item(DomainRef::Event(event.id), event.revision, now)
            .await
            .unwrap_err()
            .code,
        ErrorCode::Validation
    );
    drop(core);
    let reopened = FloeCore::open(&path).await.unwrap();
    let stored = reopened
        .day_snapshot(person, now.date_naive(), 32400, now)
        .await
        .unwrap();
    assert_eq!(
        stored.items,
        vec![TimelineItem::Event(
            serde_json::from_value::<Event>(payload).unwrap()
        )]
    );
    drop(reopened);
    let _ = std::fs::remove_file(path);
}
