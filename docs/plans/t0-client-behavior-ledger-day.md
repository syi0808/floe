# T0 client behavior ledger: Day, Calendar projection and acquisition scheduling

Baseline: `3f4b407f8079d611224cd7adbef121f9e7e75e8e`. Full-source static extraction; no execution or removal. D = durable safety/property; P = product hypothesis; O = obsolete representation; H = harness. Mixed classification preserves safety meaning without freezing old shape. Entry headings provide source registration/span; the file hash binds all prose to exact baseline bytes.

## apps/client/test/features/day/calendar_agenda_interaction_test.dart

Full source read: lines1–150; SHA-256 `ea937c0f106da51fa5220ddab24fda45963c0da7542ae1691812ec77f09c975f`.

Current owner: CalendarAgenda presentation and draft callbacks. Target: apply the D/P/O/H disposition above at that canonical owner; exact imported dependencies are recorded for this path in the source ledger.

### 'current time label replaces an overlapping hour label' (testWidgets; lines9–55; P)

At900×760 empty Day generated10:05, show current-time label and suppress overlapping10:00 hour while retaining09:00/11:00. Empty-day status is positioned and contains Material. Exact visual structure is not architecture policy.

### 'empty day keeps its grid active and supports double-click create' (testWidgets; lines57–149; P/D)

For a connected empty Day, show breathing-room/banner text centered within viewport and pointer-transparent, while timeline remains interactive with zoom divider. Double-click grid center50ms apart creates a draft callback on the selected civil date snapped to15minutes, replaces empty banner with New event and causes no exception. No external create/approval occurs; draft creation is user intent only.

## apps/client/test/features/day/calendar_context_rail_test.dart

Full source read: lines1–69; SHA-256 `186a04164146c1706f85c731a266ef9c577fb1fef093fb77ec91eff94ca502df`.

Current owner: CalendarContextRail inspection versus completion affordance. Target: apply the D/P/O/H disposition above at that canonical owner; exact imported dependencies are recorded for this path in the source ledger.

### 'task title uses a pointer when disabled is $disabled' (testWidgets; lines13–67; P/D)

Two registrations cover completion enabled and disabled. Mouse over task title always shows click cursor and tapping opens that identical task; checkbox cursor is click when enabled, forbidden when disabled. Inspection remains available even when completion mutation is unavailable. The test does not tap the checkbox, so callback suppression itself is not asserted.

## apps/client/test/features/day/calendar_day_boundary_test.dart

Full source read: lines1–88; SHA-256 `21935630199393f8931b07981625758579e6b96989666e348f630eeac81c7f90`.

Current owner: CalendarDayAxis / EventKitCalendarAdapter external native boundary. Target: apply the D/P/O/H disposition above at that canonical owner; exact imported dependencies are recorded for this path in the source ledger.

### 'local DST axis preserves elapsed time and distinguishes repeated hours' (test; lines9–27; P/D)

Build spring/fall civil-day axes. Only when host March8 offset is Pacific -28800 does the full branch assert spring1380/fall1500minutes, skipped03:00 label, distinguished repeated01:00 labels, late-fall instant at minute1485 and final24:00 label. Other hosts assert spring1440 then return, so they do not cover DST behavior. Preserve elapsed-time/civil-date meaning; exact axis rendering is product evidence.

### 'EventKit reads civil-day endpoints, not a fixed 24-hour interval' (test; lines28–62; D)

Mock floe/calendar channel and request spring offsets(-28800→-25200) and fall(-25200→-28800). Native read arguments span23h and25h respectively, not fixed24h. calendars(requestAccess:false) sends exactly request_access:false. No real EventKit prompt/read occurs.

### 'EventKit preserves the native provider identity' (test; lines63–87; D/O)

Mock native Calendar inventory with event_kit provider and inspect using local-device-1. Returned provider remains event_kit and channel args preserve that device plus request_access:true. It verifies identity translation at method channel, not actual OS authorization.

## apps/client/test/features/day/calendar_event_details_test.dart

Full source read: lines1–58; SHA-256 `cd40545379a1316e75c84f5fa1a250d1510c6dd67f76b2abcebd2b07ad09138a`.

Current owner: openCalendarEvent / CalendarEventDetails read-only presentation. Target: apply the D/P/O/H disposition above at that canonical owner; exact imported dependencies are recorded for this path in the source ledger.

### 'event details close with X without a duplicate footer' (testWidgets; lines9–57; P)

Open a one-hour external Team meeting in a localized themed dialog. Show title/Source details, no duplicate Back to my day footer/FilledButton/timezone/UTC labels. X Close removes details and restores launcher without exception. Read-only presentation does not mutate the external event.

## apps/client/test/features/day/calendar_observation_publisher_test.dart

Full source read: lines1–402; SHA-256 `b41e0dd31d6c464a59a5db94303a0918192649318a9e2e3e98332d4c7081073f`.

Current owner: CalendarObservationPublisher / LocalContextTransport native projection. Target: apply the D/P/O/H disposition above at that canonical owner; exact imported dependencies are recorded for this path in the source ledger.

### 'publishes eleven sources independently of the four-source grant limit' (test; lines7–45; D/P/O)

Build a ready native EventKit source with11 selected calendars and one empty-success batch each. Publish once; recorded calendar IDs contain all11 and there is no revocation. Projection admission must not inherit the obsolete four-source grant limit;11 is fixture evidence, not new product limit.

### 'publishes connection and device bound calendar batches' (test; lines47–91; D)

Publish Home at a fixed observation/range with device-1 and source revision8. Exactly one recorded publication preserves person/device,revision,provider,selected IDs,timestamps,expiry=observed+freshness and the identical batches object. Important limit: recorder drops connectionId, so this assertion does not independently verify the label's claimed connection-ID binding.

### 'preserves partial failure and empty batch records' (test; lines93–126; D/P)

Using dormant Android source fixture, publish Work permission_denied with empty records alongside Home empty success. Preserve both batches unchanged; a failed acquisition cannot be represented as a successful empty result. Reading this fixture does not authorize Android work.

### 'revokes instead of publishing an unbounded calendar scope' (test; lines128–167; D/P)

Give EventKit maxCalendarCount+1 resources/batches. Publish nothing and revoke only this person/device calendar.timeline observation. Bounded projection failure must not truncate silently or retain stale evidence; exact cap is a target product choice.

### 'ignores server provider and revokes device observation' (test; lines169–199; D)

Present a Google Calendar server source to the native publisher: it produces no native publication. An explicit subsequent revoke removes the device calendar.timeline projection once. The test does not prove publish itself revokes, since revoke is called separately.

## apps/client/test/features/day/calendar_observation_refresh_test.dart

Full source read: lines1–135; SHA-256 `8db45c87f0a0a6485e9e7ebd87fe1e0144209d582f60ce502820c4ddeb497e22`.

Current owner: CalendarObservationRefreshCoordinator acquisition scheduling. Target: apply the D/P/O/H disposition above at that canonical owner; exact imported dependencies are recorded for this path in the source ledger.

### 'refreshes before observation expiry and rearms after success' (test; lines8–33; D/P)

Reconcile a connected EventKit view into a manual scheduler. It schedules refresh after3minutes; manually fire, perform one refresh and schedule another3minute task on success. Freshness renewal is required; cadence is a legacy product choice and no real background timer runs.

### 'coalesces simultaneous resume and expert refresh triggers' (test; lines35–56; D)

While one ensureFresh is pending, call it again from a second trigger. Both callers receive the identical future and only one refresh. Complete it successfully and both settle; coalescing avoids parallel duplicate acquisition.

### 'permission revocation cancels future refreshes' (test; lines58–72; D)

An active refresh returns permission_denied. ensureFresh throws, coordinator becomes inactive and all future scheduled tasks are canceled. Subsequent ensureFresh does not add another schedule. Revocation cannot leave autonomous acquisition armed.

### 'disconnect and dispose cancel scheduled refresh' (test; lines74–87; D)

Reconcile disconnection(null) and require no active tasks; reconnect and then dispose, again require no tasks. Host/view scheduler disposal is acquisition lifecycle cleanup, not cancellation of unrelated Runs.

## apps/client/test/features/day/calendar_panel_test.dart

Full source read: lines1–305; SHA-256 `4e9f0c37695b1ab742c7430f4e6e2aabef7ddaa59cb1d530724d8bcba021740b`.

Current owner: CalendarPanel / CalendarSourceGateway Connections source setup. Target: apply the D/P/O/H disposition above at that canonical owner; exact imported dependencies are recorded for this path in the source ledger.

### 'groups connected calendars without merging names at $width' (testWidgets; lines123–178; P)

At390 and1200px, render four selected calendars. Group two iCloud calendars, one long-account calendar and one unprefixed Other without merging duplicate Home names or truncating Work, planning · 팀 일정. Show4 calendars and correct group counts; old aggregate title absent/no exception. Exact grouping syntax is a presentation hypothesis.

### 'preselects calendars, requires a selection, saves multiple, and cancels' (testWidgets; lines181–256; D/P)

Open reconnect/change flow, Continue and inspect selection: existing Home is prechecked. Uncheck all and Continue must remain in picker with two checkboxes/radio options and no saved selection. Select Home+Work and Continue: exactly those IDs saved, one sync and one changed callback. Reopen, alter selection and Cancel: no second sync/change. Explicit source selection is distinct from sync; cancel cannot commit.

### 'shows EventKit access state and recovery action' (testWidgets; lines258–304; D/P)

For macOS EventKit system access denied, render Needs attention in system-access row. Invoke recovery control and call openCalendarSettings exactly once. This is mocked OS-settings routing, not restored permission or a real prompt.

## apps/client/test/features/day/day_loading_test.dart

Full source read: lines1–121; SHA-256 `fb9dcdc6ec0fe9e98cc691e54afbbe035af3c65ba1f94e3c23220020e80da7b2`.

Current owner: PersonalDayController view/query freshness. Target: apply the D/P/O/H disposition above at that canonical owner; exact imported dependencies are recorded for this path in the source ledger.

### 'date navigation syncs the selected day and ignores older results' (test; lines50–84; D/P)

Complete initial load, then navigate forward one day and back two while syncs are pending. Complete newest previous-day result before older next-day result; displayed date stays newest and ready. goToday then syncs current civil date; only initial operation uses loadDay, navigation uses syncCalendar. Preserve freshness ordering; current-date calculation is clock-dependent.

### 'older loads cannot overwrite a more recent date' (test; lines86–98; D)

For a DayGateway without Calendar sync, race initial load with next-day load, complete newer first then old. Final snapshot remains September5/ready; stale results cannot replace newer navigation.

### 'failure stops loading and a retry completes' (test; lines99–112; D/P)

Fail a pending read with offline StateError: loading ends in failure. Explicitly load again and complete successfully: ready state and error cleared. Read retry is separate from mutation retry.

### 'load completion after disposal does not notify' (test; lines113–120; D)

Dispose controller while load future pending, then complete it. Await succeeds without notifying a disposed controller; no backend mutation/cancel is requested. No listener counter is asserted, so claim safe completion rather than independently proven zero notifications.

## apps/client/test/features/day/fake_day_gateway_test.dart

Full source read: lines1–41; SHA-256 `621c04aa3e8b828f36c8c2bed4a05804ed899f60ee8e0ef2f789610c1141f30f`.

Current owner: FakeDayGateway production-preview in-memory Day behavior. Target: apply the D/P/O/H disposition above at that canonical owner; exact imported dependencies are recorded for this path in the source ledger.

### 'capture classification replaces the batched snapshot' (test; lines6–27; P/H)

Product-preview FakeDayGateway trims padded Buy milk capture then classifies as TaskDraft; returned replacement snapshot contains exactly one TaskItem with that title. The fake is production-preview code and remains despite deletion of these tests.

### 'empty capture is rejected before classification' (test; lines29–40; D/P)

Whitespace-only capture rejects with FormatException before classification. No empty fake item may be created; precise normalization follows target product admission review.

## apps/client/test/features/day/native_day_gateway_test.dart

Full source read: lines1–197; SHA-256 `c916d8eefde0199315972faea1738a1fe86327b333747c5b302f98994767aa91`.

Current owner: NativeDayGateway / AppRuntime Connections and Day persistence owners. Target: apply the D/P/O/H disposition above at that canonical owner; exact imported dependencies are recorded for this path in the source ledger.

### 'remote Calendar source persists independently of the Day mirror' (test; lines12–119; D/H)

If debug FFI is missing, explicitly skip (no macOS guard). In a private real FFI/Turso profile, bind remote Google Calendar source at revision1 with one exact resource. Day mirror remains absent. Close/reopen and require source connection/authority/revision persist independently. Rename resource label with current CAS: revision increments but authority stays; stale prior-revision rename throws NativeTransportException. Disconnect changes authority and removes inspect visibility; rebinding same disconnected ID without current authority throws. Finally drain/close and delete only the private test directory; no provider call or remote data reset is performed.

### 'Rust/Turso gateway persists the complete task lifecycle' (test; lines121–196; D/P/H)

With real debug FFI/Turso in private fixed-clock profile, load empty Day, capture/classify a Korean task, complete then reopen it, capture/classify an event spanning now and a note, and require current-event ID/note title. Close/reopen: all three items persist. Delete each through gateway, reopen again and remain empty. Covers durable task/event/note lifecycle and deletion visibility, not external Calendar writes. Missing dylib skips; teardown touches only owned temp files.

## Helpers, dependencies and target owner

All ten Day files were read in full, including each helper after main. CalendarObservationPublisher translates device-local Calendar projections into LocalContextTransport; _RecordingTransport records only publications/revocations and supplies no-ops for every unrelated acquisition API. A broad implemented interface is not broad coverage. _CalendarPublication omits connectionId as noted above. _source constructs explicit native/Android/server fixtures, not live source authority.

CalendarObservationRefreshCoordinator owns scheduled acquisition refreshing; _ManualScheduler/_ManualTask are test-only controllable callbacks with no real periodic job. PanelCalendarGateway mocks source selection, access/settings and sync using product FakeDayGateway. Its reconcile/disconnect methods merely return current state and remote binding throws, so those are not tested capabilities. DelayedGateway/DelayedCalendarGateway own completers solely to expose races. Native Day tests use shared TestAppHost and a real library, unlike these mocks.

Target mapping: preserve source-owned Connections revision/authority separately from Day mirror revision, client stale-result suppression and bounded Context projection. Day presentation and callback-only drafting stay product hypotheses. Native adapters, assets, production FakeDayGateway and all real provider code are KEEP unless separately reviewed. No Android execution or parity work is implied by dormant fixtures.
