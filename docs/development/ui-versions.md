# UI iteration checkpoints

Each verified UI iteration has a numbered name (`UI-v001`, `UI-v002`, …), a dedicated commit, and a remote snapshot branch (`ui/v001`, `ui/v002`, …). Local annotated tags use `ui-v001`, etc. Remote snapshot branches are the shared recovery points; do not move or reuse them. They are not deployment branches.

## Before publishing

- Keep each iteration focused and separate unrelated work.
- Run affected tests, the full Flutter suite, analysis, and visual checks. Record platform checks that could not run.
- Publish the commit to main without force, then preserve its exact SHA as a new numbered snapshot branch. Verify that local and remote trees match.
- Summarize the changes, verification, and snapshot link to the user.

## Inspect or restore

Fetch the chosen `ui/vNNN` branch and inspect it in a separate worktree or detached checkout. Never replace a dirty working tree.

To undo a UI iteration on main, review its diff and use a new revert commit. Do not reset or force-push main; later unrelated changes must survive. If dependent iterations exist, revert in dependency order or prepare a reviewed targeted reversal, then rerun validation. Preserve the reverted checkpoints for comparison.

A UI checkpoint is a source snapshot, not a backup of personal data, credentials or external calendar changes. Do not run an older binary against a current personal profile merely to compare appearance. Use isolated synthetic preview data, and check schema/native compatibility before any non-preview rollback.
