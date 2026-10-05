use std::{
    collections::HashMap,
    io::{self, BufRead, IsTerminal, Write},
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver},
    time::Duration,
};

use floe_app::{
    AppComposition, AppHost, VaultLifecycleCommand, VaultLifecycleCommands, VaultLifecycleQueries,
    VaultLifecycleResult, VaultState,
};
use floe_conversation::{
    CommandReceipt, ConversationOwner, InteractionDecisionKind, InteractionSnapshot,
    InteractionStatus, RefreshInteraction, ResolveInteraction, RunReceipt, RunState,
    SessionMessage, SessionSnapshot, StartTurn,
};
use floe_execution::{BoxFuture, Cancellation, ExecutionScope};
use floe_kernel::{AgentFailure, CommandId, OwnerActor};
use serde_json::{Value, json};
use uuid::Uuid;

const HELP: &str = "Floe debug CLI (macOS; quit the Flutter app before opening its profile)
Usage: scripts/floe-cli.sh --database /absolute/path/to/people/PERSON/floe.db [options]
  --prompt TEXT     Run one turn, then exit; otherwise read a line-oriented conversation
  --session UUID    Open an existing session; otherwise create a separate session
  --inspect         Read the selected session without starting a turn (requires --session)
  --json            Emit newline-delimited JSON to stdout (may contain personal answers)
  --help            Show this help without opening a database
Commands: /interactions, /approve ID, /deny ID, /dismiss ID, /refresh ID,
          /cancel (while a turn is running), /quit
No connector setup, Vault creation/reset, automatic approval, or Action execution.
";

type Result<ValueType> = std::result::Result<ValueType, String>;
type Host = AppHost<AppComposition>;

#[derive(Debug, Default)]
struct Options {
    database: PathBuf,
    prompt: Option<String>,
    session: Option<Uuid>,
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
                "--database" | "--prompt" | "--session" => {
                    let value = arguments
                        .next()
                        .ok_or_else(|| format!("Missing {argument} value"))?;
                    match argument.as_str() {
                        "--database" => options.database = value.into(),
                        "--prompt" => options.prompt = Some(value),
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
        Ok(Some(options))
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

fn owner_call<T: Send + 'static>(
    host: &Host,
    operation: impl for<'a> FnOnce(
        &'a dyn ConversationOwner,
        &'a OwnerActor,
        &'a ExecutionScope,
    ) -> BoxFuture<'a, std::result::Result<T, AgentFailure>>
    + Send
    + 'static,
) -> std::result::Result<T, AgentFailure> {
    let request_id = Uuid::new_v4();
    let request = host
        .request(request_id)
        .map_err(|_| AgentFailure::PolicyDenied)?;
    let owners = request.services().ready_owners(request.caller())?;
    let actor = request.caller().owner_actor();
    let scope = floe_app::host_scope(request_id, Cancellation::new(), Duration::from_secs(30));
    request
        .services()
        .execute_owner(async move { operation(owners.conversation.as_ref(), &actor, &scope).await })
}

fn session(host: &Host, session_id: Option<Uuid>) -> Result<SessionSnapshot> {
    owner_call(host, move |owner, actor, scope| {
        Box::pin(async move {
            let id = match session_id {
                Some(id) => id,
                None => {
                    use floe_conversation::{SessionStartAdmission as A, SessionStartFailure as F};
                    // This one-shot CLI exits on uncertainty; it does not retry with a new ID.
                    match owner.start_session(actor, CommandId::new(), scope).await {
                        Ok(A::Started(receipt) | A::Replayed(receipt)) => receipt.session_id,
                        Ok(A::NotApplied(reason)) => return Err(reason.reason()),
                        Err(F::NotAdmitted(reason) | F::Indeterminate(reason)) => {
                            return Err(reason);
                        }
                    }
                }
            };
            owner.get_session(actor, id, None, scope).await
        })
    })
    .map_err(|failure| format!("Session: {failure:?}"))
}

fn session_trace(session: &SessionSnapshot) -> Value {
    let delegations: Vec<_> = session
        .messages
        .iter()
        .filter_map(|message| match message {
            SessionMessage::Delegation { turn_id, task, .. } => Some(json!({
                "turn_id": turn_id, "task_id": task.task_id, "expert": task.agent_id,
                "state": task.state, "failure": task.issue,
            })),
            _ => None,
        })
        .collect();
    let capabilities: Vec<_> = session
        .messages
        .iter()
        .filter_map(|message| match message {
            SessionMessage::Capability {
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
        "model_attempts": session.usage.model_attempts, "delegations": delegations,
        "capabilities": capabilities, "outcome": session.last_outcome})
}

fn show_session(output: &Output, session: &SessionSnapshot) -> Result<()> {
    output.emit("session", session_trace(session))?;
    for message in &session.messages {
        if let SessionMessage::Assistant { turn_id, text, .. } = message {
            output.emit("answer", json!({"turn_id": turn_id, "text": text}))?;
        }
    }
    Ok(())
}

struct Cli {
    host: Host,
    options: Options,
    output: Output,
    current: SessionSnapshot,
    reviewed: HashMap<Uuid, InteractionSnapshot>,
}

fn admit_turn(host: &Host, command: StartTurn) -> Result<CommandReceipt> {
    loop {
        let attempt = command.clone();
        match owner_call(host, move |owner, actor, scope| {
            owner.start_turn(actor, attempt, scope)
        }) {
            Ok(receipt) => return Ok(receipt),
            Err(AgentFailure::StorageUnavailable) => {
                let id = command.command_id;
                match owner_call(host, move |owner, actor, scope| {
                    owner.read_command(actor, id, scope)
                }) {
                    Ok(Some(receipt)) => return Ok(CommandReceipt::from(&receipt)),
                    Ok(None) => {}
                    Err(failure) => {
                        return Err(format!(
                            "Admission acknowledgment is uncertain for {id:?}: {failure:?}"
                        ));
                    }
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(failure) => return Err(format!("Turn admission: {failure:?}")),
        }
    }
}

impl Cli {
    fn interactions(&mut self) -> Result<()> {
        let session_id = self.current.id;
        let interactions = owner_call(&self.host, move |owner, actor, scope| {
            owner.list_interactions(actor, session_id, scope)
        })
        .map_err(|failure| format!("Interactions: {failure:?}"))?;
        self.reviewed.clear();
        for interaction in interactions {
            self.output.emit(
                "interaction",
                serde_json::to_value(&interaction).map_err(|_| "Invalid interaction projection")?,
            )?;
            self.reviewed
                .insert(interaction.interaction_id, interaction);
        }
        Ok(())
    }

    fn turn(&mut self, text: String, input: Option<&Receiver<String>>) -> Result<bool> {
        let command = StartTurn {
            command_id: CommandId::new(),
            session_id: self.current.id,
            expected_revision: self.current.revision,
            text,
            continuation_ref: None,
            retry_of: None,
        };
        let receipt = admit_turn(&self.host, command)?;
        self.observe(receipt, input)
    }

    fn observe(
        &mut self,
        receipt: CommandReceipt,
        input: Option<&Receiver<String>>,
    ) -> Result<bool> {
        self.output.emit("admitted", json!({"command_id":receipt.command_id,"run_id":receipt.run_id,"session_id":self.current.id}))?;
        let mut cancellation_requested = false;
        let finished = loop {
            if let Some(input) = input {
                if let Ok(line) = input.try_recv() {
                    if line.trim() == "/cancel" && !cancellation_requested {
                        let run_id = receipt.run_id;
                        owner_call(&self.host, move |owner, actor, scope| {
                            owner.cancel_run(actor, CommandId::new(), run_id, scope)
                        })
                        .map_err(|failure| format!("Cancel: {failure:?}"))?;
                        cancellation_requested = true;
                        self.output
                            .emit("cancel_requested", json!({"run_id":receipt.run_id}))?;
                    } else {
                        self.output.emit("busy", json!({"message":"Wait for this turn, or use /cancel. Input was not submitted."}))?;
                    }
                }
            }
            let run_id = receipt.run_id;
            let run = owner_call(&self.host, move |owner, actor, scope| {
                owner.read_run(actor, run_id, scope)
            })
            .map_err(|failure| format!("Run observation: {failure:?}"))?
            .ok_or("Admitted Run not found")?;
            if run.state.is_terminal() {
                break run;
            }
            std::thread::sleep(Duration::from_millis(100));
        };
        self.output.emit("run", run_trace(&finished))?;
        if let Some(text) = &finished.output {
            self.output
                .emit("answer", json!({"run_id":receipt.run_id,"text":text}))?;
        }
        self.current = session(&self.host, Some(self.current.id))?;
        self.output.emit("session", session_trace(&self.current))?;
        self.interactions()?;
        let blocked = self.reviewed.values().any(|interaction| {
            interaction.origin_run_id == receipt.run_id
                && matches!(
                    interaction.state,
                    InteractionStatus::Pending | InteractionStatus::Resolving
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
        let session_id = self.current.id;
        let revision = reviewed.revision;
        let digest = reviewed.target_digest;
        let result = if command == "/refresh" {
            owner_call(&self.host, move |owner, actor, scope| {
                owner.refresh_interaction(
                    actor,
                    RefreshInteraction {
                        command_id: Uuid::new_v4(),
                        interaction_id: identifier,
                        session_id,
                        expected_revision: revision,
                    },
                    scope,
                )
            })
        } else {
            let decision = match command {
                "/approve" => InteractionDecisionKind::Approve,
                "/deny" => InteractionDecisionKind::Deny,
                "/dismiss" => InteractionDecisionKind::Dismiss,
                _ => return Err("Unknown interaction command".into()),
            };
            owner_call(&self.host, move |owner, actor, scope| {
                owner.resolve_interaction(
                    actor,
                    ResolveInteraction {
                        command_id: Uuid::new_v4(),
                        interaction_id: identifier,
                        session_id,
                        expected_revision: revision,
                        decision,
                        target_digest: digest,
                    },
                    scope,
                )
            })
        }
        .map_err(|failure| format!("Interaction: {failure:?}"))?;
        self.output.emit(
            "interaction_result",
            serde_json::to_value(&result.interaction)
                .map_err(|_| "Invalid interaction projection")?,
        )?;
        if let Some(receipt) = result.linked {
            self.observe(receipt, Some(input))?;
        } else {
            self.interactions()?;
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
                if ["/approve", "/deny", "/dismiss", "/refresh"].contains(&command) {
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
