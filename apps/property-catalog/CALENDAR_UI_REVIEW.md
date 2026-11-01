# Calendar and tour UI review

Reviewed in a fresh built-in browser session on October 31, 2026. The previous
preview tab was closed before opening the new mock preview.

## Changes

- Reused FormButton for calendar and tour actions, extending its existing API
  with ID, style, and pressed-state attributes. Existing FormInput, FormSelect,
  DatePicker, AppForm, CollapsiblePanel, and party/stay summaries are retained.
- Mobile calendar includes Today, the year, selected-date semantics, and access
  to tour availability. Availability and agenda fit within the mobile viewport.
- Host reschedule, save, and destructive cancellation actions have visible labels.
  Mobile tour details retain a visible close control. Swipe dismissal starts in
  the sheet header and excludes interactive controls.
- Booking forms allow changing the preselected house, disable room selection until
  a house has rooms, and clear the chosen time after a booking conflict refresh.
- Removed redundant introductory/kicker copy. Cancelled and invalid booking links
  provide direct recovery actions. Guest management branding links to stays.

## Browser coverage

| Screen | Desktop 1440 × 1000 | Mobile 390 × 844 |
| --- | --- | --- |
| Calendar / tours alias | Populated and empty weeks; previous/next/Today | Agenda, Today, date strip; alias also checked at 320 × 740 |
| Tour availability | Open/closed house states | Open panel and switch update |
| Host tour details | Detail, reschedule, cancellation | Detail, reschedule, cancellation, close and keep actions |
| Booking | Form, property/room preselection, submission, confirmation, copy feedback | Shared house/room/date/time controls, no times, submission, confirmation |
| Guest management | Booking details, reschedule and cancel confirmation | Details, successful reschedule, keep booking, cancellation |
| Recovery | Cancelled and invalid links | Cancelled and invalid links |
| Search | — | Matching and no-result states |

Host rescheduling and cancellation updated the in-memory calendar. Guest booking,
rescheduling, cancellation, and clipboard feedback passed. No page-wide overflow
was found in the audited viewports. The inspected final browser console had no
warnings or errors. The production Docker build, JavaScript syntax check, and
`git diff --check` passed. No server logic changed; the server suite was not rerun.
Physical touch swipe gestures were not simulated.

## Isolated fixtures

Start the existing Compose application, then run:

```sh
python3 scripts/preview-tours.py
```

Preview: http://127.0.0.1:8081/calendar

Fixtures are held only in memory on a loopback-only development server. API writes
are never forwarded to the real app. Restarting the preview restores the initial
fixture set. Existing database bookings and authentication are unchanged, and no
email is sent. The mock server reuses built frontend assets from port 8080;
it is a UI preview, not a test of production authorization or booking constraints.

Screenshots and the guest route/overflow audit are saved in:

`/Users/eiston.wei/.codex/visualizations/2026/10/31/01a19357-c9ca-71a2-8a28-a86a67e3d035/tours/`
