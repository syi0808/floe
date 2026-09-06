# Calendar direct manipulation

## Automated coverage

- Flutter: segmented typing, arrow keys, visible time steppers, invalid minutes,
  narrow composer layout, explicit create without Review, persisted direct origin
  under all automation policies, uncertain work excluded from Review.
- Calendar interaction: 15-minute snapping, ghost, Escape, invalid outside drop,
  edge scrolling, right-click edit, More/delete, Shift+F10 and read-only menu state.
- Custom popovers: leap-year month changes, adjacent days, keyboard date selection,
  Escape/outside dismissal, viewport collision, focus restoration and disabled
  menu item skipping. Mouse drag runs under an iOS-themed desktop too; gesture
  availability no longer depends on the visual theme's platform setting.
- Regression: hold the primary mouse button for 700ms before moving. The old
  shared long-press recognizer opened a menu and consumed the drag; the mouse
  must now start dragging, while touch long-press still opens the menu.
- Unresolved mutations lock their own event only. Direct edits accept overlap
  while automatic creates retain conflict validation. Connected calendars refresh
  capability metadata on startup. Existing EventKit alarms are preserved.
- Rust: direct approval persistence, past-date entry, one-shot execution, original
  provider snapshot, source capability rejection, stale mirror blocking and
  compatibility with existing automation proposals.
- Native pure checks: target identity, self-overlap exclusion, other-event overlap,
  delete intent, DST interval preservation and permission-free capabilities.

Run from the repository root:

```sh
cargo test -p floe-core -p floe-protocol -p floe-ffi
cd apps/client
flutter analyze --no-pub
flutter test --no-pub
flutter build macos --debug --no-pub
```

The native pure harness can be compiled with `EventKitActions.swift` and a temporary
copy of `tools/s3-validation/native-tests.swift` named `main.swift`. It does not
request OS access, read events or save/delete any provider event.

## Live provider acceptance (not automated)

Use a disposable writable EventKit calendar with explicit user consent. No live
personal-calendar mutation is required by the automated suite.

1. Create an event with typed and stepped times. Confirm exactly one provider
   event, successful re-import, Activity ownership and no Review request.
2. Drag it 30 minutes, cancel another move with Escape, and drop another outside
   the grid. Only the first move may change the provider event.
3. Edit title and move to another date. Verify duration, original event URL/time
   zone, target identity and both days after refresh.
4. Confirm deletion; verify absence after re-import. Cancel deletion of another
   event and verify that it is untouched.
5. Change the event in the source app while the editor is open. Saving must refuse
   to overwrite stale state. Revoke permission or make the calendar read-only
   after import and verify that native validation also refuses the mutation.
6. Verify spring-forward and fall-back dates on a device in that time zone.
7. Simulate response loss only on disposable data. Lookup must not issue another
   write. Delete absence is intentionally unresolved without a durable native
   receipt; do not interpret it as evidence that Floe deleted the event.

## Delivery boundaries

The current surface is a day view: cross-date pointer dragging and resizing are
not implemented. Use Edit's date/end fields. Recurring/all-day events, guests,
new alert configuration, duplicate, calendar reassignment and Undo remain unavailable. Old actions
without direct-origin metadata are not reclassified or deleted from history.
