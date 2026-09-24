# ADR 0028: Pairing-integrated authority and connection-scoped permissions

- **Status:** proposed
- **Date:** 2026-09-13
- **Amends:** ADR 0010 pairing experience and ADR 0027 consent presentation
- **Extends:** ADR 0025 Person-owned connection boundaries

## Context

The current Remote server settings expose **Server authority enrollment** after an app has already
paired with a server. The user must inspect the server producer identity, enroll the local vault as
an issuer, open the authenticated server dashboard, approve the issuer, return to the app and refresh
the enrollment status. Calendar and remote-view grants are then managed in the same server settings
section.

The protocol separates pairing, issuer enrollment and data-access grants for valid security reasons,
but the UI presents those protocol boundaries as separate product concepts. A single operator is
asked to approve the same app/server relationship twice and understand producer, audience, issuer
and enrollment terminology before they can make the decision they care about: which connected
account Floe may use, for what capability and on behalf of which feature.

Permissions are also displayed too far from their subject. A user with personal and work accounts
does not grant abstract “Google Calendar” access; they grant access to specific resources belonging
to one connection. A server-wide consent form makes account identity less visible and becomes harder
to navigate as connectors expose more capabilities.

The same problem already exists for the device-owned macOS Calendar connection. Connections owns
EventKit setup and calendar selection, while **Data & privacy** owns Calendar Expert access and saved
Calendar scopes. The user must mentally join two screens to understand whether macOS granted source
access, which calendars Floe selected and which features may use them. These controls describe layers
of one connection and belong together on its detail screen.

This ADR changes the product ceremony and information architecture. It does not remove proof of
possession, mutual identity pinning, least privilege, durable revocation or runtime authorization.

## Decision

### 1. Pair once, with authority enrollment inside pairing

The user-visible remote-server flow has one trust ceremony: **Pair server**. Pairing establishes the
app credential and the local vault issuer relationship as one approved trust bundle. There is no
separate **Server authority enrollment** section or routine enrollment action in Settings.

The pairing bundle binds at least:

- the server instance, audience and producer public-key fingerprint;
- the Person, client and device identities;
- the local vault issuer key and fingerprint; and
- the comparison-code attempt and its expiry.

The server administrator's approval applies to that exact bundle, not to a bearer token that may
later enroll an arbitrary key. The client accepts the pairing only after it verifies the server's
signed identity and the server has durably activated the bound issuer. Client credential issuance,
identity pinning and issuer activation must complete together or leave a recoverable unpaired state.

The existing enrollment challenge, proof-of-possession and durable issuer record may remain as
internal protocol steps. The app orchestrates them as part of pairing and does not expose their
implementation vocabulary. Private signing keys remain encrypted in the local vault and are never
sent to the server.

Pairing grants no connector data access. A later, explicit connection to a concrete supported
first-party source/account is a separate product event: once account/system authority and the current
resource selection are established, Floe creates or reviews the exact default first-party Observe
grant set. Act remains denied until separately authorized.

### 2. Treat identity changes as repair, not routine setup

Routine launches do not ask the user to review fingerprints. A new trust ceremony is required when
the pinned server producer changes, the server state is reset, the local vault issuer changes, or
the requested Person/device identity no longer matches the pairing.

The ordinary UI describes this as a changed or unverifiable server identity and offers **Pair again**.
An expandable security-details view may show instance IDs, audiences and fingerprints for diagnosis
and advanced verification. It must not turn a mismatch into a warning that can be casually skipped.

### 3. Manage grants on each connection

Permission creation and expansion live on the detail screen for the connection that owns the data.
The connection header identifies the provider, account, Person, execution location and connection
state. Its **Permissions** section lists the capabilities that the connector can provide.

Each connection shows, in user language:

- the capability, such as Calendar events or Mail search;
- the exact selected resource set, such as calendars, account view or bounded category;
- the product-approved first-party features that can use it;
- Observe and Act authority separately where both exist;
- processing or remote-transfer implications that require consent; and
- its state: Off, Active, Paused, Needs review or Unavailable.

The connection detail has one **Use with Floe** control. Its state is a projection over current
connection/source/system state and Access-owned grants, not a persisted authorization bit. Turning
it off pauses the connection's product-owned first-party Observe grants while preserving credentials,
system permission and selected resources. Turning it on performs fresh source/resource validation
and exact grant review. Inspecting or discovering an old connected profile is read-only and never
creates a grant.

An explicit resource edit while Use with Floe is active coordinates the connection owner's current
selection and the exact Observe scope as one product interaction. While paused, selection can change
without activating Observe; a later On reviews the latest selection. Uninitiated provider-side drift
never widens a grant. Connector type alone is never a permission scope.

Connection removal revokes its grants before credentials and connection metadata are deleted.
Pausing a connection or permission blocks later admissions and releases without claiming to recall
data already released or provider actions already accepted.

#### macOS Calendar first vertical move

The macOS Calendar connection detail is the first connection to adopt the complete model. It brings
together three distinct layers without conflating their authority:

1. **System access** reports EventKit authorization and offers the operating-system permission or
   recovery action.
2. **Calendars available to Floe** selects the exact calendars exposed by this connection.
3. **Use with Floe** manages saved Observe grants for the exact current first-party readers, including
   the Calendar access currently edited under **Data & privacy**.

The screen explains that system access only makes calendars available, selection only establishes the
maximum connection scope, and an Observe grant authorizes actual source use. Effective access is
their intersection. Grant resource choices cannot exceed the currently selected calendars. An
explicit selection change while Use with Floe is active reviews the matching new scope; a failed
review leaves Needs review rather than silently retaining a stale or wider authorization.

Existing Calendar Tool/Expert installation state may be shown as implementation detail under the
relevant Floe feature, but users grant a capability to a named feature rather than “installing a
disabled Expert.” Package installation, source selection and grant mutation remain separate Core
operations even when one connection screen coordinates them.

**Data & privacy** removes its editable Calendar access section. It may show a Calendar permission
summary and link to **Connections → macOS Calendar**, consistent with the global overview rule below.
Memory, processing-location and other privacy controls that are not owned by one connection remain in
**Data & privacy**.

### 4. Keep a global permissions overview

Settings may retain a top-level **Permissions** screen for visibility, auditing and emergency
control. It groups active and paused grants by connection and answers “which account data can be
used by which Floe feature?”

The overview may navigate to a connection, pause a grant or revoke it. It does not create permissions,
select new resources or expand scope. Those operations return the user to the owning connection so
account identity remains visible at the decision point.

### 5. Ask in context, finish on the connection

When a Floe feature needs unavailable access, it explains the missing capability and identifies the
target connection. The user may continue to that connection's permission section. The conversation
or feature does not silently grant access, choose another account, or interpret pairing as consent.

After approval, the initiating experience may resume only with a freshly validated grant. Denial,
cancellation and unavailable sources remain normal recoverable outcomes rather than pairing errors.

## Experience model

```text
Remote server
  Pair server ── one comparison and approval ceremony
       │
       ├── app credential
       ├── pinned server producer identity
       └── active local vault issuer

Connections
  Google · personal@example.com
    Permissions
      Calendar events · Personal calendar · Assistant          Active
      Mail search     · This account      · Communication      Off

  Microsoft 365 · work@example.com
    Permissions
      Calendar events · Work calendar     · Commitments        Paused
      Mail search     · This account      · Work context       Active

  macOS Calendar · This Mac
    System access          Full access                             Allowed
    Available to Floe      Personal, Work
    Use in Floe
      Calendar events      Personal          Assistant, Schedule  Active

Settings → Permissions
  Cross-connection overview, navigation, pause and revoke only
```

## Security and authorization invariants

- Pairing remains authentication of a client/server relationship, not authorization to read source
  data or send it to a model.
- Server producer and local issuer identities are mutually verified, signed and durably pinned.
- Management approval is bound to the exact issuer fingerprint included in the pairing attempt.
- A paired client cannot enroll or replace another issuer without a new explicit pairing ceremony.
- Runtime access remains the intersection of current connection authority, resource scope, grant,
  consumer, purpose and processing policy defined by ADR 0027.
- Flutter presents and requests consent but does not become the authorization enforcement point.
- Identity mismatch, ambiguous ownership, stale consent and failed durable activation fail closed.
- Default first-party Observe never implies Act permission or approval for an external model
  recipient. Exact-recipient processing authority remains separate and fail-closed.
- Observe permission never implies Act permission, and preview never implies either permission.

## Migration

1. Extend the pairing contract to bind producer and issuer identities to the approved attempt.
2. Make pairing completion activate the issuer durably before returning an accepted connection.
3. Move existing Calendar access management from Data & privacy to the macOS Calendar detail.
4. Present EventKit state, selected calendars and consumer grants as separate layers on that screen.
5. Add the same capability and grant-management pattern to other connection detail screens.
6. Move Calendar and remote-view grant creation out of Remote server settings.
7. Replace the enrollment section with pairing health and optional security details.
8. Add the cross-connection permissions overview without scope-expansion controls.

Existing local development state may be reset. No compatibility flow is required solely to preserve
disposable pairings or grants. Migration must not reinterpret an old pairing as proof that a particular
issuer fingerprint was approved; reset or explicit repair is preferred.

## Alternatives not selected

- **Keep enrollment as a separate advanced step:** preserves protocol-shaped UI and leaves ordinary
  users with two approvals for one server relationship.
- **Remove issuer enrollment entirely:** pairing bearers would become sufficient authority for
  protected remote source use and weaken the proof and revocation boundaries.
- **Manage all grants only in Settings:** provides a compact implementation but obscures which account
  and resources own a permission, especially with multiple accounts of one connector type.
- **Keep macOS Calendar access in Data & privacy:** keeps an existing screen stable but splits EventKit
  health, calendar selection and AI-use grants across unrelated navigation paths.
- **Manage grants only at first use:** improves contextual prompting but makes later inspection,
  changes and revocation difficult to discover.
- **Allow grant expansion from the global overview:** recreates the detached account context that this
  decision removes.

## Consequences

- The common path becomes familiar: pair a server, then connect an account to establish its bounded
  first-party Observe policy without a second protocol-shaped grant ceremony.
- Cryptographic enrollment remains auditable without being a product concept users must learn.
- Connection detail screens need a consistent resource and Use with Floe component.
- Pairing becomes a larger atomic security transaction and requires coordinated Go, Rust and Flutter
  protocol changes.
- Server operators lose a separate routine issuer-approval queue; exact issuer details remain available
  in pairing approval, security details and audit records.
- The global overview remains useful for safety without becoming a second permission editor.

## Acceptance criteria

- A new user can pair once without encountering enrollment, producer, audience or issuer terminology.
- Successful pairing leaves a durably pinned producer and active issuer, but zero connector grants.
- Successful explicit connection of a supported source reviews its exact current first-party Observe
  grants; merely discovering an old connection never does.
- Failure during credential or issuer activation does not produce a partially trusted usable pairing.
- Changing either pinned identity blocks protected access and requires explicit repair or re-pairing.
- Permission creation and scope expansion occur only within the owning connection's detail context.
- Two accounts of the same connector display and enforce independent resources and consumers.
- The global permissions screen can explain, navigate, pause and revoke, but cannot expand access.
- macOS Calendar system access, selected source scope and consumer grants are distinguishable and
  manageable from the macOS Calendar connection detail.
- Data & privacy no longer creates, enables or expands Calendar grants and links to the owning
  connection when the user needs to change them.
- Selecting an additional macOS calendar while Use with Floe is active coordinates a fresh exact
  Observe review; while paused it changes selection without activating Observe.
- Deleting a connection revokes its grants and prevents subsequent admission and release.
- Focused end-to-end validation covers pair, grant, use, pause, revoke, identity change and re-pair.
