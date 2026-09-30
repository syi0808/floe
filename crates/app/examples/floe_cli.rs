use std::{
    collections::HashMap,
    io::{self, BufRead, IsTerminal, Write},
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver},
    time::Duration,
};

use floe_app::{
    AppComposition, AppHost, CancelRun, CommandReceipt, ConversationCommands, ConversationEvents,
    ConversationInteraction, ConversationQueries, ConversationSessionCommand,
    ConversationSessionCommands, ConversationSessionQueries, ConversationSessionResult,
    EventPayload, EventRead, InteractionDecision, ProfileSelection, ReadConversation,
    ReadConversationEvents, RefreshInteraction, ResolveInteraction, ResumeInteraction, RunReceipt,
    RunState, ServiceError, StartTurn, TurnMode, VaultLifecycleCommand, VaultLifecycleCommands,
    VaultLifecycleQueries, VaultLifecycleResult, VaultState,
};
use floe_conversation::{AgentMessage, AgentSession};
use serde_json::{Value, json};
use uuid::Uuid;

const HELP: &str = "Floe debug CLI (macOS; quit the Flutter app before opening its profile)
Usage: scripts/floe-cli.sh --database /absolute/path/to/people/PERSON/floe.db [options]
  --prompt TEXT     Run one turn, then exit; otherwise read a line-oriented conversation
  --session UUID    Open an existing session; otherwise create a separate session
  --inspect         Read the selected session without starting a turn (requires --session)
  --profile ID      Explicit model profile; otherwise use the existing automatic selection
  --json            Emit newline-delimited JSON to stdout (may contain personal answers)
  --help            Show this help without opening a database
Commands: /interactions, /approve ID, /deny ID, /dismiss ID, /refresh ID,
          /resume ID (interaction ID), /cancel (while a turn is running), /quit
No connector setup, Vault creation/reset, automatic approval, or Action execution.
";

type Result<ValueType> = std::result::Result<ValueType, String>;
type Host = AppHost<AppComposition>;

#[derive(Debug, Default)]
struct Options {
    database: PathBuf,
    prompt: Option<String>,
    session: Option<Uuid>,
    profile: Option<String>,
    inspect: bool,
    json: bool,
}

impl Options {
    fn parse(arguments: impl IntoIterator<Item = String>) -> Result<Option<Self>> {
        let mut arguments = arguments.into_iter();
        let mut options = Self::default();
        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "--help" => return Ok(None),
                "--inspect" => options.inspect = true,
                "--json" => options.json = true,
                "--database" | "--prompt" | "--session" | "--profile" => {
                    let value = arguments
                        .next()
                        .ok_or_else(|| format!("Missing {argument} value"))?;
                    match argument.as_str() {
                        "--database" => options.database = value.into(),
                        "--prompt" => options.prompt = Some(value),
                        "--profile" => options.profile = Some(value),
                        _ => {
                            options.session =
                                Some(Uuid::parse_str(&value).map_err(|_| "Invalid session UUID")?)
                        }
                    }
                }
                _ => return Err(format!("Unknown option: {argument}")),
            }
        }
        if !options.database.is_absolute() {
            return Err(
                "--database must identify an existing absolute client database path".into(),
            );
        }
        if options.session.is_some_and(|session| session.is_nil())
            || options.inspect && (options.session.is_none() || options.prompt.is_some())
        {
            return Err("--inspect requires --session and cannot be combined with --prompt".into());
        }
        if let Some(prompt) = &options.prompt {
            floe_conversation::normalize_turn_text(prompt).map_err(|_| "Invalid prompt")?;
        }
        options
            .profile_selection()
            .validate()
            .map_err(|_| "Invalid model profile")?;
        Ok(Some(options))
    }

    fn profile_selection(&self) -> ProfileSelection {
        self.profile
            .clone()
            .map_or(ProfileSelection::Auto, ProfileSelection::Explicit)
    }
}

fn preflight(database: &Path) -> Result<()> {
    let identity = floe_provider_adapters::local_identity_for_database(database)
        .map_err(|_| "Client profile identity is unavailable or invalid")?
        .ok_or("Not a client profile database")?;
    if !std::fs::symlink_metadata(database).is_ok_and(|metadata| metadata.file_type().is_file()) {
        return Err("Client database does not exist; configure it in the client first".into());
    }
    let vault = PathBuf::from(format!("{}.agent-vaults", database.display()))
        .join(identity.person_id.to_string());
    if !vault.join("vault.id").is_file() {
        return Err("Existing Vault is missing; configure it in the client first".into());
    }
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(vault.join("host.lock"))
        .map_err(|_| "Cannot inspect the existing Vault lock")?;
    lock.try_lock()
        .map_err(|_| "Vault is busy; quit the client or other CLI first")?;
    Ok(())
}

struct Output {
    json: bool,
}

impl Output {
    fn emit(&self, kind: &str, data: Value) -> Result<()> {
        let mut stdout = io::stdout().lock();
        if self.json {
            writeln!(stdout, "{}", json!({"kind": kind, "data": data}))
        } else if kind == "answer" {
            writeln!(stdout, "{}", data["text"].as_str().unwrap_or_default())
        } else {
            writeln!(stdout, "[{kind}] {data}")
        }
        .map_err(|_| "Cannot write CLI output")?;
        stdout.flush().map_err(|_| "Cannot flush CLI output".into())
    }
}

trait Job {
    fn done(&self) -> bool;
    fn failure(&self) -> Option<floe_app::AgentFailure>;
}

impl Job for VaultLifecycleResult {
    fn done(&self) -> bool {
        self.done
    }
    fn failure(&self) -> Option<floe_app::AgentFailure> {
        self.failure
    }
}

impl Job for ConversationSessionResult {
    fn done(&self) -> bool {
        self.done
    }
    fn failure(&self) -> Option<floe_app::AgentFailure> {
        if self.session.is_some() {
            None
        } else {
            self.failure
        }
    }
}

fn finish_job<JobType: Job>(
    mut result: JobType,
    mut poll: impl FnMut(bool) -> Result<JobType>,
) -> Result<JobType> {
    while !result.done() {
        std::thread::sleep(Duration::from_millis(50));
        result = poll(false)?;
    }
    let failure = result.failure();
    poll(true)?;
    if let Some(failure) = failure {
        return Err(format!("Host operation failed: {failure:?}"));
    }
    Ok(result)
}

fn unlock(host: &Host) -> Result<()> {
    let operation = Uuid::new_v4();
    let request = host
        .request(operation)
        .map_err(|failure| format!("Host: {failure:?}"))?;
    let result = request
        .services()
        .vault_command(request.caller(), operation, VaultLifecycleCommand::Unlock)
        .map_err(|failure| format!("Unlock: {failure:?}"))?;
    let result = finish_job(result, |release| {
        request
            .services()
            .read_vault_result(request.caller(), operation, release)
            .map_err(|failure| format!("Unlock observation: {failure:?}"))
    })?;
    if result.state != Some(VaultState::Ready) {
        return Err("Existing Vault could not be unlocked; no key was created or replaced".into());
    }
    Ok(())
}

fn session(host: &Host, session_id: Option<Uuid>) -> Result<AgentSession> {
    let operation = Uuid::new_v4();
    let request = host
        .request(operation)
        .map_err(|failure| format!("Host: {failure:?}"))?;
    let result = match session_id {
        Some(session_id) => request
            .services()
            .get_session(request.caller(), operation, session_id),
        None => request.services().session_command(
            request.caller(),
            operation,
            ConversationSessionCommand::Start,
        ),
    }
    .map_err(|failure| format!("Session: {failure:?}"))?;
    finish_job(result, |release| {
        request
            .services()
            .read_session_result(request.caller(), operation, release)
            .map_err(|failure| format!("Session observation: {failure:?}"))
    })?
    .session
    .ok_or("Host returned no session".into())
}

fn session_trace(session: &AgentSession) -> Value {
    let delegations: Vec<_> = session
        .messages
        .iter()
        .filter_map(|message| match message {
            AgentMessage::Delegation { turn_id, task } => Some(json!({
                "turn_id": turn_id, "task_id": task.id, "expert": task.agent_id,
                "state": task.state, "failure": task.failure,
            })),
            _ => None,
        })
        .collect();
    let capabilities: Vec<_> = session
        .messages
        .iter()
        .filter_map(|message| match message {
            AgentMessage::Capability {
                turn_id,
                call_id,
                capability_id,
                result,
                ..
            } => Some(json!({
                "turn_id": turn_id, "call_id": call_id, "capability": capability_id,
                "failure": result.as_ref().err(),
            })),
            _ => None,
        })
        .collect();
    json!({"session_id": session.id, "revision": session.revision,
        "model_attempts": session.model_attempts, "delegations": delegations,
        "capabilities": capabilities, "outcome": session.last_outcome})
}

fn show_session(output: &Output, session: &AgentSession) -> Result<()> {
    output.emit("session", session_trace(session))?;
    for message in &session.messages {
        if let AgentMessage::Assistant { turn_id, text } = message {
            output.emit("answer", json!({"turn_id": turn_id, "text": text}))?;
        }
    }
    Ok(())
}

struct Cli {
    host: Host,
    options: Options,
    output: Output,
    current: AgentSession,
    reviewed: HashMap<Uuid, ConversationInteraction>,
}

fn admit_turn(
    command: StartTurn,
    mut submit: impl FnMut(StartTurn) -> std::result::Result<CommandReceipt, ServiceError>,
    mut lookup: impl FnMut(Uuid) -> std::result::Result<Option<CommandReceipt>, ServiceError>,
    mut pending: impl FnMut(),
) -> std::result::Result<CommandReceipt, ServiceError> {
    let mut uncertain = false;
    loop {
        if uncertain {
            match lookup(command.command_id) {
                Ok(Some(receipt)) => return Ok(receipt),
                Ok(None) => {}
                Err(ServiceError::Unavailable | ServiceError::Conflict) => {
                    pending();
                    continue;
                }
                Err(failure) => return Err(failure),
            }
        }
        match submit(command.clone()) {
            Ok(receipt) => return Ok(receipt),
            Err(ServiceError::Unavailable) => {
                uncertain = true;
                pending();
            }
            Err(failure) => return Err(failure),
        }
    }
}

impl Cli {
    fn interactions(&mut self) -> Result<()> {
        let request = self
            .host
            .request(Uuid::new_v4())
            .map_err(|failure| format!("Host: {failure:?}"))?;
        let interactions = request
            .services()
            .list_interactions(request.caller(), self.current.id)
            .map_err(|failure| format!("Interactions: {failure:?}"))?;
        self.reviewed.clear();
        for interaction in interactions {
            self.output.emit("interaction", json!({"id": interaction.id,
                "origin_run_id": interaction.origin_run_id.as_uuid(),
                "kind": interaction.kind, "state": interaction.state,
                "revision": interaction.revision, "expires_at_unix_ms": interaction.expires_at_unix_ms,
                "requirement": interaction.requirement, "target": interaction.target}))?;
            self.reviewed.insert(interaction.id, interaction);
        }
        Ok(())
    }

    fn turn(&mut self, text: String, input: Option<&Receiver<String>>) -> Result<bool> {
        let request = self
            .host
            .request(Uuid::new_v4())
            .map_err(|failure| format!("Host: {failure:?}"))?;
        let command = StartTurn {
            command_id: Uuid::new_v4(),
            session_id: self.current.id,
            expected_revision: self.current.revision,
            text,
            mode: TurnMode::New,
            retry_of: None,
            profile: self.options.profile_selection(),
        };
        let command_id = command.command_id;
        let mut reported = false;
        let receipt = admit_turn(
            command,
            |command| request.services().start_turn(request.caller(), command),
            |command_id| {
                request
                    .services()
                    .read_conversation(request.caller(), ReadConversation::Command { command_id })
                    .map(|receipt| {
                        receipt.map(|receipt| CommandReceipt {
                            command_id: receipt.command_id.as_uuid(),
                            run_id: receipt.run_id.as_uuid(),
                            session_revision: receipt.session_revision,
                        })
                    })
            },
            || {
                if !reported {
                    let _ = self.output.emit("admission_pending", json!({"command_id": command_id,
                        "message": "Admission acknowledgement is uncertain; checking the same command, not starting another turn"}));
                    reported = true;
                }
                std::thread::sleep(Duration::from_millis(100));
            },
        );
        drop(request);
        let receipt = match receipt {
            Ok(receipt) => receipt,
            Err(failure) => {
                if failure == ServiceError::Conflict {
                    self.current = session(&self.host, Some(self.current.id))?;
                }
                return Err(format!("Turn admission: {failure:?}"));
            }
        };
        self.observe(receipt, input)
    }

    fn observe(
        &mut self,
        receipt: CommandReceipt,
        input: Option<&Receiver<String>>,
    ) -> Result<bool> {
        self.output.emit(
            "admitted",
            json!({"command_id": receipt.command_id,
            "run_id": receipt.run_id, "session_id": self.current.id}),
        )?;
        let mut cursor = 0;
        let mut cancellation_requested = false;
        'observe: loop {
            if let Some(input) = input {
                if let Ok(line) = input.try_recv() {
                    if line.trim() == "/cancel" && !cancellation_requested {
                        let request = self
                            .host
                            .request(Uuid::new_v4())
                            .map_err(|failure| format!("Host: {failure:?}"))?;
                        request
                            .services()
                            .cancel_run(
                                request.caller(),
                                CancelRun {
                                    command_id: Uuid::new_v4(),
                                    run_id: receipt.run_id,
                                },
                            )
                            .map_err(|failure| format!("Cancel: {failure:?}"))?;
                        cancellation_requested = true;
                        self.output
                            .emit("cancel_requested", json!({"run_id": receipt.run_id}))?;
                    } else {
                        self.output.emit("busy", json!({"message": "Wait for this turn, or use /cancel. Input was not submitted."}))?;
                    }
                }
            }
            let request = self
                .host
                .request(Uuid::new_v4())
                .map_err(|failure| format!("Host: {failure:?}"))?;
            let events = request
                .services()
                .read_conversation_events(
                    request.caller(),
                    ReadConversationEvents {
                        runtime_epoch: Some(request.caller().runtime_epoch()),
                        cursor: Some(cursor),
                        limit: 128,
                    },
                )
                .map_err(|failure| format!("Events: {failure:?}"))?;
            match events {
                EventRead::Events {
                    next_cursor,
                    events,
                } => {
                    cursor = next_cursor;
                    for event in events {
                        if let EventPayload::RunUpdated(record) = event.payload {
                            if record.run_id.as_uuid() == receipt.run_id {
                                self.output.emit("run_status", json!({"run_id": receipt.run_id,
                                    "state": format!("{:?}", record.state), "issue": record.issue,
                                    "model_attempt_ids": record.attempt_refs, "expert_task_ids": record.task_refs}))?;
                                if record.state != RunState::Working {
                                    break 'observe;
                                }
                            }
                        }
                    }
                }
                EventRead::ResyncRequired { snapshot_cursor } => {
                    cursor = snapshot_cursor;
                    self.output.emit("resync", json!({"cursor": cursor}))?;
                    if let Ok(Some(result)) = request.services().read_conversation(
                        request.caller(),
                        ReadConversation::Run {
                            run_id: receipt.run_id,
                        },
                    ) {
                        if result.state != RunState::Working {
                            break 'observe;
                        }
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let request = self
            .host
            .request(Uuid::new_v4())
            .map_err(|failure| format!("Host: {failure:?}"))?;
        let finished = request
            .services()
            .read_conversation(
                request.caller(),
                ReadConversation::Run {
                    run_id: receipt.run_id,
                },
            )
            .map_err(|failure| format!("Settled Run output is unavailable: {failure:?}"))?
            .ok_or("Settled Run not found")?;
        self.output.emit("run", run_trace(&finished))?;
        if let Some(text) = &finished.output {
            self.output
                .emit("answer", json!({"run_id": receipt.run_id, "text": text}))?;
        }
        drop(request);
        self.current = session(&self.host, Some(self.current.id))?;
        self.output.emit("session", session_trace(&self.current))?;
        self.interactions()?;
        let blocked = self.reviewed.values().any(|interaction| {
            interaction.origin_run_id.as_uuid() == receipt.run_id
                && matches!(
                    interaction.state,
                    floe_app::InteractionState::Pending
                        | floe_app::InteractionState::Resolving { .. }
                )
        });
        Ok(finished.state == RunState::Completed && finished.issue.is_none() && !blocked)
    }

    fn interaction_command(
        &mut self,
        command: &str,
        identifier: &str,
        input: &Receiver<String>,
    ) -> Result<()> {
        let identifier = Uuid::parse_str(identifier).map_err(|_| "Expected an interaction UUID")?;
        let reviewed = self
            .reviewed
            .get(&identifier)
            .ok_or("Run /interactions and review the target first")?;
        let request = self
            .host
            .request(Uuid::new_v4())
            .map_err(|failure| format!("Host: {failure:?}"))?;
        match command {
            "/resume" => {
                let receipt = request
                    .services()
                    .resume_interaction(
                        request.caller(),
                        ResumeInteraction {
                            command_id: Uuid::new_v4(),
                            session_id: self.current.id,
                            origin_run_id: reviewed.origin_run_id.as_uuid(),
                            expected_revision: self.current.revision,
                        },
                    )
                    .map_err(|failure| format!("Resume: {failure:?}"))?;
                drop(request);
                self.observe(receipt, Some(input))?;
            }
            "/refresh" => {
                let result = request
                    .services()
                    .refresh_interaction(
                        request.caller(),
                        RefreshInteraction {
                            command_id: Uuid::new_v4(),
                            interaction_id: identifier,
                            session_id: self.current.id,
                            expected_revision: reviewed.revision,
                        },
                    )
                    .map_err(|failure| format!("Refresh: {failure:?}"))?;
                self.output.emit(
                    "refresh",
                    json!({"outcome": format!("{:?}", result.outcome)}),
                )?;
                drop(request);
                if let Some(receipt) = result.linked_run {
                    self.observe(receipt, Some(input))?;
                } else {
                    self.interactions()?;
                }
            }
            _ => {
                let decision = match command {
                    "/approve" => InteractionDecision::Approve,
                    "/deny" => InteractionDecision::Deny,
                    "/dismiss" => InteractionDecision::Dismiss,
                    _ => return Err("Unknown interaction command".into()),
                };
                let result = request
                    .services()
                    .resolve_interaction(
                        request.caller(),
                        ResolveInteraction {
                            command_id: Uuid::new_v4(),
                            interaction_id: identifier,
                            session_id: self.current.id,
                            expected_revision: reviewed.revision,
                            decision,
                            target_digest: reviewed.target_digest,
                        },
                    )
                    .map_err(|failure| format!("Decision: {failure:?}"))?;
                self.output.emit(
                    "decision",
                    json!({"outcome": format!("{:?}", result.outcome)}),
                )?;
                drop(request);
                if let Some(receipt) = result.linked_run {
                    self.observe(receipt, Some(input))?;
                } else {
                    self.interactions()?;
                }
            }
        }
        Ok(())
    }

    fn repl(&mut self) -> Result<()> {
        let (sender, input) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            for line in io::stdin().lock().lines() {
                match line {
                    Ok(line) => {
                        if sender.send(line).is_err() {
                            break;
                        }
                    }
                    _ => break,
                }
            }
        });
        loop {
            if io::stdin().is_terminal() && !self.options.json {
                eprint!("You> ");
                io::stderr().flush().map_err(|_| "Cannot write prompt")?;
            }
            let Ok(line) = input.recv() else {
                return Ok(());
            };
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if line == "/quit" {
                return Ok(());
            }
            let result = if line == "/interactions" {
                self.interactions()
            } else if let Some((command, identifier)) = line.split_once(' ') {
                if ["/approve", "/deny", "/dismiss", "/refresh", "/resume"].contains(&command) {
                    self.interaction_command(command, identifier.trim(), &input)
                } else if line.starts_with('/') {
                    Err("Unknown command; use --help for commands".into())
                } else {
                    self.turn(line.into(), Some(&input)).map(|_| ())
                }
            } else if line.starts_with('/') {
                Err("Unknown command or missing interaction ID".into())
            } else {
                self.turn(line.into(), Some(&input)).map(|_| ())
            };
            if let Err(failure) = result {
                self.output.emit("error", json!({"message": failure}))?;
            }
        }
    }
}

fn run_trace(receipt: &RunReceipt) -> Value {
    json!({"run_id": receipt.run_id.as_uuid(), "state": format!("{:?}", receipt.state),
        "revision": receipt.aggregate_revision, "issue": receipt.issue,
        "model_attempt_ids": receipt.attempt_refs, "expert_task_ids": receipt.task_refs,
        "coverage": receipt.coverage,
        "generated_reply": receipt.output.is_some()})
}

fn run(options: Options) -> Result<bool> {
    preflight(&options.database)?;
    let output = Output { json: options.json };
    output.emit(
        "unlocking",
        json!({"message": "Reading the existing Vault key; macOS may request Keychain access"}),
    )?;
    let host = floe_app::open(
        options
            .database
            .to_str()
            .ok_or("Database path must be UTF-8")?,
    )
    .map_err(
        |_| "Cannot open the client profile; quit the client and check its identity/database",
    )?;
    unlock(&host)?;
    let current = session(&host, options.session)?;
    let mut cli = Cli {
        host,
        output,
        options,
        current,
        reviewed: HashMap::new(),
    };
    cli.output.emit(
        "opened",
        json!({"session_id": cli.current.id, "revision": cli.current.revision}),
    )?;
    let result = if cli.options.inspect {
        show_session(&cli.output, &cli.current)?;
        cli.interactions()?;
        true
    } else if let Some(prompt) = cli.options.prompt.take() {
        cli.turn(prompt, None)?
    } else {
        cli.interactions()?;
        cli.repl()?;
        true
    };
    cli.host
        .shutdown()
        .map_err(|failure| format!("Shutdown: {failure:?}"))?;
    Ok(result)
}

fn main() -> std::process::ExitCode {
    let result = Options::parse(std::env::args().skip(1)).and_then(|options| match options {
        Some(options) => run(options),
        None => {
            print!("{HELP}");
            Ok(true)
        }
    });
    match result {
        Ok(true) => std::process::ExitCode::SUCCESS,
        Ok(false) => std::process::ExitCode::from(2),
        Err(failure) => {
            eprintln!("Floe CLI: {failure}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn_command() -> StartTurn {
        StartTurn {
            command_id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            expected_revision: 0,
            text: "오늘 일정 요약해줘".into(),
            mode: TurnMode::New,
            retry_of: None,
            profile: ProfileSelection::Auto,
        }
    }

    #[test]
    fn lost_admission_acknowledgement_rejoins_without_resubmitting() {
        let command = turn_command();
        let expected = CommandReceipt {
            command_id: command.command_id,
            run_id: Uuid::new_v4(),
            session_revision: 1,
        };
        let mut submitted = Vec::new();
        let mut reads = 0;
        let result = admit_turn(
            command.clone(),
            |command| {
                submitted.push(command);
                Err(ServiceError::Unavailable)
            },
            |command_id| {
                assert_eq!(command_id, expected.command_id);
                reads += 1;
                if reads == 1 {
                    Err(ServiceError::Unavailable)
                } else {
                    Ok(Some(expected.clone()))
                }
            },
            || {},
        )
        .unwrap();
        assert_eq!(result, expected);
        assert_eq!(submitted, [command]);
    }

    #[test]
    fn retry_before_admission_preserves_the_entire_command() {
        let command = turn_command();
        let expected = CommandReceipt {
            command_id: command.command_id,
            run_id: Uuid::new_v4(),
            session_revision: 1,
        };
        let mut submitted = Vec::new();
        let result = admit_turn(
            command.clone(),
            |command| {
                submitted.push(command);
                if submitted.len() == 1 {
                    Err(ServiceError::Unavailable)
                } else {
                    Ok(expected.clone())
                }
            },
            |_| Ok(None),
            || {},
        )
        .unwrap();
        assert_eq!(result, expected);
        assert_eq!(submitted, [command.clone(), command]);
    }

    #[test]
    fn definite_admission_rejections_are_not_retried() {
        let result = admit_turn(
            turn_command(),
            |_| Err(ServiceError::Conflict),
            |_| panic!("A definite rejection is not an uncertain acknowledgement"),
            || panic!("A definite rejection must not be retried"),
        );
        assert_eq!(result, Err(ServiceError::Conflict));
    }

    fn parse(arguments: &[&str]) -> Result<Option<Options>> {
        Options::parse(arguments.iter().map(|argument| (*argument).to_owned()))
    }

    #[test]
    fn help_does_not_require_a_profile() {
        assert!(parse(&["--help"]).unwrap().is_none());
    }

    #[test]
    fn invalid_options_fail_before_opening_a_database() {
        for arguments in [
            vec![],
            vec!["--database", "relative/floe.db"],
            vec!["--database", "/floe.db", "--inspect"],
            vec!["--database", "/floe.db", "--prompt", " "],
            vec!["--database", "/floe.db", "--profile", " "],
            vec!["--database", "/floe.db", "--session", "not-a-uuid"],
            vec!["--database", "/floe.db", "--approve-all"],
        ] {
            assert!(parse(&arguments).is_err(), "{arguments:?}");
        }
    }

    #[test]
    fn explicit_profile_and_session_are_preserved() {
        let session_id = Uuid::new_v4().to_string();
        let options = parse(&[
            "--database",
            "/floe.db",
            "--session",
            &session_id,
            "--profile",
            "foundation-device",
            "--prompt",
            "오늘 일정 요약해줘",
            "--json",
        ])
        .unwrap()
        .unwrap();
        assert_eq!(options.session.unwrap().to_string(), session_id);
        assert_eq!(
            options.profile_selection(),
            ProfileSelection::Explicit("foundation-device".into())
        );
        assert!(options.json);
    }

    fn profile() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let root = tempfile::tempdir().unwrap();
        let person = Uuid::new_v4();
        let directory = root.path().join("people").join(person.to_string());
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(root.path().join("local_device_id"), "cli-test-device").unwrap();
        let database = directory.join("floe.db");
        let vault =
            PathBuf::from(format!("{}.agent-vaults", database.display())).join(person.to_string());
        (root, database, vault)
    }

    #[test]
    fn missing_database_and_vault_are_never_created() {
        let (_root, database, vault) = profile();
        assert!(preflight(&database).is_err());
        assert!(!database.exists());
        std::fs::write(&database, []).unwrap();
        assert!(preflight(&database).is_err());
        assert!(!vault.exists());
    }

    #[test]
    fn busy_vault_is_rejected_without_touching_its_marker() {
        let (_root, database, vault) = profile();
        std::fs::write(&database, []).unwrap();
        std::fs::create_dir_all(&vault).unwrap();
        std::fs::write(vault.join("vault.id"), "do-not-replace").unwrap();
        let lock = std::fs::File::create(vault.join("host.lock")).unwrap();
        lock.try_lock().unwrap();
        assert!(preflight(&database).unwrap_err().contains("busy"));
        drop(lock);
        preflight(&database).unwrap();
        assert_eq!(
            std::fs::read_to_string(vault.join("vault.id")).unwrap(),
            "do-not-replace"
        );
    }

    #[test]
    fn session_trace_does_not_include_capability_payloads() {
        let mut session = AgentSession::new(floe_app::PersonId::new());
        session.messages.push(AgentMessage::Capability {
            turn_id: Uuid::new_v4(),
            call_id: Uuid::new_v4(),
            capability_id: "calendar.read".into(),
            input: "private-input".into(),
            result: Ok("private-source-content".into()),
        });
        let trace = session_trace(&session).to_string();
        assert!(trace.contains("calendar.read"));
        assert!(!trace.contains("private-input"));
        assert!(!trace.contains("private-source-content"));
    }

    #[test]
    fn completed_failed_jobs_are_released_without_resubmitting() {
        let mut observations = Vec::new();
        let result = finish_job(
            VaultLifecycleResult {
                operation_id: Uuid::new_v4(),
                stage: "working".into(),
                done: false,
                state: None,
                failure: None,
            },
            |release| {
                observations.push(release);
                Ok(VaultLifecycleResult {
                    operation_id: Uuid::new_v4(),
                    stage: "failed".into(),
                    done: true,
                    state: None,
                    failure: Some(floe_app::AgentFailure::CapabilityDenied),
                })
            },
        );
        assert!(result.is_err());
        assert_eq!(observations, [false, true]);
    }

    #[test]
    fn halted_sessions_remain_inspectable() {
        let mut session = AgentSession::new(floe_app::PersonId::new());
        session.last_outcome = Some(floe_app::AgentOutcome::Halted {
            reason: floe_app::AgentFailure::CapabilityDenied,
        });
        let result = ConversationSessionResult {
            operation_id: Uuid::new_v4(),
            stage: "completed".into(),
            done: true,
            state: Some(VaultState::Ready),
            session: Some(session),
            failure: Some(floe_app::AgentFailure::CapabilityDenied),
        };
        assert!(result.failure().is_none());
    }
}
