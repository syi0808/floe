# Floe Mascot Motion

**Status:** implementation baseline  
**Source artwork:** [`assets/floe-mascot.svg`](../../assets/floe-mascot.svg)

## Purpose

The mascot is a quiet expression of Floe's current interaction state. Motion explains an active state or acknowledges a user-triggered event; it is not decoration and must not compete with the user's calendar, task, note, or decision.

The canonical mascot geometry and gradients remain unchanged. The Flutter client derives two render layers from the same SVG: the body and the eyes. Separating those layers lets the client animate transforms without redrawing or restyling the mascot.

## Motion states

| State | Behavior | Lifecycle | Typical use |
| --- | --- | --- | --- |
| `idle` | Canonical static mascot | Static | Passive entry, navigation, ordinary attribution |
| `listening` | Slight vertical stretch and widened eyes | Repeats only while active | Explicit voice/listening capture |
| `talking` | Gentle asymmetric body pulse | Repeats only while active | Floe voice/output delivery |
| `thinking` | Small lean with eye tracking | Repeats only while active | Waiting for model reasoning or response preparation |
| `working` | Slow restrained pulse | Repeats only while active | Expert/tool progress after the user has initiated work |
| `success` | One soft lift/expand and settle | One shot, 300ms | Successful completion feedback |
| `error` | Small decaying horizontal shake | One shot, 320ms | Recoverable failure feedback |
| `acknowledgement` | Small lean with one blink | One shot, 260ms | Explicit invocation, tap, or wake acknowledgement |

`idle` never loops. Passive Floe presence never animates to solicit attention. Do not use `success`, `error`, or `acknowledgement` as repeating attention devices.

## Motion rules

- Preserve the original silhouette, eye placement, lavender body gradient, and dark-violet eye gradient.
- Animate transform only: translation, scale, rotation, and eye-layer transform. Do not morph the body path.
- Keep amplitudes small enough that the mascot remains visually anchored to its layout slot.
- Do not add a mouth, speech bubble, sparkle pulse, unread badge, autonomous bounce, or emotional facial system.
- Continuous motion is allowed only while it communicates an ongoing user-visible state such as listening, talking, thinking, or working.
- Stop continuous motion immediately when that state ends.
- Do not use mascot motion as the only source of state information; adjacent text, progress, or control state remains authoritative.

## Reduced motion

When `MediaQuery.disableAnimations` is enabled, every motion state falls back to the canonical static `floe-mascot.svg`. This intentionally removes translation, scale, rotation, blink compression, and looping motion rather than replacing them with another animated treatment.

## Flutter API

The existing call remains unchanged and static:

```dart
const FloeMascot(size: 32)
```

Choose a state only when the surrounding product state actually owns that meaning:

```dart
const FloeMascot(
  size: 32,
  motion: FloeMascotMotion.thinking,
)
```

Callers should derive `motion` from explicit presentation state rather than timers. For one-shot states, transition the widget into `success`, `error`, or `acknowledgement` when the corresponding event occurs and then return to `idle` or the next active state.

## Asset ownership

The repository-level `assets/floe-mascot.svg` is the visual source of truth. The Flutter package keeps its existing canonical copy plus two derived runtime layers:

- `apps/client/assets/floe-mascot-body.svg`
- `apps/client/assets/floe-mascot-eyes.svg`

When the canonical mascot geometry or gradients change, update the Flutter canonical copy and both derived layers in the same change. The split layers must remain visually reconstructible into the canonical asset at rest.
