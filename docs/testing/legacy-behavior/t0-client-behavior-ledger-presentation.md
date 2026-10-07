> Historical behavior evidence, not current execution instructions or a passing test result. See [the evidence index](README.md) and the [active plan](../../plans/2026-10-02-architecture-refactor.md).

# T0 client behavior ledger: Design system, preview and App shell

Baseline `3f4b407f8079d611224cd7adbef121f9e7e75e8e`; branch `refactor/architecture-20261002`.

This is the pre-removal behavior record for the 14 enumerated source files. It contains manually interpreted setup/action/outcome/failure semantics, not only test labels or source excerpts. No owned source was changed or removed.

Coverage: 14 full-read files, 2019 source lines, 48 direct registration sites, 59 expanded test registrations, 104 behavioral scenarios. Counts are source-derived, not executed tests.

## Classification and interpretation

- **D**: Durable safety/property to re-prove at the canonical owner after S2; old implementation shape is not required.
- **P**: Product/presentation hypothesis for reassessment, including geometry, keyboard behavior, accessibility, wording and feedback timing.
- **O**: Obsolete representation or source-shape assertion to retire rather than reproduce as architecture policy.
- **H**: Harness/helper only, with no independent product authority.

Each scenario inherits its exact source path/file SHA-256, owner and dependencies from its file section. Each registration supplies its exact symbol, inclusive span and source hash. Fixed registration tables and distinct inner behavioral tables are expanded; runtime-derived collections are explicitly quantified. Re-proving D meaning never requires preserving an obsolete widget/API shape.

Read authorities: `AGENTS.md`; both repo-local architecture/change-verification skills; `docs/README.md`; current architecture README/invariants; `docs/plans/README.md`; architecture-refactor §7; full client-tests-plan; relevant owned-file entries in client-symbol-map and client-test-symbol-map, plus coordinated T0 registration metadata. No old test statement automatically becomes a product requirement.

## Consumer boundaries and H support

- consumer: apps/client/test/app/app_shell_test.dart:5,20–29,72,91,133–156; dependency: apps/client/lib/features/day/application/fake_day_gateway.dart; decision: KEEP production preview implementation; imported by product preview/other tests, not exclusively test support. No source deletion here.
- consumer: apps/client/test/preview/design_feedback_overlay_test.dart:4; dependency: apps/client/lib/preview/design_feedback_overlay.dart; decision: KEEP production preview/tooling; screenshot/clipboard tests do not convert it into test-only source.
- consumer: apps/client/test/preview/design_system_catalog_test.dart:2; dependency: apps/client/lib/preview/design_system_catalog.dart; decision: KEEP reusable production catalog and demonstration entrypoint/assets.
- consumer: apps/client/test/design_system/design_system_usage_test.dart:7–23; dependency: lib/features/**/*.dart dynamically read production files; decision: These are inspection inputs, not deletion candidates. Do not replace this retiring lexical rule with permanent architecture checker.
- consumer: all widget tests in this partition; dependency: production app shared components/theme/localization; Flutter/intl/Lucide packages; decision: Production assets/libraries remain KEEP. flutter_test/dev-dependency removal is aggregate repository ownership, not authorized by this partition.
- Supporting full read `apps/client/lib/features/day/application/fake_day_gateway.dart:1–122`; SHA-256 `8de605318547b1931dcd0452e051e18faeb1c655544afc12bf596a551e644504`; **production preview helper: KEEP**. In-memory copy of initial items; setTaskCompleted replaces matching task with incremented revision and current completion time; loadDay creates sorted immutable projection. Shell tests use these actual helper semantics but do not validate persistent Day or provider behavior.

Notable evidence limits: preview screenshot helper writes only four signature bytes; clipboard is mocked. Source-regex checks are lexical implementation-shape assertions, not visual proof. Disabled-switch assertion observes only an unrelated tracked value; checkbox shape assertion only rejects CircleBorder. The 1920-wide App-shell branch is empty and deliberately skips scrolling. Real Stopwatch delay exists in loading test but was not executed.

## Source-by-source behavior record

## `apps/client/test/app/app_shell_test.dart`

Full read: lines 1–222; 4 direct sites / 6 expanded registrations / 6 scenarios.
File SHA-256: `2100a6d9efa7ffa0e39ce678e715e84da420f1ebdb78b8a48259d03de51c9f82`

Current owner: FloeApp/PersonalDay shell, Day task-completion callbacks, FloeToastHost and navigation
Target owner: App shell composes features; Day owns task state; shared presentation owns toast/navigation/layout.
Harness/support: All tests use production FakeDayGateway (KEEP) with in-memory mutable items, no database/provider writes. Responsive table has 3 registrations; view dimensions/pixel ratio restored. Event generator produces 6 all-day items and 1 timed item for nonempty branches only.

Dependencies (actual source imports; production imports remain KEEP):
- L1: `package:floe_client/app/floe_app.dart` → `apps/client/lib/app/floe_app.dart`
- L2: `package:floe_client/app/floe_loading.dart` → `apps/client/lib/app/floe_loading.dart`
- L3: `package:floe_client/app/floe_selection.dart` → `apps/client/lib/app/floe_selection.dart`
- L4: `package:floe_client/app/floe_squircle.dart` → `apps/client/lib/app/floe_squircle.dart`
- L5: `package:floe_client/features/day/application/fake_day_gateway.dart` → `apps/client/lib/features/day/application/fake_day_gateway.dart`
- L6: `package:floe_client/features/day/domain/day_models.dart` → `apps/client/lib/features/day/domain/day_models.dart`
- L7: `package:flutter/material.dart` → `SDK/package dependency`
- L8: `package:flutter_test/flutter_test.dart` → `SDK/package dependency`
- L9: `package:intl/intl.dart` → `SDK/package dependency`

### app_shell_test#1: 'task toast preserves undo and survives navigation'
Source: `apps/client/test/app/app_shell_test.dart:12–63`; `testWidgets`; 1 expanded registration(s); SHA-256 `5c032ae7c7d8470c41df5f885944e2fc3b6bb46837874eb410513613fc446fca`.

- **app_shell_test#1.1 — Undo survives navigation [D]**
  - Preconditions: 1440×900 app with one unfinished task toast-task at revision 0 and fixed Day query.
  - Input/action: Tap first checkbox; navigate Settings; press Undo; advance loading minimum.
  - Expected outcome: Task readback becomes complete, Undo appears and no SnackBar; Undo remains after navigation; pressing it makes same task incomplete and removes Undo.
  - Failure/race and evidence limits: End-to-end only through in-memory FakeDayGateway, not durable backend. No conflicting revision, failed Undo or repeated action race.
  - Target classification/disposition: Preserve explicit Undo intent targeting the same task across navigation; final Day owner must enforce revision/recovery, not rely on fake behavior. Toast style is P.

### app_shell_test#2: 'calendar has no capture input and refresh uses toast'
Source: `apps/client/test/app/app_shell_test.dart:65–82`; `testWidgets`; 1 expanded registration(s); SHA-256 `d743eb4108fd408bd408dc4054b9e892c40d4e002a2a1a0f43057b8264bca4c2`.

- **app_shell_test#2.1 — calendar refresh feedback [P]**
  - Preconditions: 1440×900 app with empty FakeDayGateway.
  - Input/action: Inspect calendar then click Refresh calendar; wait 5 seconds after notification.
  - Expected outcome: No capture-field or TextField; Calendars refreshed appears; after delay no exception.
  - Failure/race and evidence limits: Does not explicitly assert toast disappears after 5 seconds or count refresh calls; no external Calendar sync.
  - Target classification/disposition: Reassess capture absence and toast feedback placement; truthful refresh success must come from canonical Day/Connections outcome.

### app_shell_test#3: 'calendar navigation distinguishes selected dates from today'
Source: `apps/client/test/app/app_shell_test.dart:84–119`; `testWidgets`; 1 expanded registration(s); SHA-256 `d7a865ad81fbfe01de3b3f4d500e700de2192408d6a85236e7be1bb8f41f8d4c`.

- **app_shell_test#3.1 — today versus selected date [P]**
  - Preconditions: System local now/date and current timezone offset; in-memory empty Day.
  - Input/action: Initially inspect; Next day; Go to today.
  - Expected outcome: Initially Calendar tooltip and Today; next-day English MMMEd title, no Today and Go to today action; return restores current title/Today and removes Go to today; no exception.
  - Failure/race and evidence limits: Depends on current local clock and today+24 hours; no controlled DST/midnight transition or backend date-query assertion.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

### app_shell_test#4: 'workspace fills the window without a frame at $width'
Source: `apps/client/test/app/app_shell_test.dart:122–220`; `testWidgets`; 3 expanded registration(s); SHA-256 `123d41c5918ba3b9c119aaaf5b72ebb41488b8dceaff52b1a66c6b7375f3f56a`.

- **app_shell_test#4.1 — width 390 [P]**
  - Branch evidence: L121–221 in this file.
  - Preconditions: View 390×700 at pixel ratio 1; fixed 4 Sep 2026 UTC query; six all-day events plus 9–10 AM timed event.
  - Input/action: Render and measure full shell; drag calendar-scroll 80 px upward and remeasure; navigate Settings.
  - Expected outcome: Scaffold SafeArea child Stack fills exact viewport; no frame-size FloeSquircle; Settings/Create event affordances present; no Plan a Calendar event or TextField; timeline top<140 and bottom≤604; no outer SingleChildScrollView ancestor. Inner scroll offset increases while timeline bounds unchanged. Settings bottom-nav top>600. After click Settings and Remote server visible, no exception.
  - Failure/race and evidence limits: Responsive geometry and navigation only; empty 1920 branch deliberately does not establish scroll behavior.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

- **app_shell_test#4.2 — width 1440 [P]**
  - Branch evidence: L121–221 in this file.
  - Preconditions: View 1440×768 at pixel ratio 1; fixed 4 Sep 2026 UTC query; six all-day events plus 9–10 AM timed event.
  - Input/action: Render and measure full shell; drag calendar-scroll 80 px upward and remeasure; navigate Settings.
  - Expected outcome: Scaffold SafeArea child Stack fills exact viewport; no frame-size FloeSquircle; Settings/Create event affordances present; no Plan a Calendar event or TextField; timeline top<140 and bottom≤744; no outer SingleChildScrollView ancestor. Inner scroll offset increases while timeline bounds unchanged. Settings rail right<100. After click Settings and Remote server visible, no exception.
  - Failure/race and evidence limits: Responsive geometry and navigation only; empty 1920 branch deliberately does not establish scroll behavior.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

- **app_shell_test#4.3 — width 1920 [P]**
  - Branch evidence: L121–221 in this file.
  - Preconditions: View 1920×768 at pixel ratio 1; fixed 4 Sep 2026 UTC query; no Day items.
  - Input/action: Render and measure full shell; skip scroll branch for empty wide fixture; navigate Settings.
  - Expected outcome: Scaffold SafeArea child Stack fills exact viewport; no frame-size FloeSquircle; Settings/Create event affordances present; no Plan a Calendar event or TextField; timeline top<140 and bottom≤744; no outer SingleChildScrollView ancestor. No scroll assertion in this empty branch. Settings rail right<100. After click Settings and Remote server visible, no exception.
  - Failure/race and evidence limits: Responsive geometry and navigation only; empty 1920 branch deliberately does not establish scroll behavior.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.
## `apps/client/test/design_system/design_system_usage_test.dart`

Full read: lines 1–28; 1 direct sites / 1 expanded registrations / 2 scenarios.
File SHA-256: `8f9aebf9c0bd8fb37c4fdd8a237d61c229a8a2011d93a114e2a280cac4f67e1c`

Current owner: Filesystem source-regex test over lib/features
Target owner: Design-system intent belongs to shared components and deliberate review, not permanent source-text shape enforcement.
Harness/support: Directory.listSync recursively reads production Dart feature files; regex/TextStyle string checks do not parse syntax, resolve symbols or exercise UI. No helper file or production library belongs to deletion scope.

Dependencies (actual source imports; production imports remain KEEP):
- L1: `dart:io` → `SDK/package dependency`
- L3: `package:flutter_test/flutter_test.dart` → `SDK/package dependency`

### design_system_usage_test#1: 'feature UI uses Floe design-system components and typography'
Source: `apps/client/test/design_system/design_system_usage_test.dart:6–27`; `test`; 1 expanded registration(s); SHA-256 `aeca1611c1bb3fdc40f23b7d29af7f5a8b88baa99361e4e94b65511d51a9c15d`.

- **design_system_usage_test#1.1 — direct text-style source rule [O]**
  - Branch evidence: L16–20 in this file.
  - Preconditions: Every .dart file recursively under lib/features is enumerated at test runtime.
  - Input/action: Read each source and flag any literal TextStyle( substring.
  - Expected outcome: No flagged files expected; failures list path plus direct TextStyle.
  - Failure/race and evidence limits: Comments/strings and aliases can affect this lexical check; no rendering/behavior proof.
  - Target classification/disposition: Retire lexical implementation-shape assertion; do not recreate a permanent deleted-symbol checker. Preserve design intent via final shared components and chosen validation strategy.

- **design_system_usage_test#1.2 — stock widget source rule [O]**
  - Branch evidence: L11–13 in this file.
  - Preconditions: Same production Dart source set.
  - Input/action: Match stock Scaffold/AppBar/dialog/button/checkbox/radio/switch/input/slider/divider/tile/card/chip/badge/material/InkWell/tooltip constructor-like tokens using regex.
  - Expected outcome: No matches expected; failures report stock visual component.
  - Failure/race and evidence limits: The expression is syntactic text only and not compiler-resolved; legal uses can be flagged and indirect ones missed.
  - Target classification/disposition: Retire source-regex enforcement; no target product policy should ban arbitrary symbols solely to preserve this test.
## `apps/client/test/design_system/feedback_layout_test.dart`

Full read: lines 1–87; 2 direct sites / 8 expanded registrations / 8 scenarios.
File SHA-256: `d6db5f648eeb5930668fdce02fc797edfc69ecedff100d3b72fba43a56e9140e`

Current owner: showFloeDialog; FloeInfoNote
Target owner: Shared presentation/layout and accessibility components.
Harness/support: Nested test registration loops: width×dialog kind and scale×language. Hosts provide theme/localizations; view-size/pixel-ratio restored. No external I/O.

Dependencies (actual source imports; production imports remain KEEP):
- L1: `package:floe_client/app/floe_feedback.dart` → `apps/client/lib/app/floe_feedback.dart`
- L2: `package:floe_client/app/floe_theme.dart` → `apps/client/lib/app/floe_theme.dart`
- L3: `package:floe_client/l10n/app_localizations.dart` → `apps/client/lib/l10n/app_localizations.dart`
- L4: `package:flutter/material.dart` → `SDK/package dependency`
- L5: `package:flutter_test/flutter_test.dart` → `SDK/package dependency`

### feedback_layout_test#1: 'dialog width is bounded at $width, simple: $simple'
Source: `apps/client/test/design_system/feedback_layout_test.dart:10–56`; `testWidgets`; 4 expanded registration(s); SHA-256 `ccebca0b4d4c6443856890e3613564cacbdfdf9ab7a0229b98cff4430d01f84a`.

- **feedback_layout_test#1.1 — 360 AlertDialog [P]**
  - Branch evidence: L8–58 in this file.
  - Preconditions: View 360×900 at pixel ratio 1; AlertDialog with repeated Calendar disclosure text.
  - Input/action: Tap Open and settle dialog layout.
  - Expected outcome: First dialog Material surface width ≤540, left≥0, right≤360; no widget exception.
  - Failure/race and evidence limits: Oversized textual content must remain horizontally inside viewport; vertical overflow is not explicitly bounded.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

- **feedback_layout_test#1.2 — 360 SimpleDialog [P]**
  - Branch evidence: L8–58 in this file.
  - Preconditions: View 360×900 at pixel ratio 1; SimpleDialog with repeated Calendar name.
  - Input/action: Tap Open and settle dialog layout.
  - Expected outcome: First dialog Material surface width ≤540, left≥0, right≤360; no widget exception.
  - Failure/race and evidence limits: Oversized textual content must remain horizontally inside viewport; vertical overflow is not explicitly bounded.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

- **feedback_layout_test#1.3 — 1440 AlertDialog [P]**
  - Branch evidence: L8–58 in this file.
  - Preconditions: View 1440×900 at pixel ratio 1; AlertDialog with repeated Calendar disclosure text.
  - Input/action: Tap Open and settle dialog layout.
  - Expected outcome: First dialog Material surface width ≤540, left≥0, right≤1440; no widget exception.
  - Failure/race and evidence limits: Oversized textual content must remain horizontally inside viewport; vertical overflow is not explicitly bounded.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

- **feedback_layout_test#1.4 — 1440 SimpleDialog [P]**
  - Branch evidence: L8–58 in this file.
  - Preconditions: View 1440×900 at pixel ratio 1; SimpleDialog with repeated Calendar name.
  - Input/action: Tap Open and settle dialog layout.
  - Expected outcome: First dialog Material surface width ≤540, left≥0, right≤1440; no widget exception.
  - Failure/race and evidence limits: Oversized textual content must remain horizontally inside viewport; vertical overflow is not explicitly bounded.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

### feedback_layout_test#2: 'icon centers on first line at $scale: $text'
Source: `apps/client/test/design_system/feedback_layout_test.dart:65–84`; `testWidgets`; 4 expanded registration(s); SHA-256 `b950fff48ef2ce1187ef7af356688c6eb28be31d43e72966cf9d50294fc8e36f`.

- **feedback_layout_test#2.1 — English scale 1 [P]**
  - Branch evidence: L60–85 in this file.
  - Preconditions: FloeInfoNote text has 3 lines (First line / Second line / Third line), text scale 1.
  - Input/action: Render and measure text bounds and icon center.
  - Expected outcome: Icon vertical center equals top plus half one line-height within 0.1; no exception.
  - Failure/race and evidence limits: Uses total text height divided by line count, not a font-baseline or native accessibility reading test.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

- **feedback_layout_test#2.2 — Korean scale 1 [P]**
  - Branch evidence: L60–85 in this file.
  - Preconditions: FloeInfoNote text has 2 lines (첫 번째 줄 / 두 번째 줄), text scale 1.
  - Input/action: Render and measure text bounds and icon center.
  - Expected outcome: Icon vertical center equals top plus half one line-height within 0.1; no exception.
  - Failure/race and evidence limits: Uses total text height divided by line count, not a font-baseline or native accessibility reading test.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

- **feedback_layout_test#2.3 — English scale 2 [P]**
  - Branch evidence: L60–85 in this file.
  - Preconditions: FloeInfoNote text has 3 lines (First line / Second line / Third line), text scale 2.
  - Input/action: Render and measure text bounds and icon center.
  - Expected outcome: Icon vertical center equals top plus half one line-height within 0.1; no exception.
  - Failure/race and evidence limits: Uses total text height divided by line count, not a font-baseline or native accessibility reading test.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

- **feedback_layout_test#2.4 — Korean scale 2 [P]**
  - Branch evidence: L60–85 in this file.
  - Preconditions: FloeInfoNote text has 2 lines (첫 번째 줄 / 두 번째 줄), text scale 2.
  - Input/action: Render and measure text bounds and icon center.
  - Expected outcome: Icon vertical center equals top plus half one line-height within 0.1; no exception.
  - Failure/race and evidence limits: Uses total text height divided by line count, not a font-baseline or native accessibility reading test.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.
## `apps/client/test/design_system/floe_action_card_test.dart`

Full read: lines 1–36; 1 direct sites / 1 expanded registrations / 1 scenarios.
File SHA-256: `009e78149fa0b9776c988298c195f94fb47a2f3d7ffdc8e1908c871815a6dec8`

Current owner: FloeActionCard and Flutter mouse tracker
Target owner: Shared clickable-card presentation and pointer affordance.
Harness/support: Synthetic mouse gesture registered/removed in test teardown; onPressed is empty, no product action executes.

Dependencies (actual source imports; production imports remain KEEP):
- L1: `package:floe_client/app/floe_action_card.dart` → `apps/client/lib/app/floe_action_card.dart`
- L2: `package:floe_client/app/floe_theme.dart` → `apps/client/lib/app/floe_theme.dart`
- L3: `package:flutter/gestures.dart` → `SDK/package dependency`
- L4: `package:flutter/material.dart` → `SDK/package dependency`
- L5: `package:flutter/rendering.dart` → `SDK/package dependency`
- L6: `package:flutter_test/flutter_test.dart` → `SDK/package dependency`

### floe_action_card_test#1: 'enabled action card uses a pointer cursor'
Source: `apps/client/test/design_system/floe_action_card_test.dart:9–35`; `testWidgets`; 1 expanded registration(s); SHA-256 `a19e02138ece45942debf3c585d7561b1dad64d6fc0e4a385e892b7684bcde2e`.

- **floe_action_card_test#1.1 — enabled pointer affordance [P]**
  - Preconditions: Enabled Action card has title/description and non-null callback under FloeTheme.
  - Input/action: Move mouse over center and pump.
  - Expected outcome: Mouse tracker device 1 cursor is click.
  - Failure/race and evidence limits: No disabled branch or actual onPressed action is tested.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.
## `apps/client/test/design_system/floe_calendar_popover_test.dart`

Full read: lines 1–361; 7 direct sites / 7 expanded registrations / 19 scenarios.
File SHA-256: `7fb55a9b6def440e0abd2cac936aea095ca2be3335887540f1b2539bdac700af`

Current owner: FloeContextMenu, FloeDatePicker, FloeTimePicker, showFloeDatePicker and CalendarDateTimeField
Target owner: Shared date/time/menu presentation; local editing callbacks are not external Calendar writes.
Harness/support: host (16–27) injects theme/localizations, MediaQuery800×600 and optional reduced motion. Inline expectHighlight/background/openAt helpers read widget geometry/colors; synthetic input and no provider calls.

Dependencies (actual source imports; production imports remain KEEP):
- L1: `package:flutter/gestures.dart` → `SDK/package dependency`
- L2: `package:flutter/cupertino.dart` → `SDK/package dependency`
- L3: `package:flutter/material.dart` → `SDK/package dependency`
- L4: `package:flutter/services.dart` → `SDK/package dependency`
- L5: `package:flutter_test/flutter_test.dart` → `SDK/package dependency`
- L6: `package:floe_client/app/design_tokens.dart` → `apps/client/lib/app/design_tokens.dart`
- L7: `package:floe_client/app/floe_theme.dart` → `apps/client/lib/app/floe_theme.dart`
- L8: `package:floe_client/app/floe_date_picker.dart` → `apps/client/lib/app/floe_date_picker.dart`
- L9: `package:floe_client/app/floe_context_menu.dart` → `apps/client/lib/app/floe_context_menu.dart`
- L10: `package:floe_client/app/floe_motion.dart` → `apps/client/lib/app/floe_motion.dart`
- L11: `package:floe_client/app/floe_popover.dart` → `apps/client/lib/app/floe_popover.dart`
- L12: `package:floe_client/app/floe_time_picker.dart` → `apps/client/lib/app/floe_time_picker.dart`
- L13: `package:floe_client/features/day/presentation/calendar_date_time_field.dart` → `apps/client/lib/features/day/presentation/calendar_date_time_field.dart`
- L14: `package:floe_client/l10n/app_localizations.dart` → `apps/client/lib/l10n/app_localizations.dart`

### floe_calendar_popover_test#1: 'menu hover transfers immediately without a trailing highlight'
Source: `apps/client/test/design_system/floe_calendar_popover_test.dart:30–106`; `testWidgets`; 1 expanded registration(s); SHA-256 `561108c44ef107c8ce95fa63be3d044c77c47bd2f713e83cf528bc55b02898d4`.

- **floe_calendar_popover_test#1.1 — initial Open [P]**
  - Branch evidence: L86–88 in this file.
  - Preconditions: Context menu Open/Edit/Delete/Disabled; Delete destructive with separator; Disabled not enabled; no prior hover.
  - Input/action: Move mouse to Open; pump and inspect row paint.
  - Expected outcome: Each of four rows updates immediately: only Open has primary50 background; all others transparent.
  - Failure/race and evidence limits: No trailing previous highlight; disabled/separator/outside never acquire one. No action callback runs.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

- **floe_calendar_popover_test#1.2 — Open to Edit [P]**
  - Branch evidence: L90–96 in this file.
  - Preconditions: Context menu Open/Edit/Delete/Disabled; Delete destructive with separator; Disabled not enabled; Open hovered.
  - Input/action: Move mouse to Edit; assert after pump and again after 16 ms.
  - Expected outcome: Each of four rows updates immediately: only Edit has primary50 background; all others transparent.
  - Failure/race and evidence limits: No trailing previous highlight; disabled/separator/outside never acquire one. No action callback runs.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

- **floe_calendar_popover_test#1.3 — Edit to Delete [P]**
  - Branch evidence: L90–96 in this file.
  - Preconditions: Context menu Open/Edit/Delete/Disabled; Delete destructive with separator; Disabled not enabled; Edit hovered.
  - Input/action: Move mouse to Delete; assert after pump and again after 16 ms.
  - Expected outcome: Each of four rows updates immediately: only Delete has primary50 background; all others transparent.
  - Failure/race and evidence limits: No trailing previous highlight; disabled/separator/outside never acquire one. No action callback runs.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

- **floe_calendar_popover_test#1.4 — Delete to Open [P]**
  - Branch evidence: L90–96 in this file.
  - Preconditions: Context menu Open/Edit/Delete/Disabled; Delete destructive with separator; Disabled not enabled; Delete hovered.
  - Input/action: Move mouse to Open; assert after pump and again after 16 ms.
  - Expected outcome: Each of four rows updates immediately: only Open has primary50 background; all others transparent.
  - Failure/race and evidence limits: No trailing previous highlight; disabled/separator/outside never acquire one. No action callback runs.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

- **floe_calendar_popover_test#1.5 — Open to Disabled [P]**
  - Branch evidence: L90–96 in this file.
  - Preconditions: Context menu Open/Edit/Delete/Disabled; Delete destructive with separator; Disabled not enabled; Open hovered.
  - Input/action: Move mouse to Disabled; assert after pump and again after 16 ms.
  - Expected outcome: Each of four rows updates immediately: all four backgrounds transparent.
  - Failure/race and evidence limits: No trailing previous highlight; disabled/separator/outside never acquire one. No action callback runs.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

- **floe_calendar_popover_test#1.6 — Disabled to Edit [P]**
  - Branch evidence: L90–96 in this file.
  - Preconditions: Context menu Open/Edit/Delete/Disabled; Delete destructive with separator; Disabled not enabled; disabled row hovered.
  - Input/action: Move mouse to Edit; assert after pump and again after 16 ms.
  - Expected outcome: Each of four rows updates immediately: only Edit has primary50 background; all others transparent.
  - Failure/race and evidence limits: No trailing previous highlight; disabled/separator/outside never acquire one. No action callback runs.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

- **floe_calendar_popover_test#1.7 — separator [P]**
  - Branch evidence: L97–99 in this file.
  - Preconditions: Context menu Open/Edit/Delete/Disabled; Delete destructive with separator; Disabled not enabled; Edit hovered.
  - Input/action: Move mouse to separator; pump and inspect row paint.
  - Expected outcome: Each of four rows updates immediately: all four backgrounds transparent.
  - Failure/race and evidence limits: No trailing previous highlight; disabled/separator/outside never acquire one. No action callback runs.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

- **floe_calendar_popover_test#1.8 — separator to Delete [P]**
  - Branch evidence: L100–102 in this file.
  - Preconditions: Context menu Open/Edit/Delete/Disabled; Delete destructive with separator; Disabled not enabled; separator hovered.
  - Input/action: Move mouse to Delete; pump and inspect row paint.
  - Expected outcome: Each of four rows updates immediately: only Delete has primary50 background; all others transparent.
  - Failure/race and evidence limits: No trailing previous highlight; disabled/separator/outside never acquire one. No action callback runs.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

- **floe_calendar_popover_test#1.9 — exit menu [P]**
  - Branch evidence: L103–105 in this file.
  - Preconditions: Context menu Open/Edit/Delete/Disabled; Delete destructive with separator; Disabled not enabled; Delete hovered.
  - Input/action: Move mouse to outside at origin; pump and inspect row paint.
  - Expected outcome: Each of four rows updates immediately: all four backgrounds transparent.
  - Failure/race and evidence limits: No trailing previous highlight; disabled/separator/outside never acquire one. No action callback runs.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

### floe_calendar_popover_test#2: 'custom date popover handles leap months, keyboard and dismissal'
Source: `apps/client/test/design_system/floe_calendar_popover_test.dart:108–152`; `testWidgets`; 1 expanded registration(s); SHA-256 `582083741d8becff3bf03e3a36b034fd42a55702f69c66991af70aa92d7f1a50`.

- **floe_calendar_popover_test#2.1 — leap-month pointer selection [P]**
  - Branch evidence: L128–137 in this file.
  - Preconditions: CalendarDateTimeField starts 31 Jan 2028 14:30.
  - Input/action: Open date, Next month, choose semantics-labeled Tuesday 29 February.
  - Expected outcome: Uses FloeDatePicker, no stock DatePickerDialog; February 2028 header; value becomes 29 Feb 14:30, preserving time.
  - Failure/race and evidence limits: No native calendar mutation; leap date selection only.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

- **floe_calendar_popover_test#2.2 — keyboard day crossing [P]**
  - Branch evidence: L138–143 in this file.
  - Preconditions: Field now 29 Feb 2028 14:30 after prior pointer selection.
  - Input/action: Reopen; Right arrow; Enter.
  - Expected outcome: Value becomes 1 March 2028 14:30.
  - Failure/race and evidence limits: Keyboard commit crosses leap-month boundary and preserves time.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

- **floe_calendar_popover_test#2.3 — Escape leaves prior value [D]**
  - Branch evidence: L144–150 in this file.
  - Preconditions: Field now 1 March 2028 14:30.
  - Input/action: Reopen then Escape.
  - Expected outcome: Picker disappears, value unchanged and no widget exception.
  - Failure/race and evidence limits: Dismissal must not commit an unconfirmed edit.
  - Target classification/disposition: Re-prove this durable interaction/property through the final owner; do not preserve old widget or wire shape merely for the test.

### floe_calendar_popover_test#3: 'date-time controls keep compact hover targets'
Source: `apps/client/test/design_system/floe_calendar_popover_test.dart:154–179`; `testWidgets`; 1 expanded registration(s); SHA-256 `f92b860937c2cf62da9aa4e86004db17263de2f5a91d40623052344c71ffdc6e`.

- **floe_calendar_popover_test#3.1 — compact date hit area [P]**
  - Preconditions: Ends field width 500 with 8 Sep 2026 15:45.
  - Input/action: Measure date TextButton and containing decorator.
  - Expected outcome: Date button width less than half full field width.
  - Failure/race and evidence limits: Only geometry, no hover activation performed despite title.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

### floe_calendar_popover_test#4: 'time picker overlay keeps selected values visible'
Source: `apps/client/test/design_system/floe_calendar_popover_test.dart:181–201`; `testWidgets`; 1 expanded registration(s); SHA-256 `19f76d68a0e7006ff7aacafea282a35369c70deab1423eb312f1d82178f7e79b`.

- **floe_calendar_popover_test#4.1 — translucent wheel overlays [P]**
  - Preconditions: FloeTimePicker initially 15:28.
  - Input/action: Inspect every CupertinoPickerDefaultSelectionOverlay.
  - Expected outcome: At least one overlay exists; each background alpha<1.
  - Failure/race and evidence limits: Loop covers runtime-discovered overlays, not a fixed known registration expansion; no selected wheel value or scrolling is asserted.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

### floe_calendar_popover_test#5: 'calendar hover moves without leaving the previous date styled'
Source: `apps/client/test/design_system/floe_calendar_popover_test.dart:203–234`; `testWidgets`; 1 expanded registration(s); SHA-256 `c81a18b307c5fe6cfad931bc054a3e2fbdeb962b0fb190a97d1f30a2db91edb4`.

- **floe_calendar_popover_test#5.1 — calendar hover transfer [P]**
  - Preconditions: Date picker initially 14 Feb 2028.
  - Input/action: Hover15 February, then16 February with a pump after each.
  - Expected outcome: First hovered date uses selectionHover; after movement15th is transparent and16th selectionHover.
  - Failure/race and evidence limits: No trailing hover; selected-date state and date commit are not changed.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

### floe_calendar_popover_test#6: 'popover centers on its trigger and uses directional origins'
Source: `apps/client/test/design_system/floe_calendar_popover_test.dart:236–289`; `testWidgets`; 1 expanded registration(s); SHA-256 `c3da0287a628527f30b02d31a7dae896d5bf494919f49098ac6ef7147565f44c`.

- **floe_calendar_popover_test#6.1 — below top-center trigger [P]**
  - Branch evidence: L262–273 in this file.
  - Preconditions: Reduced-motion host; Calendar trigger aligned topCenter.
  - Input/action: Open anchored date picker.
  - Expected outcome: Picker center x within1 of trigger center, top below trigger bottom; fade-scale alignment y=-1.
  - Failure/race and evidence limits: Geometry only; adaptive opening origin avoids growing from wrong edge.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

- **floe_calendar_popover_test#6.2 — above bottom-center trigger [P]**
  - Branch evidence: L275–288 in this file.
  - Preconditions: First picker closed with outside tap; rebuild bottomCenter trigger under reduced motion.
  - Input/action: Open picker again.
  - Expected outcome: Horizontal center within1; picker bottom above trigger top; transition alignment y=1.
  - Failure/race and evidence limits: Viewport-dependent direction reflected in animation origin; no overflow bound asserted here.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

### floe_calendar_popover_test#7: 'menu stays in viewport, skips disabled entries and restores focus'
Source: `apps/client/test/design_system/floe_calendar_popover_test.dart:291–360`; `testWidgets`; 1 expanded registration(s); SHA-256 `d1b1bf9eb7be870053c88b8d4e41980bc770e7faaa7ee7f19bac912247d7d7c8`.

- **floe_calendar_popover_test#7.1 — viewport and enabled keyboard navigation [D]**
  - Branch evidence: L341–352 in this file.
  - Preconditions: 320×600 view; bottom-right anchor at310,590; Open enabled, Edit disabled, Delete enabled; trigger FocusNode focused.
  - Input/action: Open menu; Down twice then Enter.
  - Expected outcome: Menu right≤308 and bottom≤588; selected result delete, skipping disabled Edit; trigger focus restored.
  - Failure/race and evidence limits: Disabled entry must not become an activation target; exact margins are presentation hypotheses.
  - Target classification/disposition: Re-prove this durable interaction/property through the final owner; do not preserve old widget or wire shape merely for the test.

- **floe_calendar_popover_test#7.2 — outside cancellation [D]**
  - Branch evidence: L353–358 in this file.
  - Preconditions: Same menu after prior keyboard selection and focus restoration.
  - Input/action: Reopen and tap outside at10,10.
  - Expected outcome: Result null; no widget exception.
  - Failure/race and evidence limits: Cancellation returns no chosen operation and cannot accidentally preserve previous Delete result.
  - Target classification/disposition: Re-prove this durable interaction/property through the final owner; do not preserve old widget or wire shape merely for the test.
## `apps/client/test/design_system/floe_design_system_test.dart`

Full read: lines 1–135; 3 direct sites / 3 expanded registrations / 6 scenarios.
File SHA-256: `3022a48347d4709ed32f8ef68a4d42d29a29b028236dad9f1baa3f9193d57af8`

Current owner: FloeRadius/ControlSize/Type/States, FloeBadge, FloeInput/Select/Button
Target owner: Shared design tokens and component accessibility/presentation.
Harness/support: Widget hosts are local; imports are all retained production design-system libraries. Constant arrays are expected values, not independent registrations.

Dependencies (actual source imports; production imports remain KEEP):
- L1: `package:floe_client/app/design_tokens.dart` → `apps/client/lib/app/design_tokens.dart`
- L2: `package:floe_client/app/floe_badge.dart` → `apps/client/lib/app/floe_badge.dart`
- L3: `package:floe_client/app/floe_button.dart` → `apps/client/lib/app/floe_button.dart`
- L4: `package:floe_client/app/floe_input.dart` → `apps/client/lib/app/floe_input.dart`
- L5: `package:floe_client/app/floe_selection.dart` → `apps/client/lib/app/floe_selection.dart`
- L6: `package:floe_client/app/floe_squircle.dart` → `apps/client/lib/app/floe_squircle.dart`
- L7: `package:floe_client/app/floe_states.dart` → `apps/client/lib/app/floe_states.dart`
- L8: `package:floe_client/app/floe_theme.dart` → `apps/client/lib/app/floe_theme.dart`
- L9: `package:flutter/material.dart` → `SDK/package dependency`
- L10: `package:flutter_test/flutter_test.dart` → `SDK/package dependency`

### floe_design_system_test#1: 'foundation and interaction tokens match the component contract'
Source: `apps/client/test/design_system/floe_design_system_test.dart:13–48`; `test`; 1 expanded registration(s); SHA-256 `7984d64b4aea7e8a6680adb46f03a65791b242aa8866f95568fe1fa0d708e300`.

- **floe_design_system_test#1.1 — radius/size/type tokens [P]**
  - Branch evidence: L14–32 in this file.
  - Preconditions: Baseline design token classes.
  - Input/action: Read radius xs/sm/md/lg/xl/frame, compact/standard/field sizes and type styles.
  - Expected outcome: Radii [8,12,16,20,28,32]; sizes [36,44,48]; control label font size13/weight500; button weight600.
  - Failure/race and evidence limits: Exact numeric theme defaults only; no layout or safety behavior tested.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

- **floe_design_system_test#1.2 — outlined hover [P]**
  - Branch evidence: L33–36 in this file.
  - Preconditions: Shared state resolver under baseline theme.
  - Input/action: Resolve outlinedBackground with hovered.
  - Expected outcome: neutralHover background.
  - Failure/race and evidence limits: Only the listed state combination is asserted; disabled/combined precedence not covered.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

- **floe_design_system_test#1.3 — quiet hover [P]**
  - Branch evidence: L37–40 in this file.
  - Preconditions: Shared state resolver under baseline theme.
  - Input/action: Resolve quietBackground with hovered.
  - Expected outcome: quietHover background.
  - Failure/race and evidence limits: Only the listed state combination is asserted; disabled/combined precedence not covered.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

- **floe_design_system_test#1.4 — focused border [P]**
  - Branch evidence: L41–44 in this file.
  - Preconditions: Shared state resolver under baseline theme.
  - Input/action: Resolve outlinedSide with focused.
  - Expected outcome: focus-colored border.
  - Failure/race and evidence limits: Only the listed state combination is asserted; disabled/combined precedence not covered.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

### floe_design_system_test#2: 'status is rendered by the semantic badge component'
Source: `apps/client/test/design_system/floe_design_system_test.dart:50–79`; `testWidgets`; 1 expanded registration(s); SHA-256 `aa7b5d233f3072e7ce5e6887280c33d460ec762b9dd2218dd27887a05c6f2b98`.

- **floe_design_system_test#2.1 — badge semantic label and style [P]**
  - Preconditions: Connected success badge.
  - Input/action: Render and inspect text, semantics and nested FloeSquircle.
  - Expected outcome: Connected once and semantics label Connected; text weight400; no border.
  - Failure/race and evidence limits: Semantic label accessibility is meaningful; exact weight/border are P. No screen-reader session runs.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

### floe_design_system_test#3: 'related controls enforce the shared size contract'
Source: `apps/client/test/design_system/floe_design_system_test.dart:81–134`; `testWidgets`; 1 expanded registration(s); SHA-256 `3936122de7bc0f04552f682abfe5eca39f3e5081bae21c2e0e095b44066852dd`.

- **floe_design_system_test#3.1 — shared control sizes [P]**
  - Preconditions: 320-wide input and select, standard filled Save button, compact More icon button.
  - Input/action: Render controls and measure.
  - Expected outcome: TextFormField height≥field token48; select decorator height equals it; filled button height44; compact icon height36.
  - Failure/race and evidence limits: Only default text scale is exercised; no large-text overflow evidence.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.
## `apps/client/test/design_system/floe_input_test.dart`

Full read: lines 1–92; 2 direct sites / 2 expanded registrations / 2 scenarios.
File SHA-256: `6982e6deaa6bb59d15d7d83667932f6414c506d878cb2e5a7c1b3897cd99c90e`

Current owner: FloeInput and FloeSelect
Target owner: Shared form-control presentation and focus/hover interaction.
Harness/support: Synthetic mouse and local Material hosts; no form submission or transport.

Dependencies (actual source imports; production imports remain KEEP):
- L1: `package:floe_client/app/design_tokens.dart` → `apps/client/lib/app/design_tokens.dart`
- L2: `package:floe_client/app/floe_input.dart` → `apps/client/lib/app/floe_input.dart`
- L3: `package:floe_client/app/floe_selection.dart` → `apps/client/lib/app/floe_selection.dart`
- L4: `package:floe_client/app/floe_theme.dart` → `apps/client/lib/app/floe_theme.dart`
- L5: `package:flutter/gestures.dart` → `SDK/package dependency`
- L6: `package:flutter/material.dart` → `SDK/package dependency`
- L7: `package:flutter_test/flutter_test.dart` → `SDK/package dependency`

### floe_input_test#1: 'input floats its label and uses the shared hover treatment'
Source: `apps/client/test/design_system/floe_input_test.dart:10–39`; `testWidgets`; 1 expanded registration(s); SHA-256 `80a94eb72581aa36ea5625fe82316e1f931a9af22ce014395c63bd4b7fbaf375`.

- **floe_input_test#1.1 — input label floats on focus [P]**
  - Preconditions: Empty Name input width 320 under theme.
  - Input/action: Record resting label position; hover TextFormField; click field.
  - Expected outcome: Decorator isHovering=true with neutral50 hover color; after focus label moves upward.
  - Failure/race and evidence limits: Only empty enabled input is exercised; no validation/error/disabled branches.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

### floe_input_test#2: 'select shares input size and floating-label interaction'
Source: `apps/client/test/design_system/floe_input_test.dart:41–91`; `testWidgets`; 1 expanded registration(s); SHA-256 `f516400979c1da7faf8da8c87203659836b6fab303b8e4bdacb90689323009cb`.

- **floe_input_test#2.1 — select label and popup focus [P]**
  - Preconditions: Name input beside unselected Calendar select with one Home option, width 320.
  - Input/action: Compare heights; open select; pump200ms.
  - Expected outcome: Select/input heights equal; placeholder absent initially; open decorator focused; trigger Material Clip.none; Calendar label rises; Choose an option appears.
  - Failure/race and evidence limits: Checks exact internal keys/decorator/clip shape; no option commit covered.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.
## `apps/client/test/design_system/floe_loading_test.dart`

Full read: lines 1–71; 3 direct sites / 3 expanded registrations / 3 scenarios.
File SHA-256: `bf79f8364e1fec510150eb538cafa3b04bbab8bcc01e14c1f27684d8a3cbbace`

Current owner: FloeLoading, FloeButton and FloeLoadingOverlay
Target owner: Shared loading UI; blocking repeated clicks is presentation boundary, actual idempotency remains domain-owned.
Harness/support: First test uses real Stopwatch and awaited minimum delay; widget tests use pump helpers switching ready/loading. No task/backend work executes.

Dependencies (actual source imports; production imports remain KEEP):
- L1: `package:floe_client/app/floe_button.dart` → `apps/client/lib/app/floe_button.dart`
- L2: `package:floe_client/app/floe_loading.dart` → `apps/client/lib/app/floe_loading.dart`
- L3: `package:floe_client/app/floe_theme.dart` → `apps/client/lib/app/floe_theme.dart`
- L4: `package:floe_client/l10n/app_localizations.dart` → `apps/client/lib/l10n/app_localizations.dart`
- L5: `package:flutter/material.dart` → `SDK/package dependency`
- L6: `package:flutter_test/flutter_test.dart` → `SDK/package dependency`

### floe_loading_test#1: 'loading operations remain visible for at least 500ms'
Source: `apps/client/test/design_system/floe_loading_test.dart:9–18`; `test`; 1 expanded registration(s); SHA-256 `95c5fb1236c354075499e28b0bdcf961aebd9872e1dbb29a82597706a0b75fbd`.

- **floe_loading_test#1.1 — minimum visible duration [P]**
  - Preconditions: Immediately completing async operation and real wall-clock Stopwatch.
  - Input/action: Await FloeLoading.run.
  - Expected outcome: Elapsed duration≥500ms.
  - Failure/race and evidence limits: No failure/long-operation/clock-jump branch; timing would consume actual time if executed.
  - Target classification/disposition: Reassess forced half-second minimum as product feedback timing, not structural or safety requirement.

### floe_loading_test#2: 'loading button keeps its layout and blocks presses'
Source: `apps/client/test/design_system/floe_loading_test.dart:20–48`; `testWidgets`; 1 expanded registration(s); SHA-256 `11bd232267a2a562c8ef89f61c56dc21b9b74d2a01228d43b5da8ba3546f9be8`.

- **floe_loading_test#2.1 — loading suppresses callback [D]**
  - Preconditions: Save note button callback increments counter, initially not loading.
  - Input/action: Measure ready FilledButton; rebuild loading=true; click.
  - Expected outcome: Same dimensions, exactly one FloeSpinner, pressed counter remains0.
  - Failure/race and evidence limits: Loading guard prevents duplicate UI callback in this state; no underlying request cancellation/idempotency asserted.
  - Target classification/disposition: Preserve suppression of disabled/loading user actions; reassess spinner/size implementation independently.

### floe_loading_test#3: 'section overlay preserves its child size'
Source: `apps/client/test/design_system/floe_loading_test.dart:50–70`; `testWidgets`; 1 expanded registration(s); SHA-256 `cb2c42df306724391965ac0c774cf5855ea934291d4a1a1f5f0278b09f486e48`.

- **floe_loading_test#3.1 — overlay layout [P]**
  - Preconditions: Loading overlay child fixed240×120.
  - Input/action: Measure not-loading then loading state.
  - Expected outcome: Overlay retains dimensions; one spinner.
  - Failure/race and evidence limits: Does not assert pointer interception despite overlay purpose; record only size/spinner.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.
## `apps/client/test/design_system/floe_selection_test.dart`

Full read: lines 1–370; 9 direct sites / 9 expanded registrations / 14 scenarios.
File SHA-256: `10e913785fa166556cd9c3b587e24ed5d5e38d6bdfa3811a92abbbed39db1cbd`

Current owner: FloeCheckbox/Tile, FloeRadioGroup/Tile, FloeSelect and FloeDropdown
Target owner: Shared accessible controls and local callback dispatch; domain permissions remain outside widget presentation.
Harness/support: StatefulBuilder variables/callback counters; focusOpacities/hoverColors/optionColors helpers inspect internal widget paint. _ignoreSelection at370 is local H; long-list data generator yields12 options, not12 tests.

Dependencies (actual source imports; production imports remain KEEP):
- L1: `package:flutter/gestures.dart` → `SDK/package dependency`
- L2: `package:flutter/material.dart` → `SDK/package dependency`
- L3: `package:flutter/services.dart` → `SDK/package dependency`
- L4: `package:flutter_test/flutter_test.dart` → `SDK/package dependency`
- L5: `package:floe_client/app/design_tokens.dart` → `apps/client/lib/app/design_tokens.dart`
- L6: `package:floe_client/app/floe_selection.dart` → `apps/client/lib/app/floe_selection.dart`
- L7: `package:floe_client/app/floe_theme.dart` → `apps/client/lib/app/floe_theme.dart`
- L8: `package:lucide_icons_flutter/lucide_icons.dart` → `SDK/package dependency`

### floe_selection_test#1: 'custom checkbox has semantics, keyboard toggle and disabled guard'
Source: `apps/client/test/design_system/floe_selection_test.dart:11–51`; `testWidgets`; 1 expanded registration(s); SHA-256 `a76d5d97e8a4d4511726d49dd676ee5d373a0c56f7fabadfefcad25449ae71b2`.

- **floe_selection_test#1.1 — keyboard checkbox plus semantic name [P]**
  - Branch evidence: L36–44 in this file.
  - Preconditions: Unchecked enabled custom checkbox labeled Complete task.
  - Input/action: Tab focus then Space.
  - Expected outcome: Checked=true, no stock Checkbox widget; semantics label Complete task.
  - Failure/race and evidence limits: Only label tested, not all accessibility flags.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

- **floe_selection_test#1.2 — disabled checkbox guard [D]**
  - Branch evidence: L45–49 in this file.
  - Preconditions: Same checkbox checked; rebuild with callback null.
  - Input/action: Tap disabled checkbox.
  - Expected outcome: Value remains checked.
  - Failure/race and evidence limits: Disabled interaction cannot trigger mutation callback.
  - Target classification/disposition: Re-prove this durable interaction/property through the final owner; do not preserve old widget or wire shape merely for the test.

### floe_selection_test#2: 'radio group uses arrows and maintains one selection'
Source: `apps/client/test/design_system/floe_selection_test.dart:53–83`; `testWidgets`; 1 expanded registration(s); SHA-256 `389996d4c54e7d08e009932ab18ba4aab5e02fd2abf48da6e2d181a8d70848e8`.

- **floe_selection_test#2.1 — single radio selection [P]**
  - Preconditions: Boolean radio group initially All=true with All/Selected options.
  - Input/action: Tab then Down; later tap All.
  - Expected outcome: Selection becomes false then true.
  - Failure/race and evidence limits: Single scalar value enforces one selection; no disabled radio or wraparound branch.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

### floe_selection_test#3: 'selection controls never scale on pointer down'
Source: `apps/client/test/design_system/floe_selection_test.dart:85–105`; `testWidgets`; 1 expanded registration(s); SHA-256 `dfdf99c3e66e93b27affbd11d65cd768a4eba8b06472515383fe132604a790bf`.

- **floe_selection_test#3.1 — no press scaling [P]**
  - Preconditions: Checked checkbox with reduced motion enabled.
  - Input/action: Measure visual; hold pointer down then release.
  - Expected outcome: Visual size identical before/during/after press.
  - Failure/race and evidence limits: Only size is inspected; title generalizes to all selection controls but this test instantiates one checkbox.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

### floe_selection_test#4: 'checkbox uses a rounded squircle rather than a circle'
Source: `apps/client/test/design_system/floe_selection_test.dart:107–122`; `testWidgets`; 1 expanded registration(s); SHA-256 `be8bfcf0d87e2c9465b27487a767188020cbff878ccc99add8f323c0994be033`.

- **floe_selection_test#4.1 — checkbox shape [P]**
  - Preconditions: Checked checkbox.
  - Input/action: Inspect AnimatedContainer ShapeDecoration.
  - Expected outcome: Shape is not CircleBorder.
  - Failure/race and evidence limits: Does not positively assert a particular rounded-squircle type despite label; do not overclaim exact shape.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

### floe_selection_test#5: 'pointer focus and hover move off the previous choice'
Source: `apps/client/test/design_system/floe_selection_test.dart:124–178`; `testWidgets`; 1 expanded registration(s); SHA-256 `6c7aa913da26ddf3ea96c42fcdd1ce2593146c94b8841a2d62306d03ea4484bb`.

- **floe_selection_test#5.1 — keyboard focus then pointer modality [P]**
  - Branch evidence: L162–167 in this file.
  - Preconditions: Two unchecked checkbox tiles First/Second.
  - Input/action: Tab then click Second.
  - Expected outcome: Focus-ring opacity [1,0] after keyboard; [0,0] after pointer.
  - Failure/race and evidence limits: Asserts visible focus treatment, not absence of actual focus.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

- **floe_selection_test#5.2 — hover paint transfers [P]**
  - Branch evidence: L169–177 in this file.
  - Preconditions: Same two tiles.
  - Input/action: Hover First then Second.
  - Expected outcome: Backgrounds change [primary50,transparent] then [transparent,primary50].
  - Failure/race and evidence limits: Previous item does not retain paint.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

### floe_selection_test#6: 'custom select commits only enabled options'
Source: `apps/client/test/design_system/floe_selection_test.dart:180–268`; `testWidgets`; 1 expanded registration(s); SHA-256 `a62de056c7f327b5d8c34f7cc1588d85b693cedb6bd1c2746a3852ddef841a7b`.

- **floe_selection_test#6.1 — open animation and popup geometry [P]**
  - Branch evidence: L209–229 in this file.
  - Preconditions: Select Target calendar starts Home; Work enabled, Team disabled.
  - Input/action: Tab then Down opens; inspect at 80 ms then settle.
  - Expected outcome: No stock DropdownButton or AnimatedScale; ScaleTransition topCenter scale strictly 0.97–1; three options and popup width>280.
  - Failure/race and evidence limits: Animation-stage values depend on current widget timing and are P.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

- **floe_selection_test#6.2 — option hover transfers [P]**
  - Branch evidence: L230–252 in this file.
  - Preconditions: Same open popup.
  - Input/action: Hover Home then Work.
  - Expected outcome: Exactly hovered option primary50; other two transparent.
  - Failure/race and evidence limits: Disabled Team remains unhighlighted in these two hover probes; no Team hover tested.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

- **floe_selection_test#6.3 — disabled option cannot commit [D]**
  - Branch evidence: L253–255 in this file.
  - Preconditions: Same popup, selection Home.
  - Input/action: Tap Team.
  - Expected outcome: Selection remains Home.
  - Failure/race and evidence limits: Disabled option does not invoke a new selected value; no backend call.
  - Target classification/disposition: Re-prove this durable interaction/property through the final owner; do not preserve old widget or wire shape merely for the test.

- **floe_selection_test#6.4 — enabled option commit and fade dismissal [P]**
  - Branch evidence: L256–267 in this file.
  - Preconditions: Same popup still available after Team attempt.
  - Input/action: Tap Work; inspect immediately and 40 ms later; settle.
  - Expected outcome: Popup initially still present; fade opacity strictly0–1 at40 ms; eventual selected value Work and popup absent.
  - Failure/race and evidence limits: Commit/dismiss transition should not lose selected value; exact fade is P.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

### floe_selection_test#7: 'disabled select ignores pointer hover'
Source: `apps/client/test/design_system/floe_selection_test.dart:270–304`; `testWidgets`; 1 expanded registration(s); SHA-256 `31451b71db2186664217ed2c9f9cf50b58b44ae9ee1664cbac87ea9195dea64a`.

- **floe_selection_test#7.1 — disabled select ignores hover [P]**
  - Preconditions: Select disabled with Home and no-op callback.
  - Input/action: Record trigger decoration; hover Home; settle.
  - Expected outcome: Decoration color and shape unchanged.
  - Failure/race and evidence limits: No click or callback count checked; proves hover paint only.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

### floe_selection_test#8: 'pointer hover does not scroll an open select'
Source: `apps/client/test/design_system/floe_selection_test.dart:306–343`; `testWidgets`; 1 expanded registration(s); SHA-256 `8c02fab703e3451a68e2bf349d1cb8354e46a18380a40bbeb36e96f9238c5e16`.

- **floe_selection_test#8.1 — hover does not scroll [P]**
  - Preconditions: Open 12-option select initially0; manually drag popup 100 px upward.
  - Input/action: Capture scroll position; hover visible Option 5.
  - Expected outcome: Position remains unchanged.
  - Failure/race and evidence limits: Pointer hover must not steal scroll position; keyboard-driven scroll is not covered.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

### floe_selection_test#9: 'custom dropdown invokes its selected action'
Source: `apps/client/test/design_system/floe_selection_test.dart:345–367`; `testWidgets`; 1 expanded registration(s); SHA-256 `f6d52f1cebd94d6f21a3f786fc91a4a8b6629d001d5128c1e9f3ec89ccebf6ae`.

- **floe_selection_test#9.1 — dropdown dispatch once [D]**
  - Preconditions: Custom Task options dropdown has one Complete item and call counter.
  - Input/action: Open ellipsis, select Complete.
  - Expected outcome: No stock PopupMenuButton; callback count1.
  - Failure/race and evidence limits: Only one tap; no rapid-repeat/race assertion.
  - Target classification/disposition: Preserve exactly one callback for one confirmed selection; custom widget/stock-component absence is P, not owner policy.
## `apps/client/test/design_system/floe_switch_test.dart`

Full read: lines 1–52; 1 direct sites / 1 expanded registrations / 2 scenarios.
File SHA-256: `44b5115f27a83047d301377c57db72bf95628e118ad0e370bd55db0263f98a1f`

Current owner: FloeSwitch
Target owner: Shared accessible control; domain permission commands remain separate.
Harness/support: StatefulBuilder holds boolean; second disabled switch has null callback. Synthetic keyboard and pointer only.

Dependencies (actual source imports; production imports remain KEEP):
- L1: `package:floe_client/app/floe_switch.dart` → `apps/client/lib/app/floe_switch.dart`
- L2: `package:floe_client/app/floe_theme.dart` → `apps/client/lib/app/floe_theme.dart`
- L3: `package:flutter/material.dart` → `SDK/package dependency`
- L4: `package:flutter/services.dart` → `SDK/package dependency`
- L5: `package:flutter_test/flutter_test.dart` → `SDK/package dependency`

### floe_switch_test#1: 'custom switch supports pointer keyboard semantics and disabled state'
Source: `apps/client/test/design_system/floe_switch_test.dart:8–51`; `testWidgets`; 1 expanded registration(s); SHA-256 `c4c8ec579faf6ea99273d236d8260a1c9ff2096566646300e730535eaeca125a`.

- **floe_switch_test#1.1 — enabled pointer and Space [P]**
  - Branch evidence: L38–45 in this file.
  - Preconditions: Enabled Calendar access switch false and separate disabled switch false.
  - Input/action: Inspect semantics, tap enabled, then press Space.
  - Expected outcome: Semantics node exists; value becomes true then false.
  - Failure/race and evidence limits: Does not inspect semantic label/toggled flags; no permission owner called.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

- **floe_switch_test#1.2 — disabled pointer [D]**
  - Branch evidence: L47–49 in this file.
  - Preconditions: Same screen after value has returned false; disabled switch has null callback.
  - Input/action: Tap disabled switch.
  - Expected outcome: Tracked enabled value stays false.
  - Failure/race and evidence limits: Weak assertion: disabled switch has no mutable state/callback counter, so only absence of side effects on tracked value is observed.
  - Target classification/disposition: Preserve disabled-control nonactivation; improve final owner/interaction proof rather than copying weak fixture assertion.
## `apps/client/test/design_system/floe_theme_test.dart`

Full read: lines 1–57; 2 direct sites / 2 expanded registrations / 25 scenarios.
File SHA-256: `fbdb0e9f0f9cc641facb2e89d2bca6cec0051dcc9e8186cd23d35a79f934976d`

Current owner: FloeTheme light component state cursors and Tooltip timing
Target owner: Shared theme/pointer affordances and feedback timing.
Harness/support: Eight cursor resolvers ×three states are an inner table, not24 registered tests. Tooltip test uses synthetic mouse and widget-clock pumping.

Dependencies (actual source imports; production imports remain KEEP):
- L1: `package:floe_client/app/floe_theme.dart` → `apps/client/lib/app/floe_theme.dart`
- L2: `package:flutter/gestures.dart` → `SDK/package dependency`
- L3: `package:flutter/material.dart` → `SDK/package dependency`
- L4: `package:flutter_test/flutter_test.dart` → `SDK/package dependency`

### floe_theme_test#1: 'interactive themes use a pointer only while enabled'
Source: `apps/client/test/design_system/floe_theme_test.dart:7–25`; `test`; 1 expanded registration(s); SHA-256 `34a4443a6d95e28997c982bab62758fb073c8f666b0251aaa57606c91ecd8b7d`.

- **floe_theme_test#1.1 — filled button / default [P]**
  - Branch evidence: L10–10 in this file.
  - Preconditions: FloeTheme.light filled button mouseCursor resolver.
  - Input/action: Resolve empty state set.
  - Expected outcome: SystemMouseCursors.click.
  - Failure/race and evidence limits: Only this single-state resolution is asserted; no pointer hit-testing or combined-state precedence.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

- **floe_theme_test#1.2 — filled button / hovered [P]**
  - Branch evidence: L10–10 in this file.
  - Preconditions: FloeTheme.light filled button mouseCursor resolver.
  - Input/action: Resolve hovered state.
  - Expected outcome: SystemMouseCursors.click.
  - Failure/race and evidence limits: Only this single-state resolution is asserted; no pointer hit-testing or combined-state precedence.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

- **floe_theme_test#1.3 — filled button / disabled [P]**
  - Branch evidence: L10–10 in this file.
  - Preconditions: FloeTheme.light filled button mouseCursor resolver.
  - Input/action: Resolve disabled state.
  - Expected outcome: SystemMouseCursors.basic.
  - Failure/race and evidence limits: Only this single-state resolution is asserted; no pointer hit-testing or combined-state precedence.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

- **floe_theme_test#1.4 — outlined button / default [P]**
  - Branch evidence: L11–11 in this file.
  - Preconditions: FloeTheme.light outlined button mouseCursor resolver.
  - Input/action: Resolve empty state set.
  - Expected outcome: SystemMouseCursors.click.
  - Failure/race and evidence limits: Only this single-state resolution is asserted; no pointer hit-testing or combined-state precedence.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

- **floe_theme_test#1.5 — outlined button / hovered [P]**
  - Branch evidence: L11–11 in this file.
  - Preconditions: FloeTheme.light outlined button mouseCursor resolver.
  - Input/action: Resolve hovered state.
  - Expected outcome: SystemMouseCursors.click.
  - Failure/race and evidence limits: Only this single-state resolution is asserted; no pointer hit-testing or combined-state precedence.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

- **floe_theme_test#1.6 — outlined button / disabled [P]**
  - Branch evidence: L11–11 in this file.
  - Preconditions: FloeTheme.light outlined button mouseCursor resolver.
  - Input/action: Resolve disabled state.
  - Expected outcome: SystemMouseCursors.basic.
  - Failure/race and evidence limits: Only this single-state resolution is asserted; no pointer hit-testing or combined-state precedence.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

- **floe_theme_test#1.7 — text button / default [P]**
  - Branch evidence: L12–12 in this file.
  - Preconditions: FloeTheme.light text button mouseCursor resolver.
  - Input/action: Resolve empty state set.
  - Expected outcome: SystemMouseCursors.click.
  - Failure/race and evidence limits: Only this single-state resolution is asserted; no pointer hit-testing or combined-state precedence.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

- **floe_theme_test#1.8 — text button / hovered [P]**
  - Branch evidence: L12–12 in this file.
  - Preconditions: FloeTheme.light text button mouseCursor resolver.
  - Input/action: Resolve hovered state.
  - Expected outcome: SystemMouseCursors.click.
  - Failure/race and evidence limits: Only this single-state resolution is asserted; no pointer hit-testing or combined-state precedence.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

- **floe_theme_test#1.9 — text button / disabled [P]**
  - Branch evidence: L12–12 in this file.
  - Preconditions: FloeTheme.light text button mouseCursor resolver.
  - Input/action: Resolve disabled state.
  - Expected outcome: SystemMouseCursors.basic.
  - Failure/race and evidence limits: Only this single-state resolution is asserted; no pointer hit-testing or combined-state precedence.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

- **floe_theme_test#1.10 — icon button / default [P]**
  - Branch evidence: L13–13 in this file.
  - Preconditions: FloeTheme.light icon button mouseCursor resolver.
  - Input/action: Resolve empty state set.
  - Expected outcome: SystemMouseCursors.click.
  - Failure/race and evidence limits: Only this single-state resolution is asserted; no pointer hit-testing or combined-state precedence.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

- **floe_theme_test#1.11 — icon button / hovered [P]**
  - Branch evidence: L13–13 in this file.
  - Preconditions: FloeTheme.light icon button mouseCursor resolver.
  - Input/action: Resolve hovered state.
  - Expected outcome: SystemMouseCursors.click.
  - Failure/race and evidence limits: Only this single-state resolution is asserted; no pointer hit-testing or combined-state precedence.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

- **floe_theme_test#1.12 — icon button / disabled [P]**
  - Branch evidence: L13–13 in this file.
  - Preconditions: FloeTheme.light icon button mouseCursor resolver.
  - Input/action: Resolve disabled state.
  - Expected outcome: SystemMouseCursors.basic.
  - Failure/race and evidence limits: Only this single-state resolution is asserted; no pointer hit-testing or combined-state precedence.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

- **floe_theme_test#1.13 — segmented button / default [P]**
  - Branch evidence: L14–14 in this file.
  - Preconditions: FloeTheme.light segmented button mouseCursor resolver.
  - Input/action: Resolve empty state set.
  - Expected outcome: SystemMouseCursors.click.
  - Failure/race and evidence limits: Only this single-state resolution is asserted; no pointer hit-testing or combined-state precedence.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

- **floe_theme_test#1.14 — segmented button / hovered [P]**
  - Branch evidence: L14–14 in this file.
  - Preconditions: FloeTheme.light segmented button mouseCursor resolver.
  - Input/action: Resolve hovered state.
  - Expected outcome: SystemMouseCursors.click.
  - Failure/race and evidence limits: Only this single-state resolution is asserted; no pointer hit-testing or combined-state precedence.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

- **floe_theme_test#1.15 — segmented button / disabled [P]**
  - Branch evidence: L14–14 in this file.
  - Preconditions: FloeTheme.light segmented button mouseCursor resolver.
  - Input/action: Resolve disabled state.
  - Expected outcome: SystemMouseCursors.basic.
  - Failure/race and evidence limits: Only this single-state resolution is asserted; no pointer hit-testing or combined-state precedence.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

- **floe_theme_test#1.16 — slider / default [P]**
  - Branch evidence: L15–15 in this file.
  - Preconditions: FloeTheme.light slider mouseCursor resolver.
  - Input/action: Resolve empty state set.
  - Expected outcome: SystemMouseCursors.click.
  - Failure/race and evidence limits: Only this single-state resolution is asserted; no pointer hit-testing or combined-state precedence.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

- **floe_theme_test#1.17 — slider / hovered [P]**
  - Branch evidence: L15–15 in this file.
  - Preconditions: FloeTheme.light slider mouseCursor resolver.
  - Input/action: Resolve hovered state.
  - Expected outcome: SystemMouseCursors.click.
  - Failure/race and evidence limits: Only this single-state resolution is asserted; no pointer hit-testing or combined-state precedence.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

- **floe_theme_test#1.18 — slider / disabled [P]**
  - Branch evidence: L15–15 in this file.
  - Preconditions: FloeTheme.light slider mouseCursor resolver.
  - Input/action: Resolve disabled state.
  - Expected outcome: SystemMouseCursors.basic.
  - Failure/race and evidence limits: Only this single-state resolution is asserted; no pointer hit-testing or combined-state precedence.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

- **floe_theme_test#1.19 — list tile / default [P]**
  - Branch evidence: L16–16 in this file.
  - Preconditions: FloeTheme.light list tile mouseCursor resolver.
  - Input/action: Resolve empty state set.
  - Expected outcome: SystemMouseCursors.click.
  - Failure/race and evidence limits: Only this single-state resolution is asserted; no pointer hit-testing or combined-state precedence.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

- **floe_theme_test#1.20 — list tile / hovered [P]**
  - Branch evidence: L16–16 in this file.
  - Preconditions: FloeTheme.light list tile mouseCursor resolver.
  - Input/action: Resolve hovered state.
  - Expected outcome: SystemMouseCursors.click.
  - Failure/race and evidence limits: Only this single-state resolution is asserted; no pointer hit-testing or combined-state precedence.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

- **floe_theme_test#1.21 — list tile / disabled [P]**
  - Branch evidence: L16–16 in this file.
  - Preconditions: FloeTheme.light list tile mouseCursor resolver.
  - Input/action: Resolve disabled state.
  - Expected outcome: SystemMouseCursors.basic.
  - Failure/race and evidence limits: Only this single-state resolution is asserted; no pointer hit-testing or combined-state precedence.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

- **floe_theme_test#1.22 — popup menu / default [P]**
  - Branch evidence: L17–17 in this file.
  - Preconditions: FloeTheme.light popup menu mouseCursor resolver.
  - Input/action: Resolve empty state set.
  - Expected outcome: SystemMouseCursors.click.
  - Failure/race and evidence limits: Only this single-state resolution is asserted; no pointer hit-testing or combined-state precedence.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

- **floe_theme_test#1.23 — popup menu / hovered [P]**
  - Branch evidence: L17–17 in this file.
  - Preconditions: FloeTheme.light popup menu mouseCursor resolver.
  - Input/action: Resolve hovered state.
  - Expected outcome: SystemMouseCursors.click.
  - Failure/race and evidence limits: Only this single-state resolution is asserted; no pointer hit-testing or combined-state precedence.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

- **floe_theme_test#1.24 — popup menu / disabled [P]**
  - Branch evidence: L17–17 in this file.
  - Preconditions: FloeTheme.light popup menu mouseCursor resolver.
  - Input/action: Resolve disabled state.
  - Expected outcome: SystemMouseCursors.basic.
  - Failure/race and evidence limits: Only this single-state resolution is asserted; no pointer hit-testing or combined-state precedence.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

### floe_theme_test#2: 'tooltips wait two seconds before appearing on hover'
Source: `apps/client/test/design_system/floe_theme_test.dart:27–56`; `testWidgets`; 1 expanded registration(s); SHA-256 `2f75cd5599baf43c936ff71eef79ba505a56947370039c73eff627d8b3ab46ed`.

- **floe_theme_test#2.1 — delayed tooltip [P]**
  - Preconditions: Enabled Settings IconButton under light theme.
  - Input/action: Hover; pump 1999 ms then 1 ms and settle.
  - Expected outcome: Settings tooltip absent before 2 seconds and appears once at 2 seconds.
  - Failure/race and evidence limits: Exact delay uses widget time; no touch, focus or hover-exit race.
  - Target classification/disposition: Reassess two-second delay as product/accessibility timing, not immutable architecture contract.
## `apps/client/test/design_system/floe_toast_test.dart`

Full read: lines 1–263; 8 direct sites / 11 expanded registrations / 11 scenarios.
File SHA-256: `a7a3fe6d0b16fa2ee9f38f1dd9455eb126fc5a292f31893ffa6943f53b73906d`

Current owner: FloeToastHost state, animation, timer, lifecycle/focus handling
Target owner: Shared notification presentation; underlying Undo action remains caller/domain-owned.
Harness/support: mount (12–47) supplies viewport, scale, reduced motion, accessibility and keyboard inset; resets view settings. Toast actions are synthetic callbacks, timers advance through widget pump; no real external write is undone.

Dependencies (actual source imports; production imports remain KEEP):
- L1: `package:floe_client/app/floe_theme.dart` → `apps/client/lib/app/floe_theme.dart`
- L2: `package:floe_client/app/floe_toast.dart` → `apps/client/lib/app/floe_toast.dart`
- L3: `package:floe_client/l10n/app_localizations.dart` → `apps/client/lib/l10n/app_localizations.dart`
- L4: `package:flutter/gestures.dart` → `SDK/package dependency`
- L5: `package:flutter/material.dart` → `SDK/package dependency`
- L6: `package:flutter/services.dart` → `SDK/package dependency`
- L7: `package:flutter_test/flutter_test.dart` → `SDK/package dependency`

### floe_toast_test#1: 'retains newest three and expires without blocking workspace'
Source: `apps/client/test/design_system/floe_toast_test.dart:49–65`; `testWidgets`; 1 expanded registration(s); SHA-256 `887f7e2804d65327f336ba24d23cdd8679989a7381405a57239559cb2ec14d24`.

- **floe_toast_test#1.1 — stack cap and expiry [P]**
  - Preconditions: Wide1440×900 default host.
  - Input/action: Show Saved0,1,2,3 synchronously; settle; tap Workspace; advance5s.
  - Expected outcome: Oldest Saved0 absent, newest Saved3 present, three Close controls; after expiry no Close; no exception.
  - Failure/race and evidence limits: Toast layer permits workspace input; retained count3 and lifetime are P; callback effect of Workspace not counted.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

### floe_toast_test#2: 'hover expands and pauses remaining lifetime'
Source: `apps/client/test/design_system/floe_toast_test.dart:67–108`; `testWidgets`; 1 expanded registration(s); SHA-256 `fffb0b051495198cb2ae921bda320488cc995f9f64bbae3eed5138a896f1ca9c`.

- **floe_toast_test#2.1 — hover and inter-card gap pause [P]**
  - Preconditions: Default host, First then Second notices.
  - Input/action: Hover above stack then over Second; wait 6 seconds; move into gap and wait 6 seconds; move outside; wait 5 seconds.
  - Expected outcome: Collapsed text spacing<30; hovered stack retains First and expands to>30 separation; gap remains10±0.1 with pause; after exit Second expires.
  - Failure/race and evidence limits: Hover coverage includes inter-card gap so timer does not restart prematurely. Exact spacing/lifetime is P.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

### floe_toast_test#3: 'keyboard focus pauses and Escape dismisses'
Source: `apps/client/test/design_system/floe_toast_test.dart:110–123`; `testWidgets`; 1 expanded registration(s); SHA-256 `b71c4955b6cc94a1f56e1ac2f7a2f2ddb2bedd389c0c58d20897c7c0b01b4ea9`.

- **floe_toast_test#3.1 — focus pause and Escape [P]**
  - Preconditions: Keyboard notice with Close icon focusable.
  - Input/action: Request focus on close button; wait 6 seconds; Escape; settle and300 ms.
  - Expected outcome: Notice survives while focused then disappears after Escape.
  - Failure/race and evidence limits: Keyboard dismissal is explicit; no underlying action callback invoked.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

### floe_toast_test#4: 'mixed heights normalize collapsed and restore expanded: $tallFirst'
Source: `apps/client/test/design_system/floe_toast_test.dart:126–179`; `testWidgets`; 2 expanded registration(s); SHA-256 `86b64a5fe20adf0c16ac337a35179861569b47afccdc9f1f124357f7ee104f04`.

- **floe_toast_test#4.1 — tallFirst=true [P]**
  - Branch evidence: L125–180 in this file.
  - Preconditions: Two notices: tall four-line description first/back, short Task completed with Undo second/front.
  - Input/action: Observe collapsed; hover front; exit; add third short notice; remove pointer and unmount.
  - Expected outcome: Collapsed older/front heights equal, older bottom above front; expanded tall exceeds short by>40 and gap10±0.1; exit returns both to captured collapsed height; after new short, first and front each equal newest height; disposal no exception.
  - Failure/race and evidence limits: Both orderings explicitly covered; callbacks are empty and visual height normalization is P.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

- **floe_toast_test#4.2 — tallFirst=false [P]**
  - Branch evidence: L125–180 in this file.
  - Preconditions: Two notices: tall four-line description second/front, short Task completed with Undo first/back.
  - Input/action: Observe collapsed; hover front; exit; add third short notice; remove pointer and unmount.
  - Expected outcome: Collapsed older/front heights equal, older bottom above front; expanded tall exceeds short by>40 and gap10±0.1; exit returns both to captured collapsed height; after new short, first and front each equal newest height; disposal no exception.
  - Failure/race and evidence limits: Both orderings explicitly covered; callbacks are empty and visual height normalization is P.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

### floe_toast_test#5: 'Undo sits to the right of the message at text scale $scale'
Source: `apps/client/test/design_system/floe_toast_test.dart:183–201`; `testWidgets`; 2 expanded registration(s); SHA-256 `3a02b51c584a00aa714e3405bbc5e6e7a78c04e3b9817b404cb356a1d2e6e4f3`.

- **floe_toast_test#5.1 — scale 1 [P]**
  - Branch evidence: L182–202 in this file.
  - Preconditions: 390-wide host, text scale 1, Task completed notice with Undo.
  - Input/action: Render and measure message, Undo and Close.
  - Expected outcome: Undo sits right of text and left of Close; vertical center matches message within0.1; no exception; unmount.
  - Failure/race and evidence limits: No Undo action invoked; accessibility geometry only.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

- **floe_toast_test#5.2 — scale 2 [P]**
  - Branch evidence: L182–202 in this file.
  - Preconditions: 390-wide host, text scale 2, Task completed notice with Undo.
  - Input/action: Render and measure message, Undo and Close.
  - Expected outcome: Undo sits right of text and left of Close; vertical center matches message within0.1; no exception; unmount.
  - Failure/race and evidence limits: No Undo action invoked; accessibility geometry only.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

### floe_toast_test#6: 'background pauses expiry and dispose cancels timers'
Source: `apps/client/test/design_system/floe_toast_test.dart:204–222`; `testWidgets`; 1 expanded registration(s); SHA-256 `f21766a35992e1255070c4fad155e9209f25beb5bfbad99a8cb498b5dcca0a10`.

- **floe_toast_test#6.1 — background pause then disposal [D]**
  - Preconditions: Default host with Background notice.
  - Input/action: Set lifecycle inactive; wait 6 seconds; resume; wait 5 seconds; show Unmount then dispose host and wait 10 seconds.
  - Expected outcome: Notice survives inactive, expires after resumption; no delayed timer exception after unmount.
  - Failure/race and evidence limits: Background is pause, not dismissal; disposal cancels toast timers only, never a backend Run.
  - Target classification/disposition: Preserve safe timer/lifecycle cleanup without conflating UI disposal with backend cancellation; reassess toast expiry UX.

### floe_toast_test#7: 'accessible action remains available and invokes undo once'
Source: `apps/client/test/design_system/floe_toast_test.dart:224–242`; `testWidgets`; 1 expanded registration(s); SHA-256 `e853c9c91217bd7a339523ead5d06ca56301761f62e4ffec4667348a4fc94adc`.

- **floe_toast_test#7.1 — accessible Undo persists and dispatches once [D]**
  - Preconditions: accessibleNavigation=true host, Completed notice with Undo callback counter.
  - Input/action: Wait10s; press Undo; settle and300 ms.
  - Expected outcome: Undo still available before click; counter exactly1 afterward and notice disappears.
  - Failure/race and evidence limits: Access mode must not time out actionable notice before this interval; actual task recovery is outside fixture.
  - Target classification/disposition: Preserve accessible explicit action and one callback per activation; reassess exact persistence timing through final accessibility intent.

### floe_toast_test#8: 'fits $width with large text, keyboard and reduced motion'
Source: `apps/client/test/design_system/floe_toast_test.dart:245–261`; `testWidgets`; 2 expanded registration(s); SHA-256 `825dd113ad80596ab8b494be774c0cc92b7048ade638e7b2c90b8db577ba9502`.

- **floe_toast_test#8.1 — width 390 [P]**
  - Branch evidence: L244–262 in this file.
  - Preconditions: View 390×900, text scale 2, reduced motion and bottom keyboard inset 300; long wrapping title+description.
  - Input/action: Render; measure Close bounds; tap Close; advance 100 ms.
  - Expected outcome: Close right≤width−16 and bottom≤600, no exception; control absent after close.
  - Failure/race and evidence limits: Checks only Close geometry, not full toast clipping or real keyboard system integration.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

- **floe_toast_test#8.2 — width 1440 [P]**
  - Branch evidence: L244–262 in this file.
  - Preconditions: View 1440×900, text scale 2, reduced motion and bottom keyboard inset 300; long wrapping title+description.
  - Input/action: Render; measure Close bounds; tap Close; advance 100 ms.
  - Expected outcome: Close right≤width−16 and bottom≤600, no exception; control absent after close.
  - Failure/race and evidence limits: Checks only Close geometry, not full toast clipping or real keyboard system integration.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.
## `apps/client/test/preview/design_feedback_overlay_test.dart`

Full read: lines 1–213; 4 direct sites / 4 expanded registrations / 4 scenarios.
File SHA-256: `0c93f1d00b0e1427293dbd8e2bd6da1dea4afde6cc3c28324bb51ba36bdab4ec`

Current owner: DesignFeedbackOverlay production preview tooling
Target owner: Keep preview/feedback product tooling; tests and local screenshot stub are H, no authority on production rendering or export security.
Harness/support: toggleDesignFeedback (9–16) sends Meta+Shift+F. captureTestScreenshot (18–24) writes a real temp .png containing only four PNG signature bytes and returns path; not a valid image/render test. Clipboard MethodChannel mocked/restored in export tests. No teardown removes screenshot temp files in source; extraction did not execute or create any.

Dependencies (actual source imports; production imports remain KEEP):
- L1: `dart:convert` → `SDK/package dependency`
- L2: `dart:io` → `SDK/package dependency`
- L4: `package:floe_client/preview/design_feedback_overlay.dart` → `apps/client/lib/preview/design_feedback_overlay.dart`
- L5: `package:flutter/material.dart` → `SDK/package dependency`
- L6: `package:flutter/services.dart` → `SDK/package dependency`
- L7: `package:flutter_test/flutter_test.dart` → `SDK/package dependency`

### design_feedback_overlay_test#1: 'opens from the global shortcut while a field owns focus'
Source: `apps/client/test/preview/design_feedback_overlay_test.dart:27–43`; `testWidgets`; 1 expanded registration(s); SHA-256 `8730279ebcc1068481ae23efdf896b30cabf434ca1e68abd4e330b6d6c184b0b`.

- **design_feedback_overlay_test#1.1 — global shortcut with input focus [P]**
  - Preconditions: Overlay surrounds autofocused TextField.
  - Input/action: Render; send Meta+Shift+F.
  - Expected outcome: Initially one input and no Inspect; afterward Inspect appears.
  - Failure/race and evidence limits: Global shortcut works while field owns focus; no text-entry preservation or shortcut collision branch.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

### design_feedback_overlay_test#2: 'selects a rendered target and creates an editable pin'
Source: `apps/client/test/preview/design_feedback_overlay_test.dart:45–88`; `testWidgets`; 1 expanded registration(s); SHA-256 `42f0d37acade8c2b82b4e53113ed16e07327642fa3516f8c1eb9892a07e09b3a`.

- **design_feedback_overlay_test#2.1 — select, edit and delete pin [P]**
  - Preconditions: 900×700 view; overlay wraps rendered Review target text.
  - Input/action: Toggle inspector; click Inspect and target center; enter Increase the contrast.; Save pin; open pin; Delete.
  - Expected outcome: Hidden launch tooltip initially absent; Add feedback appears; pin 1 and 1 pins after save; edit dialog retains comment; deletion removes pin 1 and shows 0 pins.
  - Failure/race and evidence limits: Pin lifecycle is local preview state; no application data deletion or external transmission.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

### design_feedback_overlay_test#3: 'copies Markdown feedback to the clipboard'
Source: `apps/client/test/preview/design_feedback_overlay_test.dart:90–144`; `testWidgets`; 1 expanded registration(s); SHA-256 `27416380adca6a7cce77cfaa71a54df29c15e33c4ff3b801bd6ac6d1b3e2c10f`.

- **design_feedback_overlay_test#3.1 — Markdown clipboard export [P]**
  - Preconditions: Overlay uses screenshot stub and mocked Clipboard.setData; Export target Text is selectable.
  - Input/action: Add Align this with the rail. pin; press Copy Markdown.
  - Expected outcome: Captured clipboard text has Floe design feedback heading, comment, selector Text[text="Export target"], Text/text identifiers and screenshot path; file at extracted path exists; no exception.
  - Failure/race and evidence limits: File existence does not prove a valid screenshot. Clipboard mock receives data instead of real system clipboard; no export failure or sensitive-field redaction assertion.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

### design_feedback_overlay_test#4: 'exports stable control and page identification as JSON'
Source: `apps/client/test/preview/design_feedback_overlay_test.dart:146–212`; `testWidgets`; 1 expanded registration(s); SHA-256 `327f9e0db8dd95d9a248366acdf2245bee7b42c31510d372d96e98b65de15cad`.

- **design_feedback_overlay_test#4.1 — JSON identification export [P]**
  - Preconditions: Overlay uses stub screenshot and clipboard mock; IconButton key refresh-control, tooltip Refresh calendar.
  - Input/action: Inspect control; save Move this control.; Copy JSON.
  - Expected outcome: JSON version 2; one annotation selector includes key, renderObject nonempty, creatorChain includes IconButton; identifiers widgetType/key/tooltip preserved; screenshot path nonempty; no exception.
  - Failure/race and evidence limits: Despite title mentioning page identification, explicit expectations cover control/creator identity only; no page field is independently asserted. No valid PNG or external upload is proven.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.
## `apps/client/test/preview/design_system_catalog_test.dart`

Full read: lines 1–32; 1 direct sites / 1 expanded registrations / 1 scenarios.
File SHA-256: `9fc03e02450b2a07425e5594c9e86f5b3d2cf9221fd14a10ef15c0bbdf0bbe04`

Current owner: DesignSystemCatalog production preview screen
Target owner: Retain design-system catalog/entrypoint as reusable product preview; smoke-test source is test-only.
Harness/support: View 1024×1200/pixel ratio 1 restored after test; theme and production catalog imported. No support file or asset deletion authority.

Dependencies (actual source imports; production imports remain KEEP):
- L1: `package:floe_client/app/floe_theme.dart` → `apps/client/lib/app/floe_theme.dart`
- L2: `package:floe_client/preview/design_system_catalog.dart` → `apps/client/lib/preview/design_system_catalog.dart`
- L3: `package:flutter/material.dart` → `SDK/package dependency`
- L4: `package:flutter_test/flutter_test.dart` → `SDK/package dependency`

### design_system_catalog_test#1: 'catalog exposes the reusable component sections'
Source: `apps/client/test/preview/design_system_catalog_test.dart:7–31`; `testWidgets`; 1 expanded registration(s); SHA-256 `eb5eb3ef8f02449e6ea462739af0e2e29197430986dd9890b25db0cf4b66daad`.

- **design_system_catalog_test#1.1 — catalog sections [P]**
  - Preconditions: 1024×1200 display under light theme, debug banner disabled.
  - Input/action: Render DesignSystemCatalog and settle.
  - Expected outcome: Interaction colors, Typography, Status badges, Buttons, Fields and Selection each appear once; no widget exception.
  - Failure/race and evidence limits: Presence smoke only; component functionality, exact screenshots and scrolling not tested.
  - Target classification/disposition: Reassess the product/accessibility/presentation intent; exact widget types, keys, wording, timing and pixel constants are legacy evidence, not binding target requirements.

## Handoff limits

- Only this Markdown and JSON presentation ledger were written; no source rewritten/deleted.
- No tests, compilers, analyzers, formatters, builds, architecture checkers, provider calls, credential operations, development-data reset, Git push/PR/deployment ran.
- No current or future behavior is claimed passing; tests are untrusted historic product hypotheses except explicitly extracted durable properties.
- Runtime-generated collections (source files, widget overlays) are described as quantified assertions, not fabricated fixed case counts; fixed parameter tables are enumerated.
- Every scenario inherits its source registration span/hash and containing full-file hash, owner and direct dependencies.
- No imported production fake, preview tool, component, font/localization asset or dependency becomes test-only merely because a test uses it.
- Empty incoming lists are preparation lexical evidence; the aggregate removal step still owns residual consumer/manifest review.
