# Debug conversation CLI

The macOS CLI is a thin caller of the same admitted `AppHost` services as the
Flutter client, not another Manager implementation. Configure the Vault, model
connection, connectors, Expert bindings and standing source grants in the client.
Quit the client before opening that profile from the CLI.

## Run

From the repository root:

```sh
database="$HOME/Library/Containers/app.floe.floeClient/Data/Library/Application Support/app.floe.floeClient/development-storage/client/people/00000000-0000-4000-8000-000000000001/floe.db"
./scripts/floe-cli.sh --database "$database" --prompt '오늘 일정 요약해줘'
./scripts/floe-cli.sh --database "$database"
```

Select the actual profile explicitly; the CLI does not discover, create, reset or
copy profiles. It reads the existing device identity and unlocks the existing
Vault through the build-selected custody provider. The default script uses the
isolated development profile and private file keys. For an existing production
profile, pass `--production` before the CLI arguments; this builds Release with
OS keyring custody. A profile mismatch fails closed. The normal single-host Vault lock remains
authoritative. Do not run the client and CLI concurrently against one profile.
In production mode, macOS may ask you to authorize this executable's access to the existing Keychain
items, including again after a rebuild changes its ad-hoc code identity. Handle
that prompt locally; never paste a password into the conversation or CLI input.

The script incrementally builds a Rust example and an ad-hoc-signed native host
bundle under `target/cli/`. It reuses the client's EventKit and Apple model source
and native artifact cache helper; Flutter is not built or launched. After building,
the executable can be called directly:

```sh
target/cli/development/FloeDebugCLI.app/Contents/MacOS/floe_cli --database "$database" --prompt '안녕'
```

Every launch creates a separate conversation session unless `--session UUID` is
supplied. It persists normal conversations and Runs in the selected profile.
`--inspect --session UUID` prints a stored session's answers and execution trace
without starting a turn. Model planning uses the same purpose-based
Gateway-primary/local-fallback policy as the client. The CLI has no model-profile
selection option.

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
- `/cancel`: explicitly cancel the currently observed Run. Other input during a
  Run is rejected, not submitted or queued as another turn.
- `/quit`: exit when idle. Piped input is also supported.

Source-processing review belongs to Connections/Access; interaction resolution and
eligible linked resume belong to Conversation. The CLI observes the linked Run
returned by the owner and has no `/resume` command. Navigation-only and
Expert-binding requirements must be configured in the client; CLI approval cannot
choose a binding or substitute for OS permission. No Action execution or connector
setup command is exposed. Model-produced Action proposals remain proposals.

The CLI waits for the canonical Run to settle; it has no observer timeout that
silently cancels work. EOF does not cancel an admitted Run. Use `/cancel` for
controlled cancellation rather than killing the process.
Run observation uses the host's event cursor and runtime epoch, then reads the
settled receipt; it does not poll encrypted output during model/provider work.
An unavailable admission acknowledgement is not a definite rejection: the CLI
checks the original command ID and, only if no receipt exists, retries the exact
same command. It stays in admission observation rather than inviting another turn.
One-shot exit status is 0 for a successful Run, 2 for a failed/blocked Run, and 1
for a CLI/profile/observation error. A failure is not a synthetic answer.

## Platform scope

Saved Gateway connectors use the existing owner paths. This minimal CLI does not
register Calendar, Attention or Personal acquisition pumps. Native-source reads
that require those pumps report unavailable evidence; embedding the EventKit
library does not establish a registered host or OS permission. The CLI does not
request or modify OS permissions automatically. Gateway-backed sources and
intrinsic local Tasks/Memory do not need native acquisition registration.

## Verification

The [active refactor gates](../plans/2026-10-02-architecture-refactor.md#8-verification-policy-and-final-evidence)
control compilation, build and behavioral exercises. At the authorized build gate,
compile the development example with
`cargo build -p floe-app --example floe_cli --no-default-features --features development-storage`.
The production variant uses `cargo build -p floe-app --example floe_cli --release`.
`./scripts/floe-cli.sh --help` also builds and signs its native bundle before
printing help; it is not a read-only source check. The removed example test suite
is reconstructed only in S3.

Use a configured local testing profile for live reads. Approve only the exact
source-processing decision disclosed by its owner. Never reset a profile after an
open or key-access error.

## Synthetic Manager guidance evaluation

The separate signed local-model smoke executable evaluates Manager choice and
synthesis over a synthetic corpus without reading connected source payloads. The
Foundation mode creates an isolated encrypted diagnostic profile; the server mode
uses the explicitly selected existing profile described below.

```sh
./tools/validation/run-local-model-smoke.sh --exercise-manager-guidance
./tools/validation/run-local-model-smoke.sh --exercise-manager-guidance-server
```

Both modes require `FLOE_MANAGER_EVAL_APPROVED=1`. The server mode additionally
requires `FLOE_MANAGER_EVAL_DATABASE` naming an existing absolute product-profile
database with verified Person/device identity and an available existing encrypted
Vault. It reads the profile's existing verified Gateway credential through the
normal adapter; it does not import a connection file, pair an account, replace
credentials or approve source access. Quit other hosts using that profile first.
`FLOE_MANAGER_EVAL_MODEL_ID` is an optional bounded recording label, not a routing
choice. `FLOE_MANAGER_EVAL_STAGE` accepts `baseline`, `native`, `manager` or `card`
and defaults to `baseline`.

The separate Learner modes require an explicitly selected isolated prepared
profile to perform an exercise:

```sh
./tools/validation/run-local-model-smoke.sh --exercise-learner --profile /absolute/path/to/isolated/people/PERSON/floe.db
./tools/validation/run-local-model-smoke.sh --exercise-learner-expiry --profile /absolute/path/to/isolated/people/PERSON/floe.db
```

Without `--profile`, these modes report `SKIPPED` and exit unsuccessfully. They
do not create a profile, replace keys or configure external accounts. The selected
profile must have an existing Vault, no configured source connections or active
Gateway, and no existing session, pending memory candidate or confirmed memory.
The exercise retains its synthetic conversation and pending memory candidate for
inspection; it does not approve the candidate.

The versioned corpus is `fixtures/manager-guidance/corpus.json`. Record the tested commit, prompt and Card hashes, provider/model configuration, all repetitions, and the fixed case rubrics. Shape checks alone are not behavioral acceptance. Synthetic result replay tests model behavior, not real Expert execution or source correctness. Missing live prerequisites remain unverified; do not retune prompts or relax expectations while implementing the execution plan.
