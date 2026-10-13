# Floe landing page

A self-contained, static introduction to Floe, owned entirely by `apps/landing`.
It does not depend on, connect to, or change the Rust, Flutter, native, or Go
applications. There are no runtime or build dependencies beyond Node.js.

## Run and build

Use Node.js 22 or newer and the package's pinned pnpm version.

```sh
cd apps/landing
pnpm check
pnpm build
pnpm preview
```

Open `http://127.0.0.1:4173`. `PORT=4174 pnpm preview` selects another port.
`pnpm dev` serves the source directly. The preview server binds only to loopback.
The production artifact is `dist/`: ordinary HTML, CSS, JavaScript, and local
assets, suitable for a static host without a server-side runtime. These commands
do not deploy anything. The build recreates only this package's ignored `dist/`.

No dependency installation is required. If installing or adding dependencies,
use pnpm and retain `pnpm-lock.yaml`; do not add npm or Yarn lockfiles.

## Page story and design

The page starts with breathing room, makes the idea concrete through a small
sample day, explains three product principles, and separates client foundations
from future directions. Repository and documentation links are the next step;
there is no download, signup, pricing, or fabricated social proof.

- **Calm Flow:** tokens come from [`DESIGN.md`](../../DESIGN.md) and the client's
  [`design_tokens.dart`](../client/lib/app/design_tokens.dart). The page uses
  Inter, neutral-25, neutral-950, restrained primary-600, flat surfaces, and one
  faint ambient field. Blue and mint identify labeled sample user content.
- **Continuous corners:** `squircle.js` is one shared fourth-order superellipse
  renderer for the semantic corner sizes. Its decorative SVG stays behind native
  content and controls, preserving hit targets and external focus rings. It is
  a web interpretation, not a pixel-identical port of Flutter's 0.82-smoothed
  `FloeSquircle`. Rounded CSS corners are the no-JavaScript fallback.
- **Typography and motion:** the established 48/40/32px heading scale and compact
  content typography carry the hierarchy. Motion is limited to 140ms color
  feedback; there is no entrance animation, idle mascot animation, or parallax.
  Reduced-motion removes transitions.
- **Responsive behavior:** the application sidebar becomes a horizontal tablist;
  the timeline precedes stacked tasks and notes on phones. Content reflows at
  narrow widths and enlarged text rather than shrinking a desktop screenshot.
- **Interaction:** Day Canvas, Tasks, and Notes use native buttons with ARIA tab
  semantics, roving focus, orientation-aware arrow keys, Home, and End. The
  selected panel enters the normal Tab order. Sample task checkboxes stay in
  sync across the two illustrated views, announce changes, and reset on reload.
  This interaction belongs only to the illustration; nothing is sent or stored.
- **Access:** all controls have visible keyboard focus and at least 44px-high
  activation areas. The page includes a skip link, landmarks, headings, reduced
  motion and increased/forced contrast rules, and a useful no-script view.

## Evidence and claim boundaries

Source and documentation were reviewed from main starting at
`2e4ca99dd186696d3b452e695c85038f333b081f`.

| Page statement | Repository evidence |
| --- | --- |
| Open-source personal assistant, pre-stable, macOS stability focus | [Root README](../../README.md), [AGENTS.md](../../AGENTS.md) |
| Now/Next, one assistant, context, explicit action authority | [Product](../../docs/product/README.md), [principles](../../docs/product/principles.md) |
| Client foundations: Day Canvas, tasks, Notes | [Design implementation status](../../docs/design/README.md), [`personal_day_screen.dart`](../client/lib/features/day/presentation/personal_day_screen.dart), [`calendar_context_rail.dart`](../client/lib/features/day/presentation/calendar_context_rail.dart), [`personal_day/tasks.dart`](../client/lib/features/day/presentation/personal_day/tasks.dart), [`personal_day/notes.dart`](../client/lib/features/day/presentation/personal_day/notes.dart) |
| Voice, device handoff, contextual assistance are directions | [Capability roadmap](../../docs/product/roadmap.md) |
| Product illustration hierarchy | [Day Canvas specification](../../docs/design/screens/day-canvas.md), [foundations](../../docs/design/foundations.md) |

The mock workspace is explicitly labeled as an interactive illustration with
sample content. It is not a screenshot or a live version of the Flutter client.
No provider connection, release availability, security guarantee, user count,
testimonial, or completion of roadmap work is claimed. GitHub's releases endpoint
returned an empty list during implementation; CTAs use existing repository files
instead of speculative download URLs. External links navigate in the same tab.

## Assets

- `assets/floe-mascot.svg` is copied unchanged from the repository's canonical
  [`assets/floe-mascot.svg`](../../assets/floe-mascot.svg).
- `assets/fonts/InterVariable.woff2` is self-hosted from the
  [Inter project's font files](https://github.com/rsms/inter/tree/master/docs/font-files).
  Its SIL Open Font License is retained as `assets/fonts/Inter-OFL.txt`.
- The small interface icons are inline SVG. No icon CDN, remote font request,
  tracking script, account connection, form backend, or runtime API is used.

## Verification

Run `pnpm check`, `pnpm build`, and `git diff --check` for scoped source/build
checks. Browser QA should use Firefox and cover 1440×1000, 768×1024, 760×560,
390×844, and 320×760; check overflow, the three panels, pointer and keyboard task
completion, focus, text enlargement, reduced motion, and forced colors.

The sample UI is static presentation code, so this package does not require the
Rust/Flutter/Go runtime test gates. Browser screenshots and task-specific QA
reports are deliverable artifacts, not application source; keep them outside the
tracked package. Automated accessibility checks supplement, rather than replace,
screen-reader and real-device review.
