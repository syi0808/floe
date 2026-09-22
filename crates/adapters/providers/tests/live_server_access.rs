#![cfg(target_os = "macos")]

use std::{
    collections::HashMap,
    net::TcpListener,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    time::Duration,
};

use floe_access::{RemoteCallWindow, RemotePairingChallenge, inspect_remote_authority};
use floe_agent_contract::{AgentFailure, PersonId};
use floe_connections::{
    PairingConfirmationRequest, PairingIdentity, PairingService, PairingStatusRequest,
    admit_pairing_report,
};
use floe_execution::Cancellation;
use floe_inference::{SavedConnectionStore, SavedServerConnection};
use floe_provider_adapters::control::{
    CurrentSavedConnectionStore, HttpRemoteControl, PairingStartResponse, RemoteAuthorityEndpoint,
    authorization::access_producer_identity,
};
use floe_vault::{EncryptedAgentVault, VaultKey, VaultKeyProvider};
use reqwest::{Client, StatusCode, header};
use serde_json::{Value, json};
use tokio::time::Instant;
use uuid::Uuid;

#[path = "support/live_model.rs"]
mod live_model;

#[derive(Clone, Default)]
struct MemoryKeys(Arc<Mutex<HashMap<(PersonId, Uuid), [u8; 32]>>>);

impl VaultKeyProvider for MemoryKeys {
    fn load(&self, person: PersonId, vault: Uuid) -> Result<VaultKey, AgentFailure> {
        self.0
            .lock()
            .unwrap()
            .get(&(person, vault))
            .copied()
            .map(VaultKey::from_bytes)
            .ok_or(AgentFailure::VaultUnavailable)
    }

    fn insert(&self, person: PersonId, vault: Uuid, key: &VaultKey) -> Result<(), AgentFailure> {
        self.0
            .lock()
            .unwrap()
            .insert((person, vault), *key.as_bytes());
        Ok(())
    }
}

struct ServerProcess(Child);

impl Drop for ServerProcess {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn window() -> RemoteCallWindow {
    RemoteCallWindow {
        deadline: Instant::now() + Duration::from_secs(10),
        cancellation: Cancellation::default(),
    }
}

#[tokio::test]
async fn live_pairing_current_connection_access_is_denied_after_server_revocation() {
    exercise_live_server(None).await;
}

#[tokio::test]
#[ignore = "requires an explicitly approved existing Codex OAuth credential and model"]
async fn live_codex_model_uses_canonical_inference_and_exact_recipient() {
    let model = std::env::var("FLOE_VALIDATION_CODEX_MODEL").expect("approved configured model");
    assert!(!model.trim().is_empty());
    exercise_live_server(Some(model)).await;
}

async fn exercise_live_server(model: Option<String>) {
    let temporary = tempfile::tempdir().unwrap();
    std::fs::set_permissions(temporary.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let server_directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../server");
    let binary = temporary.path().join("floe-server");
    let build = Command::new("go")
        .current_dir(&server_directory)
        .args(["build", "-o"])
        .arg(&binary)
        .arg("./cmd/floe-server")
        .output()
        .expect("Go is required for the live local-server integration");
    assert!(build.status.success(), "Go server build failed");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let base_url = format!("http://{address}");
    let environment = temporary.path().join("server.env");
    std::fs::write(&environment, "").unwrap();
    let mut process = ServerProcess(
        Command::new(binary)
            .env("FLOE_SERVER_DATA", temporary.path().join("server"))
            .env("FLOE_SERVER_ADDRESS", address.to_string())
            .env("FLOE_ENV_FILE", environment)
            .env("FLOE_INFERENCE_CONFIG", "")
            .env("FLOE_GITHUB_OAUTH_CLIENT_ID", "")
            .env("FLOE_SLACK_OAUTH_CLIENT_ID", "")
            .env("FLOE_GOOGLE_OAUTH_CLIENT_ID", "")
            .env("FLOE_MICROSOFT_OAUTH_CLIENT_ID", "")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let client = Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let startup_deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(
            process.0.try_wait().unwrap().is_none(),
            "server exited during startup"
        );
        if client
            .get(format!("{base_url}/manage/"))
            .send()
            .await
            .is_ok_and(|response| response.status() == StatusCode::OK)
        {
            break;
        }
        assert!(
            Instant::now() < startup_deadline,
            "server startup deadline exceeded"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let admin = std::fs::read_to_string(temporary.path().join("server/admin-token")).unwrap();
    let login = client
        .post(format!("{base_url}/manage/api/login"))
        .header(header::ORIGIN, &base_url)
        .json(&json!({"token": admin}))
        .send()
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let cookie = login
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let state: Value = client
        .get(format!("{base_url}/manage/api/state"))
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let csrf = state["csrf"].as_str().unwrap();
    if let Some(model) = &model {
        let configured = client
            .post(format!("{base_url}/manage/api/provider"))
            .header(header::ORIGIN, &base_url)
            .header(header::COOKIE, &cookie)
            .header("X-Floe-CSRF", csrf)
            .json(&json!({"provider":"codex_oauth", "classes": {
                "balanced": {"model":model, "reasoning_effort":"medium"}
            }}))
            .send()
            .await
            .unwrap();
        assert_eq!(configured.status(), StatusCode::OK);
    }
    let vault_directory = tempfile::tempdir().unwrap();
    std::fs::set_permissions(
        vault_directory.path(),
        std::fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    let person = PersonId::new();
    let device = "live-access-test-device";
    let vault = EncryptedAgentVault::create(vault_directory.path(), person, MemoryKeys::default())
        .await
        .unwrap();
    let owner = vault.remote_owner_public_key().await.unwrap();
    let started: PairingStartResponse = client
        .post(format!("{base_url}/pair/start"))
        .json(&json!({
            "schema_version": 1,
            "person_id": person.to_string(),
            "device_id": device,
            "issuer_key_id": owner.key_id,
            "issuer_public_key": owner.public_key,
        }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(started.schema_version, 1);
    assert_eq!(started.person_id, person.to_string());
    assert_eq!(started.device_id, device);
    assert_eq!(started.issuer.key_id, owner.key_id);
    assert_eq!(started.issuer.public_key, owner.public_key);
    assert_eq!(started.issuer.fingerprint, owner.fingerprint());
    let challenge = RemotePairingChallenge {
        pairing_id: started.pairing_id.clone(),
        challenge_id: started.challenge_id,
        challenge_b64url: started.challenge_b64url,
        producer_signature: started.producer_signature,
        producer: access_producer_identity(&started.producer),
        issuer: owner.clone(),
        expires_at_unix_ms: started.expires_at_unix_ms,
    };
    let person_text = person.to_string();
    let identity = PairingIdentity {
        person_id: &person_text,
        client_id: &started.pairing_id,
        device_id: device,
    };
    let signature = vault
        .remote_sign_pairing(&challenge, &person_text, &started.pairing_id, device)
        .await
        .unwrap();
    let pairing = PairingService::new(HttpRemoteControl::new(&base_url).unwrap());
    let confirmed = pairing
        .confirm(
            PairingConfirmationRequest {
                pairing_id: started.pairing_id.clone(),
                polling_proof: started.proof.clone(),
                challenge_id: challenge.challenge_id.clone(),
                key_id: signature.key_id,
                signature: signature.signature,
            },
            window().deadline,
            &Cancellation::default(),
        )
        .await
        .unwrap();
    assert_eq!(confirmed.status, "local_confirmed");
    let approval = client
        .post(format!("{base_url}/manage/api/pair/approve"))
        .header(header::ORIGIN, &base_url)
        .header(header::COOKIE, &cookie)
        .header("X-Floe-CSRF", csrf)
        .json(&json!({"schema_version": 1, "pairing_id": started.pairing_id, "issuer_fingerprint": owner.fingerprint()}))
        .send()
        .await
        .unwrap();
    assert_eq!(approval.status(), StatusCode::OK);
    let approved = pairing
        .status(
            PairingStatusRequest {
                pairing_id: started.pairing_id.clone(),
                polling_proof: started.proof,
            },
            window().deadline,
            &Cancellation::default(),
        )
        .await
        .unwrap();
    let approved = admit_pairing_report(&person_text, identity, approved).unwrap();
    assert_eq!(approved.status, "approved");
    vault
        .finalize_remote_pairing(&started.pairing_id, &challenge, true)
        .await
        .unwrap();
    let saved = SavedServerConnection {
        base_url: base_url.clone(),
        token: approved.token.unwrap(),
        client_id: approved.client_id.unwrap(),
        person_id: person_text.clone(),
        device_id: device.into(),
        allow_external: false,
        external_recipients: vec![],
    };
    let current = CurrentSavedConnectionStore::fixed(Some(saved.clone()));
    assert!(current.load().unwrap().as_ref() == Some(&saved));
    let endpoint = RemoteAuthorityEndpoint::from_current_connection(
        &current,
        &person_text,
        device,
        Some(&vault),
    )
    .unwrap();
    let inspected = inspect_remote_authority(&endpoint, None, &window())
        .await
        .unwrap();
    assert_eq!(inspected.producer, challenge.producer);
    assert!(matches!(
        RemoteAuthorityEndpoint::from_current_connection(
            &current,
            &person_text,
            "foreign-device",
            Some(&vault)
        ),
        Err(AgentFailure::PolicyDenied)
    ));
    let generated = if model.is_some() {
        live_model::assert_remote_profile(&saved).await;
        let denied = live_model::attempt(&saved).await;
        assert_eq!(denied.result.unwrap_err(), AgentFailure::PolicyDenied);
        assert_eq!(denied.usage.attempts, 0);
        let mut consented = saved.clone();
        consented.allow_external = true;
        consented.external_recipients = vec!["OpenAI (Codex OAuth)".into()];
        Some((live_model::attempt(&consented).await, consented))
    } else {
        None
    };
    let revoked = client
        .post(format!("{base_url}/manage/api/client/delete"))
        .header(header::ORIGIN, &base_url)
        .header(header::COOKIE, &cookie)
        .header("X-Floe-CSRF", csrf)
        .json(&json!({"id": saved.client_id}))
        .send()
        .await
        .unwrap();
    assert_eq!(revoked.status(), StatusCode::OK);
    assert!(current.load().unwrap().as_ref() == Some(&saved));
    let reloaded = RemoteAuthorityEndpoint::from_current_connection(
        &current,
        &person_text,
        device,
        Some(&vault),
    )
    .unwrap();
    assert_eq!(
        inspect_remote_authority(&reloaded, None, &window()).await,
        Err(AgentFailure::CredentialExpired)
    );
    assert_eq!(
        inspect_remote_authority(&endpoint, None, &window()).await,
        Err(AgentFailure::CredentialExpired)
    );
    if let Some((outcome, consented)) = generated {
        let after_revoke = live_model::attempt(&consented).await;
        assert!(after_revoke.result.is_err());
        assert_eq!(after_revoke.usage.attempts, 0);
        live_model::assert_generated(outcome);
        println!("LIVE_CODEX_CANONICAL_INFERENCE_CONSENT_GENERATION_REVOCATION_PASSED");
    }
}
