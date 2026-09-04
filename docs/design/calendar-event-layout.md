# Calendar timeline and event density

## Time geometry

- The calendar has its own vertical scroll region covering 00:00–24:00. All-day
  events and zoom controls stay outside that region. Initial viewport starts at 08:00;
  date changes retain the viewed time, and zoom retains the top visible time.
- Base scale is 60px/hour (1px/minute). A violet range slider adjusts zoom from 1× to
  12× in 1× steps, with keyboard support and an accessible value. A five-minute
  event is 5px at 1× and 60px at 12×; never enlarge its actual time block to fit content.
- Compute top from minutes since midnight and height from end minus start. The hour
  lines and current-time marker share that coordinate system. No rounding to five
  minutes: five-minute increments are a design reference, not a precision limit.
- Draw only hourly labels/lines and faint half-hour guides, at every zoom. Do not draw
  five-minute grid lines. The final 24:00 line represents the next midnight.

## Information by duration

At base zoom, five-minute increments map to the following treatments. Apply the
pixel-height thresholds again when zoom changes, rather than fixing density by duration.

| Duration at 1× | Height | Content | Style |
| --- | --- | --- | --- |
| 5, 10, 15, 20 min | 5–20px | No inline text; full title/time on native hover tooltip, accessible name and detail dialog | Thin source-colored bar, source-color accent, no padding or decorative dot |
| 25, 30, 35 min | 25–35px | One-line title | Centered dot/title/lock; ellipsis, no time or provenance rows |
| 40, 45, 50, 55 min | 40–55px | Title + time | Centered two-line content with source dot and lock |
| 60 min and longer | 60px+ | Title + time + source/calendar context | Centered three-line content; no stretching text to fill the block |

Exact cutoffs: below 24px = bar; 24–39px = title; 40–57px = title/time;
58px and above = title/time/source. Card corners shrink with available height.
Source colors and timestamps never change with density. Metadata is never allowed
to overflow into the next event. Hover must not change geometry.

## Interaction and accessibility

- Every event remains a real focusable button, in chronological order. Enter/click
  opens the same full-detail dialog regardless of density. Focus is not removed from
  short events. Accessible names contain title, time, calendar and event purpose.
- Thin bars are not adequate touch targets at overview scale. Use 12× for five-minute
  events: a 60px block provides enough room to tap and read. Do not invisibly expand
  hit areas into neighboring events, which would make adjacent short events ambiguous.
- The scroll region is keyboard-focusable. Wheel/trackpad/touch and keyboard scrolling
  navigate it independently of the page. No automatic scroll on every refresh.
- Day-boundary records must be split/clipped to the displayed day's range in the
  eventual native layout. Collision columns for overlapping events are separate work;
  these prototype fixtures are non-overlapping. All-day spans remain outside this scale.

## Prototype verification

Fixtures include 12:30–12:35 and 23:55–24:00 five-minute events. Alongside 30-, 45-
and 60-minute records, they exercise thin bars, zoom density and the final day boundary.
This is a visual fixture, not native EventKit or timezone normalization acceptance.
