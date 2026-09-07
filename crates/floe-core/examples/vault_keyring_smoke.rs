#[cfg(target_os = "macos")]
mod macos {
    use std::{
        fs,
        os::unix::fs::{MetadataExt, PermissionsExt},
        path::Path,
        time::Duration,
    };

    use apple_native_keyring_store::protected::{AccessPolicy, Cred};
    use floe_agent::{AgentFailure, AgentOutcome, Cancellation, SessionStore};
    use floe_core::{AgentFixturePrompt, AgentFixtureTurn, EncryptedAgentVault, KeyringVaultKeys};
    use floe_domain::PersonId;
    use keyring_core::{Entry, Error};
    use serde_json::json;
    use uuid::Uuid;
    use zeroize::Zeroizing;

    fn key_error(error: Error) -> String {
        match error {
            Error::PlatformFailure(error) => format!("platform_failure: {error}"),
            Error::NoStorageAccess(error) => format!("storage_access: {error}"),
            Error::NoEntry => "no_entry".into(),
            _ => "key_store_unavailable".into(),
        }
    }

    fn entry(person: PersonId, vault: Uuid) -> Result<Entry, String> {
        Cred::build(
            "com.floe.agent-vault.v1",
            &format!("{person}/{vault}"),
            AccessPolicy::WhenUnlockedThisDeviceOnly,
            None,
            false,
        )
        .map_err(|_| "entry_configuration_failed".into())
    }

    fn key_access_probe() -> Result<(), String> {
        match entry(PersonId::new(), Uuid::new_v4())?.get_secret() {
            Err(Error::NoEntry) => Ok(()),
            Err(error) => Err(key_error(error)),
            Ok(secret) => {
                let _secret = Zeroizing::new(secret);
                Err("unexpected_existing_random_key_slot".into())
            }
        }
    }

    fn owned_entry(root: &Path, person: PersonId) -> Result<Option<Entry>, String> {
        let marker = root.join(person.to_string()).join("vault.id");
        match fs::read_to_string(marker) {
            Ok(value) => Ok(Some(entry(
                person,
                Uuid::parse_str(&value).map_err(|_| "invalid_disposable_marker")?,
            )?)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(_) => Err("cannot_read_disposable_marker".into()),
        }
    }

    fn remove_owned_key(root: &Path, person: PersonId) -> Result<(), String> {
        if let Some(entry) = owned_entry(root, person)? {
            match entry.delete_credential() {
                Ok(()) | Err(Error::NoEntry) => {}
                Err(error) => {
                    return Err(format!(
                        "disposable_key_cleanup_failed: {}",
                        key_error(error)
                    ));
                }
            }
            if !matches!(entry.get_secret(), Err(Error::NoEntry)) {
                return Err("disposable_key_absence_not_verified".into());
            }
        }
        Ok(())
    }

    fn validate_cleanup_root(root: &Path, person: PersonId) -> Result<(), String> {
        let temporary = std::env::temp_dir()
            .canonicalize()
            .map_err(|_| "temporary_directory_unavailable")?;
        let canonical = root.canonicalize().map_err(|_| "cleanup_root_missing")?;
        let metadata = fs::symlink_metadata(root).map_err(|_| "cleanup_root_missing")?;
        if canonical.parent() != Some(temporary.as_path())
            || !canonical.file_name().is_some_and(|name| {
                name.to_string_lossy()
                    .starts_with("floe-vault-keyring-smoke-")
            })
            || !metadata.is_dir()
            || metadata.uid() != unsafe { libc::geteuid() }
            || metadata.mode() & 0o777 != 0o700
        {
            return Err("not_a_private_disposable_root".into());
        }
        let directory = root.join(person.to_string());
        let directory_metadata =
            fs::symlink_metadata(&directory).map_err(|_| "disposable_person_missing")?;
        let marker = fs::symlink_metadata(directory.join("vault.id"))
            .map_err(|_| "disposable_marker_missing")?;
        if !directory_metadata.is_dir()
            || directory_metadata.uid() != metadata.uid()
            || directory_metadata.mode() & 0o777 != 0o700
            || !marker.is_file()
            || marker.uid() != metadata.uid()
            || marker.len() != 36
            || marker.mode() & 0o777 != 0o600
        {
            return Err("not_a_private_disposable_person".into());
        }
        let entries: Vec<_> = fs::read_dir(root)
            .map_err(|_| "cleanup_root_unreadable")?
            .collect::<Result<_, _>>()
            .map_err(|_| "cleanup_root_unreadable")?;
        if entries.len() != 1 || entries[0].file_name() != person.to_string().as_str() {
            return Err("unexpected_disposable_contents".into());
        }
        Ok(())
    }

    async fn exercise(root: &Path, person: PersonId) -> Result<(), AgentFailure> {
        let vault = EncryptedAgentVault::create(root, person, KeyringVaultKeys).await?;
        let initial = vault.create_sample_session().await?;
        let completed = vault
            .run_persisted_agent_sample(
                AgentFixtureTurn {
                    person_id: person,
                    session_id: initial.id,
                    expected_revision: 0,
                    prompt: AgentFixturePrompt::Today,
                },
                Cancellation::default(),
                Duration::ZERO,
                |_| {},
            )
            .await?;
        if completed.last_outcome != Some(AgentOutcome::Completed) || completed.messages.len() != 3
        {
            return Err(AgentFailure::InvalidModelOutput);
        }
        let registry = vault
            .expert_registry()
            .await?
            .ok_or(AgentFailure::StorageUnavailable)?;
        vault.checkpoint().await?;
        drop(vault);
        let reopened = EncryptedAgentVault::open(root, person, KeyringVaultKeys).await?;
        if reopened.load(person, initial.id).await? != completed {
            return Err(AgentFailure::StorageUnavailable);
        }
        if reopened.expert_registry().await? != Some(registry) {
            return Err(AgentFailure::StorageUnavailable);
        }
        let key = owned_entry(root, person)
            .map_err(|_| AgentFailure::VaultUnavailable)?
            .ok_or(AgentFailure::VaultUnavailable)?;
        key.delete_credential()
            .map_err(|_| AgentFailure::VaultUnavailable)?;
        if reopened.check_access() != Err(AgentFailure::VaultUnavailable) {
            return Err(AgentFailure::StorageUnavailable);
        }
        drop(reopened);
        if !matches!(
            EncryptedAgentVault::open(root, person, KeyringVaultKeys).await,
            Err(AgentFailure::VaultUnavailable)
        ) {
            return Err(AgentFailure::StorageUnavailable);
        }
        Ok(())
    }

    pub fn run() -> Result<(), String> {
        let arguments: Vec<String> = std::env::args().skip(1).collect();
        if let [mode, root, person] = arguments.as_slice()
            && mode == "--cleanup"
        {
            let root = Path::new(root);
            let person = serde_json::from_value(json!(person)).map_err(|_| "invalid_person_id")?;
            validate_cleanup_root(root, person)?;
            remove_owned_key(root, person)?;
            fs::remove_dir_all(root).map_err(|_| "disposable_directory_cleanup_failed")?;
            println!(
                "{}",
                json!({"schema_version":1,"status":"cleaned","personal_data":false})
            );
            return Ok(());
        }
        if arguments != ["--probe"] && arguments != ["--exercise"] {
            return Err(
                "use --probe, --exercise or --cleanup <retained temporary root> <Person UUID>"
                    .into(),
            );
        }
        key_access_probe()?;
        if arguments == ["--probe"] {
            println!(
                "{}",
                json!({"schema_version":1,"status":"read_probe_passed","evidence":"read_only_random_slot","write_access_verified":false,"backend":"apple_protected","personal_data":false})
            );
            return Ok(());
        }
        let directory = tempfile::Builder::new()
            .prefix("floe-vault-keyring-smoke-")
            .tempdir()
            .map_err(|_| "temporary_directory_failed")?;
        fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700))
            .map_err(|_| "private_directory_failed")?;
        let person = PersonId::new();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| "runtime_failed")?;
        let result = runtime.block_on(exercise(directory.path(), person));
        if let Err(error) = remove_owned_key(directory.path(), person) {
            let retained = directory.keep();
            return Err(format!(
                "{error}; exercise_result: {result:?}; retained disposable artifacts at {}",
                retained.display()
            ));
        }
        directory
            .close()
            .map_err(|_| "disposable_directory_cleanup_failed")?;
        result.map_err(|failure| format!("vault_exercise_failed: {failure:?}"))?;
        println!(
            "{}",
            json!({"schema_version":1,"status":"passed","evidence":"signed_native_core","checks":["real_key_create","encrypted_sample_turn","reopen","registry_reopen","key_loss_fail_closed","exact_key_cleanup","temporary_file_cleanup"],"personal_data":false})
        );
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn disposable() -> (tempfile::TempDir, PersonId) {
            let root = tempfile::Builder::new()
                .prefix("floe-vault-keyring-smoke-")
                .tempdir()
                .unwrap();
            fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
            let person = PersonId::new();
            let directory = root.path().join(person.to_string());
            fs::create_dir(&directory).unwrap();
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
            let marker = directory.join("vault.id");
            fs::write(&marker, Uuid::new_v4().to_string()).unwrap();
            fs::set_permissions(marker, fs::Permissions::from_mode(0o600)).unwrap();
            (root, person)
        }

        #[test]
        fn cleanup_scope_is_one_private_disposable_person() {
            let (root, person) = disposable();
            assert!(validate_cleanup_root(root.path(), person).is_ok());
            assert!(validate_cleanup_root(root.path(), PersonId::new()).is_err());
            fs::write(root.path().join("unrelated"), "fixture").unwrap();
            assert!(validate_cleanup_root(root.path(), person).is_err());
        }

        #[test]
        fn cleanup_rejects_public_root_and_symlink_marker() {
            let (root, person) = disposable();
            fs::set_permissions(root.path(), fs::Permissions::from_mode(0o755)).unwrap();
            assert!(validate_cleanup_root(root.path(), person).is_err());
            fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
            let marker = root.path().join(person.to_string()).join("vault.id");
            let moved = marker.with_file_name("other.id");
            fs::rename(&marker, &moved).unwrap();
            std::os::unix::fs::symlink(&moved, &marker).unwrap();
            assert!(validate_cleanup_root(root.path(), person).is_err());
        }

        #[test]
        fn cleanup_rejects_root_symlink() {
            let (root, person) = disposable();
            let holder = tempfile::tempdir().unwrap();
            let alias = holder.path().join("alias");
            std::os::unix::fs::symlink(root.path(), &alias).unwrap();
            assert!(validate_cleanup_root(&alias, person).is_err());
        }
    }
}

fn main() -> std::process::ExitCode {
    #[cfg(target_os = "macos")]
    let result = macos::run();
    #[cfg(not(target_os = "macos"))]
    let result: Result<(), String> = Err("unsupported_platform".into());
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            println!(
                "{}",
                serde_json::json!({"schema_version":1,"status":"unavailable","reason":error,"personal_data":false})
            );
            std::process::ExitCode::FAILURE
        }
    }
}
