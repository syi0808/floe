# UI iteration checkpoints

Each verified UI iteration has a numbered name (`UI-v001`, `UI-v002`, …), a reviewed commit or focused, reviewable commit range, and one immutable remote snapshot branch (`ui/v001`, `ui/v002`, …). Local annotated tags such as `ui-v001` are optional. Remote snapshot branches are the shared recovery points; do not move or reuse them. They are not deployment branches.

## Before publishing

- Keep each iteration focused and separate unrelated work.
- Run affected tests, the full Flutter suite, analysis, and visual checks. Record platform checks that could not run.
- Record the expected old `main` SHA and verify the complete candidate commit range and tree.
- Create the new immutable numbered snapshot branch at the exact verified candidate. Then verify that remote `main` still equals its expected old SHA and advance it to the candidate with a normal non-force update. If `main` moved, stop and reintegrate; never force the update. Verify both refs and their trees after publication.
- Summarize the changes, verification, and snapshot link to the user.

## Inspect or restore

Fetch the chosen `ui/vNNN` branch and inspect it in a separate worktree or detached checkout. Never replace a dirty working tree.

To undo a UI iteration on main, review its complete change set or commit range, not only the snapshot tip commit, and make a new reviewed reversal commit on top of current `main`. Preserve later unrelated work; do not reset or force-push `main`. If dependent iterations exist, reverse them in dependency order or prepare a reviewed targeted reversal, then rerun validation. Preserve the immutable checkpoints for comparison.

A UI checkpoint is a source snapshot, not a backup of personal data, credentials or external calendar changes. Do not run an older binary against a current personal profile merely to compare appearance. Use isolated synthetic preview data, and check schema/native compatibility before any non-preview rollback.
