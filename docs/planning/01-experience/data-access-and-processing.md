# Data Access & Processing

> Status: Product direction and implementation contract

## Implemented slice

The first Calendar slice now uses **Data & privacy**, domain access cards, an
explicit review action, aggregate pause/resume, immutable scope replacement, and
tombstoned removal. Runtime Installation, Assignment, and View switches no longer
appear in the default Settings experience.

External-model consent also lives in **Data & privacy** and is bound to the
recipient disclosures returned by the paired server. Purpose availability and
recent content-free server processing records are visible without exposing model
selection or provider credentials. Generic grants for other domains, third-party
extension cards, durable server audit storage, and richer trace detail remain
follow-up work.

## Problem

The current **Floe access** page combines several different decisions without
showing their consequences:

- Conversation abilities expose runtime installation and assignment state.
- Calendar setup separately asks for a View scope and a confirmation checkbox.
- Action authority lives on another page.
- Server pairing and external-model consent live together even though pairing a
  server does not itself authorize an external data transfer.

This makes an enabled switch ambiguous. A person cannot tell whether it means
Floe can read data, suggest an action, execute it, or transmit context elsewhere.
Refresh and retry controls also expose consistency mechanics instead of a clear
access state.

## Product vocabulary

Rename **Floe access** to **Data & privacy**. The page answers three questions:

1. **What can Floe use?** Person-approved data sources and scopes.
2. **Where can it be processed?** Device, paired Floe server, or an identified
   external provider.
3. **How do I stop it?** Pause, change scope, or remove access with an explicit
   description of the result.

The following remain separate concepts and destinations:

- **Connections** establish OS or service access to an account or source.
- **Data & privacy** grants Floe a selected subset of connected data.
- **Actions** determines whether Floe may mutate external state automatically,
  ask first, or never act.
- **Floe Server** pairs this device and manages server-side provider routing.

Installation, Assignment, Expert, Tool, and View are runtime terms. They do not
appear in the default settings experience. An optional diagnostics surface may
show identifiers and revisions for support.

## Information architecture

Settings navigation becomes:

```text
Actions
Data & privacy
Floe Server
```

**Data & privacy** contains:

### Data Floe can use

Each domain appears as one card, for example **Calendars**. The card shows:

- status: Not set up, Active, Paused, Needs attention, or Removing;
- human-readable scope, such as “Work and Personal — 2 calendars”;
- allowed use, such as “Read event details and prepare suggestions”;
- controls: Set up, Change, Pause/Resume, and Remove access.

The card does not show independent Tool and Expert switches. For a built-in
domain, its runtime capabilities are enabled atomically with the data grant and
paused atomically with it. Third-party extensions later receive a separate
**Extensions** section with their own publisher and permission summary.

### AI processing

This section reports policy and current availability rather than offering a
conversation-level model picker:

- **On this device** — available local processing paths.
- **On your Floe Server** — paired/unpaired and route availability by purpose.
- **External providers** — off by default; shows the recipient organization and
  allowed data categories before consent is granted.

Floe selects the route for each purpose. A conversation never selects a model.
The UI may explain “Fast response”, “Everyday assistance”, or “Deep work”, but
does not reveal or persist a model choice as conversation state.

### Recent data use

Show a concise list of data access and outbound processing records:

- time and purpose;
- data category and scope, never raw private content in the list;
- placement: device, Floe Server, or named external provider;
- outcome and trace identifier in technical details.

This is a privacy activity projection, not a debugging trace viewer. Replay is
available only from a detail view and must re-run current policy checks.

## Calendar setup flow

Calendar access uses a short review flow instead of an installation form:

1. **Choose calendars** — list only currently connected calendars, with a clear
   remediation link when Calendar permission or the source connection is absent.
2. **Review access** — summarize fields Floe may read, what Floe may suggest, and
   the current processing policy.
3. **Allow access** — the primary action itself records consent; do not require a
   redundant “I understand” checkbox.

The review states that Calendar mutations are controlled separately in
**Actions**. Expanding calendar scope, adding a more sensitive data category, or
allowing a new external recipient requires a new explicit decision. Reducing
scope does not.

After setup, **Change** reopens the calendar selection with the existing scope.
**Pause** stops new conversation access without deleting the grant. **Remove
access** revokes the grant, disables its runtime assignments and views, clears
derived resumable context governed by that grant, and retains only the minimal
audit record required to explain the revocation.

## Processing consent

Pairing a Floe Server authorizes authenticated requests to that server. It does
not authorize the server to forward personal context externally.

External consent is:

- off by default;
- bound to a Person, device, data categories, and recipient organization;
- independent of a particular model name;
- invalidated when the recipient changes or the allowed data categories expand;
- immediately withdrawable without disconnecting the server;
- checked both by the client privacy router and the server gateway.

The server purpose inventory returns a sanitized processing disclosure for each
purpose: availability, local/server/external placement, recipient identity when
external, and whether current consent covers it. It does not expose API keys or
the server operator's model configuration.

If no permitted route is available, the conversation stays usable. Floe explains
which capability is unavailable and offers a policy-safe alternative: local
processing, a less capable permitted route, or retry after server setup. It never
silently broadens scope or external consent.

## State contract

Introduce a Person-scoped `DataAccessGrant` projection:

```text
id
domain
source_connection_id
scope_ids
data_categories
allowed_uses
processing_policy_id
status
revision
granted_at
revoked_at
```

The existing Installation, Assignment, and domain View records remain execution
mechanics behind this projection. One atomic command creates, changes, pauses,
resumes, or revokes the aggregate. Partial updates remain pending internally and
surface as **Needs attention** with one recovery action, not separate conflicting
switches.

Authorization is evaluated as an intersection:

```text
source connection and OS permission
∩ active DataAccessGrant scope
∩ Expert/Tool declared permissions
∩ processing policy and external consent
∩ Action authority for mutations
```

No layer can expand authority granted by another layer.

## Delivery sequence

1. Add the `DataAccessGrant` projection over existing Calendar registry records.
2. Replace capability switches with one Calendar access card and setup flow.
3. Add atomic pause, resume, scope-change, and revoke commands.
4. Move external-transfer consent into **Data & privacy** while keeping pairing
   controls under **Floe Server**.
5. Extend purpose inventory with sanitized placement and recipient disclosure.
6. Add privacy activity from outbound audit records and trace detail/replay.
7. Generalize the cards and grant contract for additional data domains and
   third-party extensions.

The first delivery preserves the existing fail-closed registry and vault behavior.
It changes the user projection before replacing the underlying runtime records.
