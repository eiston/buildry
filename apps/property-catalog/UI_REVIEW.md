# UI review — October 31, 2026

## Plan and baseline

1. Review the current admin, guest, tenant, and private-link pages at desktop and mobile widths.
2. Improve the existing shared controls instead of introducing a second design system.
3. Improve gallery uploads, feedback, ordering, and keyboard access.
4. Resolve responsive and performance issues found during review.
5. Build, exercise workflows in the built-in browser, inspect screenshots, and record limits.

The newer `origin/codex/property-catalog` UI was merged into local `main` as
`ceaa940`. The newer app files resolve the parallel app additions; main's
existing deployment configuration is retained. Nothing has been pushed.
The unfinished work against the old UI is preserved in a Git stash.

## Changes

| Area | Result |
| --- | --- |
| Shared inputs | Caller placeholders are respected; fields inherit their accessible label and required state from `FormField`. |
| Shared selectors | Explicit empty hints, disabled states, arrow-key navigation, Home/End, type-ahead, Escape, and focus restoration. Empty searches have a visible result state. |
| Shared actions | `FormButton` provides consistent sizing and disabled/busy semantics across existing primary actions. Booking house/time choices now use the same `FormSelect` as other forms. |
| Navigation | Mobile More menu exposes Pricing, Vacancy, Access, Guest site, and Sign out. Current-page semantics and a skip link were added. |
| Dialogs and editors | Shared modal focus containment and restoration; notebook editors focus their writing surface. Escape restores inline edits. |
| Uploads | Reusable keyboard-accessible file picker/dropzone in property galleries, tasks, and maintenance requests. Property uploads show per-file results and completed-file progress; failures do not stop valid files; failed files can be retried. |
| Galleries | Original hold-and-drag ordering and cover presentation restored at the user's request. The added arrow and cover buttons were removed. Remove/undo, same-file reselection, and lazy image decoding remain. |
| Pricing | A base-rent edit enables Save immediately. Raw drafts preserve intermediate input; invalid drafts prevent saving. |
| Copy | Removed redundant gallery instructions, property-manager explanations, pricing instructions, wizard descriptions, login/signup intros, and other explanatory subheaders. Labels, validation, consent/privacy text, and demo notices remain. Technical save messages now say “Saved.” |
| Responsive behavior | Fixed task-search header overflow, increased mobile field text, unified focus indicators, and supported reduced motion. |
| Rendering/payload | Memoized guest-card derivation, requested card-size image variants, lazy-loaded secondary images, bounded notebook history to 100 entries, and enabled gzip. Optional intersection lookups refresh in the background rather than blocking guest listings. |
| Docker | Full app/API/PostgreSQL setup, runtime assets for PDF/signing pages, cached Rust build targets, removal of obsolete generated bundles, and a server-test build target. |

## Page coverage

Browsed in the built-in browser at **1440 × 1000** and **390 × 844**.
Screenshots were inspected during implementation, including the property editor,
photo controls, calendar, pricing editor, task form, guest room, account forms,
lease PDF, and private-link pages.

| Pages | Checks |
| --- | --- |
| `/`, `/properties/search` | Populated catalog, search, navigation, desktop map. |
| `/properties/new`, `/properties/1` | Empty/populated forms, room tabs, shared selectors, notebook, photos and save. |
| `/pricing` | Populated matrix, base rent editing and saving, monthly-price dialog at both sizes. |
| `/calendar` and `/tours` alias | Desktop week calendar, mobile agenda, tour availability and tour detail dialog. |
| `/tasks`, `/tasks/search`, `/tasks/new`, `/tasks/1`, `/tasks/2` | Lists, empty states, filters, creation, all four workflow steps, notebook undo/redo, upload control. |
| `/vacancy` | Room/month matrix and contained horizontal scrolling. |
| `/leases`, `/leases/search`, `/leases/view?lease=…` | Empty/populated register, filters, demo detail, rendered lease PDF. |
| `/leases/new` | All five steps through stay/rent review, using a local demo guest; no delivery. |
| `/leases/templates`, `/leases/templates/search`, `/leases/templates/new?id=1` | Register/search, all four editor steps, additional-PDF picker and delivery review. |
| `/access`, `/access/search`, `/access/new`, `/access/view?user=1` | Directory, search, account fields, role picker and keyboard interaction. |
| `/access/landlords`, `/access/landlords/new` | Verified the newer branch's redirects to the unified Access screens. |
| `/stays`, `/stays/house/1`, `/stays/room/1/9001` | Populated cards, rental period controls, details, gallery/illustration labels and booking links. |
| `/book`, `/booking/{token}` | Booking, confirmation, management, rescheduling, cancellation and invalid-link state. |
| `/login`, `/signup`, `/forgot-password`, `/resend-verification` | Desktop/mobile layout, input hints, links, signup preview and unavailable-provider state. |
| `/verify-email/{invalid}`, `/set-password/{invalid}`, `/set-password/{demo-token}` | Invalid/expired links and the valid password-setup form; no password change. |
| `/my-home`, `/my-home/account` | Empty and populated tenant home, account controls; no password change. |
| `/my-home/requests`, `/my-home/requests/search`, `/my-home/requests/new`, `/my-home/requests/3` | Empty/populated requests, new request, successful local submission and detail. |
| `/lease/{demo-token}`, `/lease-template/{demo-token}` | Tenant and landlord-template review views, PDF rendering, signature form layout; no signature submission. |
| `/lease/{fixture-token}/photo-id` | Valid identity-document form from a temporary local fixture; no ID uploaded. The temporary non-demo fixture was removed. |
| `/privacy`, `/photo-id-terms` | Desktop/mobile document layout. |

No page-wide horizontal overflow was found in the audited cases. Wide pricing
and vacancy grids scroll inside their own containers. A desktop task-search
header overflow found in the initial audit was fixed and rechecked.

## Workflow verification

- Multiple images uploaded, reordered, made cover, removed, restored, saved, and reloaded.
- An HTML file and a valid JPEG in the same batch produced an individual error and a successful upload; retry processed only the failed file.
- Shared role picker opened with ArrowDown, moved focus between options, and closed with Escape, returning focus to its trigger.
- Notebook undo/redo and save worked; the editor receives focus when opened.
- Pricing saved a new demo-room rent directly after typing, without needing a preliminary blur.
- A local guest booking was created, rescheduled, and cancelled. The calendar detail and cancellation state rendered correctly.
- A local task traversed all four steps. A tenant maintenance request was submitted and its detail page opened.
- Lease and template viewers rendered 15 PDF canvases for the local fixtures.
- Docker app and PostgreSQL health checks pass. Server unit suite: **36 passed, 0 failed**.
- `git diff --check` passes.

## Performance evidence

A measured WebAssembly response was **4,267,863 bytes** without compression and
**1,120,038 bytes** with gzip: **73.8% fewer transferred bytes**. Subsequent small
UI builds can change the precise byte counts. This measures transfer size, not
an end-to-end page-speed score.

Local guest-listing requests completed in approximately **2–4 ms** after the
optional map lookup was removed from their loading path. Map-service response
time no longer gates the initial catalog response. Tests cover a pending cold
lookup and stale data during refresh without calling the external map service.

## Local data and limits

The real local API and authentication remain enabled. Demo rooms were added only
to initially empty houses 1 and 2; their names start with `Demo ·`. Local workflow
fixtures use `UI Demo` names and reserved email domains. The original database
was backed up to `/tmp/property-catalog-before-new-ui.sql` before the merge's
schema migration. Demo lease rows are removed by the app's existing startup
cleanup, so those preview records are temporary.

Compose enables `APP_LOCAL_PREVIEW=1` to render signup without an email provider.
This affects the GET view only: signup submission still requires the normal
email/verification configuration. No email was sent, no contract was signed,
no password was changed, and no government ID was uploaded. Live Google OAuth,
email delivery, actual signature completion, and encrypted ID submission were
not exercised. Drag/drop handlers are implemented; browser upload verification
used the file chooser rather than a physical OS drag gesture.

## Screenshots and audit files

The full screenshot and JSON audit collection is stored at:

`/Users/eiston.wei/.codex/visualizations/2026/10/31/01a1900f-6c8a-7323-87ec-8f05e1fd4297/ui-review/`

Final property screenshots:

- `property-desktop-final.jpg`
- `property-mobile-final.jpg`

Other inspected examples include `navigation-mobile.jpg`, `price-editor-mobile.jpg`,
`task-mobile.jpg`, `room-mobile.jpg`, `signup-desktop.jpg`, `signup-mobile.jpg`,
`tenant-home-mobile.jpg`, `lease-pdf-mobile.jpg`, and the signing/ID previews.

## Gallery follow-up

Restored the original clean gallery tiles and original cover badge. The existing
`assets/gallery-sort.js` hold-and-drag behavior is unchanged. Upload dropzones,
batch results, retry, and removal undo remain.
