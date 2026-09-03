# Design Foundations

**Status:** normative detail for `DESIGN.md`

## Visual hierarchy

Floe uses neutral space for structure and color for meaning. The default order of emphasis is:

1. current user context or primary object;
2. next required action;
3. supporting details and related objects;
4. optional Floe help;
5. historical or distant information.

Do not give the primary content and the contextual rail equal visual weight.

## Squircle geometry

`FloeSquircle` is the single source of truth for continuous corners. It must produce a smooth, curvature-continuous corner through a platform-native continuous-corner shape or a shared superellipse path. A regular rounded rectangle is a fallback only where the platform cannot render the canonical path.

| Token | Extent | Typical use |
| --- | ---: | --- |
| `sq-xs` | 8px | checkbox, compact icon target, dense inline control |
| `sq-sm` | 12px | button, segmented item, compact event block |
| `sq-md` | 16px | input, capture bar, small card |
| `sq-lg` | 20px | standard card, rail card, note card |
| `sq-xl` | 28px | dialog, sheet, assistant panel |
| `sq-frame` | 32px | optional desktop workspace frame |

Rules:

- Components request a semantic size; they do not draw their own path.
- A nested shape uses a smaller token than its parent.
- Borders, clipping, hit testing, focus rings, and ink effects follow the same path.
- Focus rings sit outside the silhouette and must not be clipped.
- Circular geometry is reserved for the mascot, avatars, status dots, and radio-like controls.
- Pills are reserved for short statuses and progress, never as the default control shape.

## Color

The complete palette is defined in `DESIGN.md`. Its role hierarchy is:

| Role | Token | Use |
| --- | --- | --- |
| Canvas | `neutral-25` | window and page background |
| Surface | `neutral-0` | focused cards, inputs, overlays |
| Subtle surface | `neutral-50` | hover or grouped background |
| Primary ink | `neutral-950` | headings and required content |
| Secondary ink | `neutral-600` | supporting copy and metadata |
| Border | `neutral-200` | standard separation |
| Strong border | `neutral-300` | hover, selected neutral edge |
| Brand/action | `primary-600` | primary CTA, focus, selection, Floe |

Broad tints stop at color step `100`. Steps `500–900` are for small marks, text that passes contrast, and interactive states. Never lower opacity to invent a token; add a reviewed explicit color when a new role is necessary.

### Categorical colors

Blue, mint, amber, and coral may help scan calendar or collection content. A color dot is always paired with a text label or established row position. Users can customize categories without changing semantic warning, error, or success colors.

### Purple semantics

Violet means Floe authorship, primary interaction, focus, or selection. It does not mean “focus event,” “planning note,” or another user category by default. The sparkle mark is exclusive to an AI action or AI-authored material.

## Typography

Use the tokens in `DESIGN.md` without per-screen variants.

- `display-*`: primary date, task title, or an exceptional empty state.
- `headline-*`: section and card headings.
- `body-*`: descriptions and note content.
- `label-*`: controls, metadata labels, and compact navigation.
- `numeric`: times, durations, counts, and changing values.

Long user content may wrap freely. Controls remain concise and use sentence case. Truncation must expose the complete value through detail view or accessible description.

## Spacing and density

The base rhythm is 4px; 8px and 16px are the common intervals.

- 8–12px inside compact controls.
- 16–24px inside cards and rows.
- 24–32px between related sections.
- 48–64px between major page regions.

Dense days fold detail before shrinking type or targets. Empty space is functional and must not be filled with low-value suggestions.

## Borders and elevation

The base interface is flat. Use one of these separation methods in order: spacing, tonal change, 1px border, then shadow.

- Cards: 1px `border`, normally no shadow.
- Hover: `border-strong` or `surface-subtle`.
- Popover: `0 8px 24px #15182B14` plus border.
- Dialog or sheet: `0 20px 60px #15182B1F` plus border.

Do not combine tinted fill, strong border, and shadow on one routine surface.
