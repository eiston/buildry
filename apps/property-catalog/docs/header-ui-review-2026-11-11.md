# Shared header review — November 11, 2026

The shared `PageHeader` now wraps page headings, searches, form action bars, guest stays, and booking headers. Header geometry follows the actual sidebar boundary and viewport, including nested fieldsets and constrained form containers. The property editor keeps its existing body width while its toolbar spans the entire content pane.

Actions use shared buttons or icon links with accessible labels. Save, draft, task progression, booking, and calendar Today actions use icons. Date cells retain their dates. Duplicate lease, task, and administration search headers were removed. Maintenance pages now have the same page chrome and a single search header.

Compact form titles follow the actual heading position, including administration's account heading. They appear after the heading passes behind the sticky header and disappear when scrolling back. Form titles are 14px; public mobile titles use 12–13px. Dynamic Dioxus title nodes remain owned by Dioxus.

## Local validation

- Used the existing native API at `http://127.0.0.1:8092` and rebuilt the release web assets. No application container or database changes.
- `cargo check --target wasm32-unknown-unknown` passed, with the existing dead-code warning.
- `dx build --web --release` passed.
- `git diff --check` passed.
- Captured and visually inspected 144 screenshots across 48 routes at 1440px, 390px, and 320px. Checked that each page has one visible top header, spans the content pane, and has no document overflow. The calendar's date strip is part of its single header.
- Checked four administration layouts at all three widths with a synthetic user response and client-side navigation. The real local account is a regular user, so server authorization for those routes was not changed or validated as an administrator.
- Rechecked the final calendar icon and maintenance search changes at all three widths, plus the administration search header after removing its duplicate title bar.
- Verified property header edges at 768px (76px sidebar), 1440px (expanded 210px and collapsed 68px sidebar), and 1920px (constrained page body).
- Scrolled property, task, lease, public stay, and booking pages and confirmed the smaller title appears in the header. Checked lease wizard next/back navigation, calendar Today/search/back controls, maintenance search, and opening/closing the full-screen notebook without saving.

The payment backend reports partial/unavailable activity locally; payment headers were reviewed in those real recovery states. Invalid booking and bill links were checked as error states. No lease, booking, task, payment, or account was submitted during this review.

## Screenshots

Local screenshots and the Playwright route sweep are in `ui-review/2026-11-11-headers/` (gitignored).

- `verified-{width}-{index}.png`: the 48-route sweep; `audit.js` contains the ordered route list.
- `verified-contact-{width}-{0..3}.jpg`: visually inspected header contact sheets.
- `last-{width}-{0..3}.png`: final calendar and maintenance pages.
- `admin-fixture-{width}-{0..3}.png`: final administration list, search, new person, and person detail.
- `scroll-{1..5}-390.png`: compact titles on property, task, lease, stay, and booking pages.
- `property-768.png`, `property-1920.png`, `collapsed-1440.png`, and `notebook-1440.png`: responsive and full-screen editor cases.

The sweep covers properties/list/search/new/detail/rooms; pricing; vacancy; calendar; tasks/list/search/new/detail; leases/list/templates/search/new/detail/template editor; landlords/list/new/detail; sharing; apps; contacts; account/profile/security/preferences; rentals; maintenance/list/new/search; payments/activity/bills/compose/detail/settings/billing/autopay/payouts/verification; public stays/list/house/room; tour booking; and missing-booking recovery.
