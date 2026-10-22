# Copy and information

Floe's baseline is **Calm Flow**, defined in [DESIGN.md](../../DESIGN.md).
This guide applies that existing visual language to product copy; it does not
replace domain behavior, accessibility requirements or permission disclosures.

## Visual baseline

- Keep neutral canvas and white continuous-corner cards. Use the existing
  `FloeCard`, `FloeSquircle`, semantic colors, spacing and typography rather than
  creating a competing screen-specific theme.
- Use violet for identity, selection and the primary action, not as a tint on
  every container. Ordinary success and cancellation are not errors.
- Group content by the user's decision: title, current state, material effects,
  then the available action. Keep long explanatory text to a readable measure
  (about 720 logical pixels on roomy layouts), while data views retain useful space.
- Let action groups wrap and text grow. Check narrow layouts and 200% text;
  never hide a consequence or shrink essential text to fit a desktop row.

## Give each fact a job

Keep text when it identifies something, changes the next action, explains an
outcome, prevents a consequential mistake or supports recovery. Each fact has
one primary home. Do not repeat it in the title, subtitle, body and status.

Use visible state first, a short action label next, and contextual details only
when useful. These are information levels, not mandatory rows. A clear icon
can reinforce a familiar action, but cannot replace an accessible label or a
material disclosure.

## Labels and states

- Name what a control actually does. A toggle for personal notes is **Personal
  only**, not **Filter**; expose its selected state to assistive technology too.
- Distinguish an empty collection from a filtered list with no matches. An empty
  collection should not offer to clear filters that do not exist.
- Use calm, concrete sentence case. Avoid decorative praise, false urgency,
  unexplained abbreviations and implementation narration.
- Translate internal lifecycle vocabulary into its consequence: **Connection
  pending**, **Microphone muted**, **Stopped on this device**. Do not expose
  command IDs, journal mechanics or leases in the ordinary reading path.
- Do not equate an admitted session with an active microphone or a confirmed
  remote connection. Keep those states separate when their consequences differ.
- Show neutral outcomes neutrally. Use error styling only for a failure or an
  unconfirmed outcome that needs attention.

Do not style a placeholder as an available action. If a control only opens a
not-available notice, omit it until the real flow exists. Retain genuine
unavailable states when they explain an existing operation or a recovery path.

## Truth and consent are not optional detail

Keep the recipient, transmitted data, microphone permission and playback/Stop
behavior visible before the voice action. Do not hide those facts in a tooltip
or a collapsed diagnostic section. A local Stop is not proof that a remote
service has closed; preserve uncertainty when present.

Never claim a successful save, connection, cancellation or recovery without
owner-issued evidence. Do not invent a recovery action or promise that setup
will enable a feature when availability is unknown. Preserve user-authored
content. Localize new product strings through the existing ARB catalog.

## Review

Inspect the production widgets with synthetic data before and after changes.
Cover initial, empty, filtered, pending, unavailable and error states; repeated
navigation, cancellation and recovery; desktop and narrow widths; large text
and keyboard/assistive labels. Static renders do not qualify real provider,
audio-device or platform lifecycle behavior.

Adapted from the [Roblox copy and information reference](https://github.com/syi0808/roblox/blob/d3a55b2f576c16f39779ec1b149a10202830f038/.agents/skills/roblox-ui-content-design/references/copy-and-information.md).
Its information hierarchy transfers to Floe; game-specific labels and character
budgets do not.
