# Decision-first review and selection controls

Date: 2026-09-06. Two parallel implementation tracks integrated and checked together.

## Delivered

- Prototype Select and Dropdown share controlled selection/navigation, continuous
  corners, neutral surfaces, violet selection/focus and restrained origin-aware
  motion. Select commits a value; Dropdown invokes an action. Settings contains an
  explicitly simulated interaction preview, with no real settings/clipboard effects.
- Calendar review uses the shared Select. The prototype and Flutter review lead
  with title, human-readable destination, date/time and approval scope. Internal
  IDs, provider and useful ledger metadata remain in collapsed technical details;
  raw UTC/timezone metadata stays internal.
- Flutter formats immutable UTC instants in device-local time and includes both
  dates for overnight intervals. The proposal accepts local date/time and derives
  scheduling metadata internally. Same-day intervals show the date once. Expiry and blocked reasons are
  expressed plainly; successful creation is distinct from local collection.
- Product principles, DESIGN.md and assistant/action specifications record why
  decision-relevant meaning takes precedence over exposing implementation fields.

No executor, policy, persistence, calendar permission or native write gate changed.
This UI pass does not advance live S3 acceptance. No external event was created.

## Automated validation

- `pnpm check:components`: 48 component contracts.
- `pnpm check:actions`: 26 reducer assertions.
- `node scripts/check-selection-controls.mjs`: 13 navigation assertions and 11
  source guards. The guards are structural checks, not browser interaction tests.
- `pnpm build`: passes using the repository's Node 24/pnpm version.
- `flutter analyze`: no issues; `flutter test`: all 72 tests pass.
- Default macOS Debug build and strict deep signature verification pass. The actual
  bundled native capability response remains `writes_enabled=false`; the app was
  not launched and no Calendar access or permission prompt was invoked.
- Nine pure native assertions include acceptance of the internally derived local
  fixed-offset scheduling metadata; they do not read Calendar.
- Flutter review tests cover 390/1200 layouts, collapsed versus expanded IDs,
  readable local and overnight intervals, expiry/conflict explanations, decisions,
  in-flight behavior and lookup/read-only recovery regressions.

## Browser validation

Actual prototype browser, desktop default 1280×720, 390×844 and 320×760:

- Reviewed neutral/squircle surfaces and trigger alignment. Fixed a global button
  rule overriding the Select's left-label/right-chevron alignment.
- Opened Select inside the native dialog; its portal stayed interactive above the
  modal. Arrow navigation changed only the active item; Escape preserved the old
  selection, retained the dialog and returned focus to the trigger.
- End/Enter committed a different destination. Tab closed the popup and moved to
  Decline. Settings Select skipped disabled Holidays; typeahead selected Personal.
- Dropdown End/Enter selected the last enabled action, with explicit simulated
  feedback. Clicking outside dismissed without invoking another action.
- Observed upward placement near the lower desktop edge and downward placement
  in the review. Popups stayed within the measured 390/320 viewport width; the
  320px document had no horizontal overflow. Minimum-width review scrolls vertically.
- Technical details were absent from ordinary visible review text until expanded;
  proposal/execution/state data appeared on demand. Decline showed no-create state.
- Simulated missing-response lookup remained unresolved with no replacement-create
  action. Simulated collection failure offered only read retry and then showed
  collected success. These browser checks do not touch EventKit or the native DB.

Reduced-motion/forced-colors rules are implemented and source-checked, not verified
by changing OS accessibility settings in this run. VoiceOver, 200% text scaling and
native Flutter visual comparison remain unperformed. Flutter Select/Dropdown
parity is outside this prototype component pass; Flutter action review did change.
