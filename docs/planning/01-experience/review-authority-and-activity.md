# Review, Action Authority & Activity

> Status: Product direction and implementation contract

## Product boundary

`Calendar Proposal` is not a standalone Floe feature. Calendar creation is one
action type carried by the same human-in-the-loop pipeline as mail delivery,
memory changes, permission escalation, reports and recovery decisions.

The user-facing concepts are:

- **Review requests** — items that currently need a person's decision.
- **Action permissions** — durable rules for what Floe may do automatically.
- **Activity** — completed, blocked and unresolved work with its audit trail.

An immutable action intent and durable execution ledger remain internal safety
concepts. They are not navigation destinations or product terminology.

## Unified pipeline

```text
Intent / report / permission need
              ↓
        deterministic policy
       ┌──────┼─────────┐
       ↓      ↓         ↓
     deny    ask       allow
       ↓      ↓         ↓
   Activity  Review   validation
              ↓         ↓
          decision   execution
              └────┬────┘
                   ↓
                Activity
```

Intelligence may request work but never supplies its own authority, approval,
policy snapshot, receipt or execution time. Automatic authority skips only the
human decision; it never skips fresh validation, provider permission, conflict
checks, execution idempotency or audit recording.

## Review request kinds

| Kind | Example | Primary response |
| --- | --- | --- |
| Action | Create one Calendar event | `Create event` |
| Permission | Allow future Calendar creates | `Always allow` / `Allow once` |
| Report | Weekly summary is ready | `Mark reviewed` |
| Change | Save an inferred memory | `Save change` |
| Recovery | Calendar result is uncertain | `Check Calendar` |

The primary control names its outcome. `Approve proposal` is not shared product
copy. Closing is neither acceptance nor rejection.

## Review inbox

Today may show a compact **Review requests** section. It contains only active
items that need input now:

- pending decisions;
- approved work that still requires an explicit restored execution;
- unresolved execution requiring safe lookup or user inspection.

Resolved, rejected, expired and blocked items leave the inbox immediately.
Execution progress may remain in the open detail until it finishes, but does not
turn the inbox into history. A failure returns to the inbox only when the user can
take a meaningful recovery action.

## Activity

Activity is a separate destination and the durable audit surface. It contains
manual and automatic actions, decisions, reports, blocks, expiry, execution
results and collection results. The default view uses human-readable summaries;
actor, policy decision, intent/execution ids and external ids live in technical
details.

Removing an item from Review never deletes its action or audit record.

## Action permissions

Action permissions are Person-scoped and default to `ask` for external mutation.
Each supported capability offers:

```text
allow automatically | ask every time | do not allow
```

Rules may later narrow authority by actor/Expert, connector/account, destination,
side effect and time limit. Calendar create, update and delete are separate
capabilities. Guests, alerts and recurrence are separate side effects rather than
implicit parts of `calendar.create`.

Settings may offer cautious, balanced and fully automatic presets. A broad
`Allow all supported actions` choice must enumerate what it covers and remain
reversible. It does not bypass OS permission, connector scope, Expert data access,
fresh safety validation, OS authentication required for sensitive operations, or
capabilities Floe does not expose.

These grants are distinct:

```text
OS/connector access to Floe
        ≠ Expert access to Person data
        ≠ Floe authority to execute without review
```

New or broader capability scopes require another explicit grant. Account/security
changes, payment and other non-delegable actions may remain review-only even under
a broad preset.

## Calendar rollout

Release artifacts include the trusted Calendar create executor. Calendar creation
still requires EventKit access, an eligible writable destination, a current
connection, fresh conflict checks and an `allow` policy or a live review decision.
The default Calendar create authority is `ask`.

Compile-time write exclusion is retained only for explicitly write-disabled test
or recovery artifacts. It is not the production authorization mechanism.

## Domain records

```text
ActionIntent     immutable requested mutation
ReviewRequest    active human decision and presentation contract
AuthorityPolicy allow / ask / deny for a scoped capability
Execution       idempotent external mutation and recovery state
ActivityRecord  durable user-readable audit projection
```

The initial Calendar slice may project its existing durable action ledger into
Review and Activity while the generic records are introduced. New domains must
not add their own proposal inbox.

## Delivery sequence

1. Enable the Calendar executor in Release while retaining runtime policy checks.
2. Project only actionable Calendar records into Review requests.
3. Add a separate Activity destination for terminal Calendar records.
4. Add persistent Calendar create authority with `allow`, `ask` and `deny`.
5. Move Calendar action initiation and copy out of proposal-specific UI.
6. Generalize storage/contracts for report, permission, change and recovery kinds.
7. Add actor, scope and expiry controls plus broad permission presets.

