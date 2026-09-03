# Shared Component Specifications

**Status:** target component contract

## `FloeSquircle`

All component shapes depend on `FloeSquircle`. Required inputs are semantic size, fill, border, clipping behavior, and optional elevation. It owns path generation for painting, clipping, focus, hit testing, and animation.

Acceptance criteria:

- the silhouette is identical across fill, border, focus, and clipping;
- resizing does not visibly change corner character;
- nested components use the documented token hierarchy;
- reduced motion and high contrast do not replace the shape with a local variant.

## Buttons

| Variant | Use | Default shape |
| --- | --- | --- |
| Primary | one highest-priority action per surface | `sq-sm`, violet fill |
| Secondary | alternative or reversible action | `sq-sm`, white fill, border |
| Tertiary | low-emphasis action | `sq-sm`, transparent fill |
| Icon | compact command with tooltip/label | `sq-xs` or `sq-sm` |
| Destructive | confirmed destructive action | `sq-sm`, semantic error |

All standard buttons are 44px high. Pointer-only compact targets may be 36px, but their touch presentation remains 44px. Disabled state changes fill, border, and text token; it does not rely on opacity alone.

## Segmented controls

Use for mutually exclusive local views such as `Day / Week / Month`. The group uses `sq-md`; items use `sq-sm`. Selection receives a subtle primary tint and primary text, not a pill sliding across unrelated screens.

Segmented controls are local tools. They must not substitute for global navigation.

## Inputs and capture

Inputs use `sq-md`, a 1px border, 48px minimum height, and an external 2px focus ring. Labels remain visible when content is present. Errors stay near the field and preserve the draft.

Universal Capture is a dedicated productivity control, not a chat composer. It accepts typed or voiced material, preserves the original, and asks for Event/Task/Note classification when required by policy.

## Cards

Cards use `sq-lg`, 20–24px padding, and flat white or a step-50 tint. A card exists only when grouping or interaction needs a boundary.

- Do not wrap every timeline row in a card.
- Do not tint the entire card more strongly than step `100`.
- Do not use a sparkle on ordinary user-authored notes.
- Selection uses border, focus, and semantics in addition to tint.

## Timeline and calendar blocks

Calendar blocks use `sq-sm` or `sq-md` depending on height. Each block exposes title and time; color is supplemental. Current time uses a labeled line with sufficient contrast and an accessible current-time description.

Floe content never floats over an event block. Suggestions appear in the contextual rail or a reserved inline slot on a narrow layout.

## Task rows and checkboxes

Task rows remain open list structures with whitespace or hairlines. Checkbox shape is `sq-xs`, with a minimum 44px activation target around the visible control. Completion retains readable text and offers undo where mutation is immediate.

## Contextual rail

The rail is a composition region, not a monolithic card. It may contain, in order:

1. context-specific items such as Today's tasks;
2. related notes or source information;
3. one Floe entry or suggestion.

Each region uses the same `sq-lg` card system. Floe does not receive floating geometry that makes it appear detached from the product.

## Menus, popovers, dialogs, and sheets

- Menus and popovers use `sq-md` or `sq-lg` and originate from their trigger edge.
- Dialogs and sheets use `sq-xl`.
- Desktop dialogs are reserved for focused confirmation or short workflows.
- Narrow layouts use a bottom sheet or full-height panel with the same information and action order.
- Opening a dialog preserves the underlying context; closing restores focus to the trigger.

## Floe mascot

The canonical mark is the mouthless, two-eyed, softly flowing head. Approved sizes are 44px brand, 36px assistant entry, 32px suggestion, and 24px inline attribution. Do not add a halo, outer badge, or unique shadow per context.
