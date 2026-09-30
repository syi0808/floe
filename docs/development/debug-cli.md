# Debug conversation CLI

The macOS CLI is a thin caller of the same admitted `AppHost` services as the
Flutter client, not another Manager implementation. Configure the Vault, model
connection, connectors, Expert bindings and standing source grants in the client.
Quit the client before opening that profile from the CLI.

## Run

From the repository root:

```sh
database="$HOME/Library/Containers/app.floe.floeClient/Data/Library/Application Support/app.floe.floeClient/people/00000000-0000-4000-8000-000000000001/floe.db"
./scripts/floe-cli.sh --database "$database" --prompt '오늘 일정 요약해줘'
./scripts/floe-cli.sh --database "$database"
```

Select the actual profile explicitly; the CLI does not discover, create, reset or
copy profiles. It reads the existing device identity and unlocks the existing
Vault through its exact Keychain slot. The normal single-host Vault lock remains
authoritative. Do not run the client and CLI concurrently against one profile.
macOS may ask you to authorize this executable's access to the existing Keychain
items, including again after a rebuild changes its ad-hoc code identity. Handle
that prompt locally; never paste a password into the conversation or CLI input.

The script incrementally builds a Rust example and an ad-hoc-signed native host
bundle under `target/cli/`. It reuses the client's EventKit and Apple model source
and native artifact cache helper; Flutter is not built or launched. After building,
the executable can be called directly:

```sh
target/cli/FloeDebugCLI.app/Contents/MacOS/floe_cli --database "$database" --prompt '안녕'
```

Every launch creates a separate conversation session unless `--session UUID` is
supplied. It persists normal conversations and Runs in the selected profile.
`--inspect --session UUID` prints a stored session's answers and execution trace
without starting a turn. `--profile ID` selects an existing model profile; omitted
selection uses the same automatic policy as the client.

## Diagnose and review

Output includes session/Run IDs, Run state, model attempts, Expert Task IDs and
source dependency coverage, plus settled delegation states where retained in the
session. Failed Runs can expose Task/attempt references without transcript records.
A Run with no Expert Task IDs did not delegate; that
alone does not establish whether its answer was grounded in existing context.
The CLI does not dump raw capability inputs, source payloads, credentials or
provider replay. `--json` emits newline-delimited JSON on stdout; build diagnostics
go to stderr. Answers and interaction disclosures can contain personal information.
Keep saved output private.

Interactive commands:

- `/interactions`: display the current session's exact reviewed targets.
- `/approve ID`, `/deny ID`, `/dismiss ID`: decide a displayed interaction using
  its displayed revision and target digest. There is no automatic approval.
- `/refresh ID`: reconcile a displayed interaction after changing settings in
  the client. Quit the CLI before reopening the client on this profile.
- `/resume ID`: explicitly admit a fresh linked Run for that interaction's origin.
- `/cancel`: explicitly cancel the currently observed Run. Other input during a
  Run is rejected, not submitted or queued as another turn.
- `/quit`: exit when idle. Piped input is also supported.

Inline review and exact model-recipient consent remain host-owned. Navigation-only
and Expert-binding requirements must be configured in the client; CLI approval
cannot choose a binding or substitute for OS permission. No Action execution or
connector setup command is exposed. Model-produced Action proposals remain proposals.

The CLI waits for the canonical Run to settle; it has no observer timeout that
silently cancels work. EOF does not cancel an admitted Run. Use `/cancel` for
controlled cancellation rather than killing the process.
Run observation uses the host's event cursor and runtime epoch, then reads the
settled receipt; it does not poll encrypted output during model/provider work.
One-shot exit status is 0 for a successful Run, 2 for a failed/blocked Run, and 1
for a CLI/profile/observation error. A failure is not a synthetic answer.

## Platform scope

Saved server connectors and macOS Calendar reads use the existing owner paths.
The CLI bundle has its own OS code identity: the client's EventKit permission
does not necessarily authorize this process. Permission denial is a real blocked
read, never an empty calendar or synthetic fallback. The CLI does not request or
modify OS permissions automatically.

Flutter-only live Attention/Contacts host publication is not provided by this
minimal CLI. Experts requiring it may report unavailable evidence. Server-backed
sources and intrinsic local Tasks/Memory do not need that publication.

## Verification

```sh
cargo test -p floe-app --example floe_cli
./scripts/floe-cli.sh --help
```

Use a configured local testing profile for live reads. Approve only the exact
source and model recipient disclosed by the host. Never reset a profile after an
open or key-access error.
