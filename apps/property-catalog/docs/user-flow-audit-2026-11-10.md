# Buildry user-flow inventory

Code review followed by local browser verification, November 9–10, 2026. Source baseline: `a7e0b00`.

This inventory covers the distinct user actions, lifecycle transitions, permission branches, recovery paths and external integration paths discoverable in the current frontend and backend. It is a complete code-derived checklist at that level, not a claim that every input combination or every external service path has passed an end-to-end test.

## Original review environment

- Reused the existing native application at `http://127.0.0.1:8089`, serving the existing release web output. No new application container or image was created.
- Verified its process uses `buildry_unified_test`, separate from the working `buildry` database. Kept the PostgreSQL container, image and volume intact.
- Used the built-in in-app browser with owner, tenant, worker, administrator and fresh-account fixtures. Checked the default desktop surface and a 390 × 844 phone viewport, then reset the viewport.
- Email and Stripe credentials are absent on this review process. Photo-ID encryption is configured. No external email, live payment, legal signature or real identity upload was submitted.
- Created only three synthetic business records in the test database: task 6, maintenance request/task 7 and tour 2. Task lifecycle and maintenance submission were exercised. Tour 2 was booked, rescheduled and cancelled, and cancellation survived reload. Synthetic task/request records were removed after review; the cancelled tour is retained as a cancellation fixture.
- No application source code, existing property/room content, prices, permissions, account roles, credentials or personal profiles were changed. Authentication created/revoked normal review sessions; rental review links can rotate private tokens as part of normal navigation.
- Existing automated regression scripts and prior UI review were read as supporting context. They were not rerun and are not counted as browser passes in this report. The existing built app was reused; no fresh-build equivalence claim is made.

## Remediation verification — November 10, 2026

The first three findings below are **Done**. The original environment notes above describe the baseline review; the source and rebuilt application were changed during this remediation.

- Ran a native API on `http://127.0.0.1:8090` from `server/`, with the fresh release frontend and the existing `buildry_unified_test` database. Preserved the working database and PostgreSQL container, image and volume.
- Used the built-in in-app browser for real navigation and submissions. Inspected desktop/tablet and phone layouts at 1440, 805, 390 and 320 pixels; measured no page overflow in the checked 390/320 views and restored the viewport afterward. Compared the tour form and rental calendar against the prior screenshots in `ui-review/2026-11-08/`.
- Verified stale synthetic-room recovery; configured-house filtering; occupied house/room periods; explicit calendar changes, links and reload persistence; room → booking prefills; successful synthetic booking and cancellation; manual and tour-prospect draft creation; draft reload; rent editing; editor closure; and PDF preview reload. Drafts 40/41 are retained as synthetic review fixtures. The new synthetic tour was cancelled.
- Also fixed the centered standalone booking back arrow, indistinguishable selectable calendar months, empty amenities sections, redundant rental-period headings and PDF-open controls, duplicate property labels, stale draft PDFs after edits, and transiently enabled email actions before email status loads. Drafts now say Draft, with a concise Email unavailable status. Shared controls are reused.
- Checked missing-date, zero-rent, occupied-date, incomplete-email and custom-duration validation. Added regression coverage for unconfigured offers, preserved occupied periods and malformed email actions. Availability lookup failures now block booking and expose an error; that network-failure branch was reviewed in code, not injected in the browser.
- `cargo test`: **14 passed**. `cargo test --manifest-path server/Cargo.toml`: **47 passed, 2 ignored**. `cargo check --target wasm32-unknown-unknown`, `dx build --web --release` and `git diff --check` passed. The ignored database tests were not rerun. Browser console checked after final draft rendering: no warnings/errors.
- Email and Stripe are unconfigured. No external email, payment, signature or identity upload was submitted. Actual email delivery remains **Blocked** in F171; saving a draft is verified independently.

Screenshot evidence: [draft actions, phone](../ui-review/2026-11-10-audit-fixes/draft-ready-390.jpg), [saved draft, desktop](../ui-review/2026-11-10-audit-fixes/draft-saved-1440.jpg), [saved draft, phone](../ui-review/2026-11-10-audit-fixes/draft-saved-390.jpg), [occupied room](../ui-review/2026-11-10-audit-fixes/occupied-room-390.jpg), [selectable calendar months](../ui-review/2026-11-10-audit-fixes/calendar-open-options-390.jpg), [320-pixel calendar](../ui-review/2026-11-10-audit-fixes/room-calendar-320.jpg), [booking recovery](../ui-review/2026-11-10-audit-fixes/booking-recovery-390.jpg), [date picker](../ui-review/2026-11-10-audit-fixes/booking-date-picker-320.jpg). Screenshots remain local under the existing ignored `ui-review/` directory.

## Remediation verification — November 11, 2026

The next three unfinished findings, F207/F135/F127, are **Done**.

- Rebuilt the release frontend and ran the native API at `http://127.0.0.1:8090` against `buildry_unified_test`. Preserved the working database and existing PostgreSQL container, image and volume. No application container was created.
- Exercised the real local UI using the browser automation tools and the built-in in-app browser. Created synthetic tasks 8–12; verified backlog and planned creation, required-title and missing-schedule validation, backlog → planned, schedule Save/reload, Start work, and abort from all three active states. Aborted tasks survived reload with no date, time or estimate; API attempts to reverse terminal status failed. Removed all five synthetic tasks afterward.
- Confirmed Cancel/Escape dismiss the shared abort dialog and restore focus. Aborting ignores incomplete unsaved edits, explicitly warns that edits are discarded, and operates on the saved record. An injected abort failure left the task active and allowed a successful retry. A real view-only worker had no mutation controls; Work permission enabled abort of a scheduled task. View-only API writes were also rejected. Permission changes affected only the removed synthetic task.
- Verified Bills against the native unavailable response, including Retry remaining on Bills and no false No bills yet. Controlled browser responses separately verified partial data with a visible bill, successful empty data, and successful populated data. These controlled responses are UI coverage, not claims of Stripe delivery or reconciliation. Activity reuses the same warning component.
- Inspected screenshot output against the existing task and Bills screenshots under `ui-review/2026-11-08/`. Reviewed desktop at 1440 pixels and phone at 390/320 pixels. Both creation actions fit at 320 pixels; checked phone states had no page overflow. Reused shared controls; moved Abort inside the details card, corrected the empty general-task house label and read-only assignee display, and restored the in-app browser viewport afterward.
- `cargo test`: **14 passed**. Server tests: **47 passed, 2 ignored**. `cargo check --target wasm32-unknown-unknown`, final `dx build --web --release`, and `git diff --check` passed. Existing dead-code warnings remain. Console inspection found expected task-billing 503 responses because Stripe is unconfigured; the injected failure also generated its expected HTTP 500. No external payment or email was submitted.

Final screenshot evidence: [task creation, desktop](../ui-review/2026-11-11-audit-fixes/final-task-new-1440.jpg), [task creation, phone](../ui-review/2026-11-11-audit-fixes/final-task-new-390.jpg), [task creation, 320 pixels](../ui-review/2026-11-11-audit-fixes/final-task-new-320.jpg), [abort confirmation](../ui-review/2026-11-11-audit-fixes/final-abort-confirm-390.jpg), [Bills unavailable](../ui-review/2026-11-11-audit-fixes/final-bills-unavailable-390.jpg), [partial bills](../ui-review/2026-11-11-audit-fixes/bills-partial-390.png). Screenshots remain local in the ignored `ui-review/` directory.

## Remediation verification — November 11, 2026 (account and identity)

The next three unfinished findings, F239/F192/F183, are **Done**. The Stripe Identity integration below was additionally requested during remediation; its live provider flow remains unverified without configured Stripe credentials.

- Rebuilt the release frontend and ran the native API at `http://127.0.0.1:8090` against the existing `buildry_unified_test` database. Used a second native process on 8091 only to check the opt-in Stripe Identity surface with credentials absent. Preserved the working database and PostgreSQL container, image and volume; no application image was created. A concurrent review-server restart hit a migration deadlock; restarting sequentially recovered, and the primary server was checked healthy afterward.
- In the built-in in-app browser, saved Inactive on synthetic account 84, reloaded to confirm persistence, and checked disabled password-invitation action and failed sign-in. Restored Active through the editor; the same credentials then authenticated through the native API. Reused the shared status selector and removed redundant sign-in helper copy. Checked desktop and 390/320-pixel screenshots against the existing access screenshot.
- Followed My Rentals → confirmed fixture 38 → Review lease. Verified Tenancy confirmed and My Rentals, with no signing or identity-upload next step; reloaded and checked that direct confirmed `/photo-id` navigation returns to the lease. Corrected the browser title, private-link label and confirmed rental action; removed a duplicate signing-complete heading. Visually checked the rendered PDF and confirmation screenshot against `ui-review/2026-11-08/tenant-signing-before-390.jpg`.
- Created synthetic signed leases 43/44 with pending landlord signatures. On 43, submitted a generated 700-pixel color test pattern through the real tenant passport upload UI. As the owner, approval first failed because the image had not been viewed; View front then Approve ID succeeded. Reload retained approval while Awaiting landlord signature remained visible. The confirmation control stayed absent and a direct confirmation attempt returned 409. Native API checks denied image/review/approval/confirmation access to worker and tenant accounts. No real ID or new legal signature was submitted. Removed both synthetic leases and their image/access records afterward.
- Added opt-in Stripe Identity using the official Stripe.js `verifyIdentity` SDK. Documents go directly to Stripe; Buildry stores the session reference, consent and result, not document images or client secrets. The approver sees the verified name transiently. A provider result never automatically approves ID or confirms a tenancy; approval and confirmation retrieve the provider state server-side. Sessions are reused idempotently and scoped to the authenticated tenant, lease and Stripe mode. Added retention redaction requests and legal-hold support. Existing encrypted uploads keep their review flow.
- On the actual Stripe-enabled local process, checked consent validation, official SDK loading, unavailable response, re-enabled retry controls, Refresh status and Back to lease. Inspected screenshots at 1440, 390 and 320 pixels; reused the shared action layout to fix phone spacing. Checked phone widths had no horizontal overflow; restored the browser viewport. SDK contract tests separately cover cancellation, load failure, server failure and server-authoritative processing/verified states. A local simulated-provider/database test covers session reuse, transition reconciliation and clearing approval when verification no longer passes. These are simulations, not a claim that Stripe processed an identity document. Stripe test-account verification, the successful live modal/result UI, and provider redaction remain unverified until credentials and Identity access are configured.
- Final checks: frontend tests **14 passed**; server tests **49 passed, 3 ignored**; the explicitly selected simulated-provider database test **1 passed**; SDK contract tests **6 passed**. `cargo check --target wasm32-unknown-unknown`, release web build, native API build and `git diff --check` passed. The other two ignored database regressions were not rerun. Final account-page console had no warnings/errors; expected ID-review 409 and Stripe unavailable 503 responses occurred during negative-path tests. No external email, payment or identity submission was made.

Screenshot evidence: [account status, phone](../ui-review/2026-11-11-identity-fixes/account-status-options-390.png), [account status, 320 pixels](../ui-review/2026-11-11-identity-fixes/account-status-options-320.png), [account, desktop](../ui-review/2026-11-11-identity-fixes/account-active-1440.png), [confirmed tenant lease](../ui-review/2026-11-11-identity-fixes/confirmed-lease-390.png), [ID approved before landlord signature, desktop](../ui-review/2026-11-11-identity-fixes/id-approved-before-signature-1440.png), [ID approved before landlord signature, phone](../ui-review/2026-11-11-identity-fixes/id-approved-before-signature-390.png), [Stripe unavailable, phone](../ui-review/2026-11-11-identity-fixes/stripe-identity-unavailable-390.png), [Stripe unavailable, 320 pixels](../ui-review/2026-11-11-identity-fixes/stripe-identity-unavailable-320.png). Screenshots remain local in the ignored `ui-review/` directory. Coverage counts below apply to the original 261 inventory rows with their updated evidence; the additional Stripe integration is described in this section.

## Remediation verification — November 11, 2026 (property, contact and delivery)

The next three unfinished findings, F100/F061 and the conflicting delivery messages in F185, are **Done**. F185 remains **UI** because successful email delivery is unavailable locally.

- Rebuilt the release frontend and reused the native API on `http://127.0.0.1:8090` for the initial property flow. Started another native API directly from `server/` on `http://127.0.0.1:8092` for the remaining and final-build checks. Both used the existing `buildry_unified_test` database. Preserved the working database and PostgreSQL container/image/volume; created no application container.
- Used the built-in in-app browser to create synthetic property 7 through Add a room, without a prior Save property. Required name/address validation appeared. Named and saved room 1, reloaded it, returned to the parent property, and opened/backed out of a second room; the second room was not persisted. Final-build checks also verified new-property validation and hidden sharing actions. Removed synthetic property 7, its room and ownership afterward.
- Verified Contact us on public listing, room and house pages. Replaced unavailable messaging placeholders with Email us, using the email address already published in the privacy page. Checked the mailto destination without opening an email client or sending mail. Followed the generic tour action to the booking form; inspected the room-specific tour destination retaining room 9001, Jan 2031, 12 months and return URL. Occupied house dates offer Explore stays. Verified Close, Escape, focus restoration and forward/reverse keyboard focus wrapping using the shared dialog.
- On confirmed fixture 38, checked exactly one Signed copy not sent message. Native resend returned Email is not configured; the tenancy remained Confirmed and reload retained the unsent state. Temporary demo fixture 46, with a recorded `copy_sent_at`, showed Signed copy sent after reload; removed it afterward. This database fixture is display coverage, not proof of email delivery. The successful resend handler refreshes leases, but actual provider delivery and its resulting refresh remain unverified without email configuration. No external email, payment, legal signature or identity submission occurred.
- Compared new-property, room and lease layouts with existing screenshots under `ui-review/2026-11-08/`; visually inspected the contact dialog and final phone screens. Checked desktop at 1440 pixels and phones at 390/320 pixels. Measured no horizontal page overflow in the checked 320-pixel room/contact/lease views and the 390-pixel room view. Reused shared Dialog and FormButton components, removed redundant contact copy, and reset the viewport. Final browser console inspection returned no warning/error entries.
- Frontend tests: **14 passed**. Server tests: **49 passed, 3 ignored**; ignored database tests were not rerun. `cargo check --target wasm32-unknown-unknown`, final `dx build --web --release` and `git diff --check` passed. Existing dead-code warnings remain.

Screenshot evidence: [saved room, phone](../ui-review/2026-11-11-contact-property-fixes/room-saved-390.jpg), [saved room, 320 pixels](../ui-review/2026-11-11-contact-property-fixes/room-saved-320.jpg), [new-property validation](../ui-review/2026-11-11-contact-property-fixes/new-property-validation-320.jpg), [contact, desktop](../ui-review/2026-11-11-contact-property-fixes/contact-1440.jpg), [final contact, 320 pixels](../ui-review/2026-11-11-contact-property-fixes/contact-final-320.jpg), [unsent copy, phone](../ui-review/2026-11-11-contact-property-fixes/lease-not-sent-390.jpg), [final retry, 320 pixels](../ui-review/2026-11-11-contact-property-fixes/lease-retry-final-320.jpg), [sent-state database fixture](../ui-review/2026-11-11-contact-property-fixes/lease-sent-fixture-390.jpg). Screenshots remain local in the ignored `ui-review/` directory.

## Status key

| Status | Meaning |
| --- | --- |
| Pass | The stated action/branch was exercised in the browser and its visible outcome checked. |
| UI | Screen/control was inspected, or part of the flow was exercised; the final mutation or all branches were not completed. |
| Blocked | An attempted local path could not continue because its required external service is unavailable. |
| Code | Implemented branch traced in source; not executed in this browser pass. |
| Gap | Observed mismatch, absent UI action or code-derived unreachable/broken path. Evidence states whether reproduced. |

## Findings

| Finding | Evidence and implication |
| --- | --- |
| **Done** — Public fallback rooms can advertise an impossible booking | Removed fabricated room offers and excluded properties without rentable units. Stale room/house links show the missing state; booking can recover to a configured house. Verified in the local browser. See F060/F066. |
| **Done** — Requested occupied dates silently change | Preserved requested dates and duration on listing, house and room pages. Occupied periods stay unavailable; explicit changes update the URL and survive reload. Verified Jan–Dec 2030 and explicit 2031 selection. See F054. |
| **Done** — New lease wizard cannot save a draft while email is unavailable | Added shared Save draft action independent of email setup. Manual and tour-prospect drafts saved and persisted; email remains separate. Verified drafts 40/41 and draft editing/PDF refresh. See F172/F174. |
| **Done** — Bills hides partial-loading failure | Bills and Activity reuse the loading warning. Retry keeps the current page, available rows remain visible, and incomplete empty responses no longer say No bills yet. Verified against the native API and controlled browser responses. See F207. |
| **Done** — Task abort is API-only | Added Abort task using shared confirmation and button controls. Verified backlog/planned/in-progress abort, cleared schedules, reload persistence, cancellation, Escape/focus restoration, failure/retry, and view-only versus Work permissions. See F135. |
| **Done** — Backlog-only creation is missing from the task UI | Added Save to backlog independently of Create task. Verified required-title validation, backlog creation/reload, saving without advancement, planning, and existing direct planned creation. See F127/F129/F133. |
| **Done** — Account activation has no editor control | Added Account status using the shared selector. Active/inactive changes save and survive reload; inactive sign-in is denied and restoring Active permits the same credentials. Verified with synthetic account 84. See F239. |
| **Done** — Confirmed tenant review shows stale next steps | Confirmed reviews show Tenancy confirmed and My Rentals, with no signature or ID prompt. The rental action says Review lease; direct confirmed ID routes return to the lease. Verified fixture 38 and reload. See F192. |
| **Done** — ID review is gated on landlord signature in the UI | Approvers can view and approve ID before the landlord signs. Verified required-image review, persisted approval and permission denial; confirmation stays unavailable and the API rejects it until the landlord signs. See F183. |
| **Done** — Add a room on an unsaved new property uses update, not create | Creates new properties before opening the room editor; existing properties still use update. Verified validation, room save, reload and cancellation with synthetic property 7. Sharing stays hidden until the property exists. See F099/F100/F103. |
| **Done** — Contact channels remain placeholders | Replaced placeholder channels with Email us using the already-published privacy contact address. Listing, room and house pages share the existing dialog, with context-preserving tour/recovery links and keyboard dismissal/focus handling. No email sent. See F061. |
| **Done** — Signed-copy delivery messaging conflicts | Confirmed leases show one recorded state: Signed copy not sent or Signed copy sent. Native failed retry keeps confirmation and a single unsent state; successful resend now refreshes lease data. Sent-state display verified with a temporary database fixture; actual delivery remains unverified. See F185. |
| Legacy legal-profile routes disagree | `/leases/landlords/new` redirects to Add person; `/leases/landlords/view` redirects to `/access/landlords/view`, but frontend dispatch does not handle that path as a legal profile. Code-derived; modern `/landlords` flow exists. |
| README differs from current flows | Code/browser use a five-step lease wizard, nine tour starts through 6 PM, and manager email retry/status. Older README passages describe two steps, a 4 PM last start and older manager controls. Use current implementation for this checklist. |

## Lifecycle and permission notes

- Account interests install apps; they do not grant resource rights. App removal does not remove the underlying data or permissions.
- Task lifecycle is backlog → planned → in progress → done. Abort is terminal from active states through the API. Done/aborted cannot return to earlier states. Creator-only sharing/deletion/reassignment differs from Work access.
- Lease lifecycle is draft → sent → tenant-signed → confirmed, with the designated landlord signature and ID approval as separate prerequisites for confirmation. Ownership/editing, signing and ID approval are distinct capabilities.
- A signed/confirmed entire-house lease occupies all rentable spaces. Overlap rules apply across room and entire-house leases.
- Tour availability is global across houses for overlapping two-hour windows, in Toronto time. Current selectable starts are 10 AM through 6 PM; booking dates extend 60 days ahead.
- Payment statuses include requested/open, processing, attention, paid/recorded and closed/cancelled states. These are not a bank balance. Refunds, disputes and bank payout-ledger aggregation are not represented as complete user flows in the current activity app.
- No self-service account deletion, Google unlink, generic standalone bill creation, lease deletion or full refund/dispute-management UI was found. These are absent capabilities, not hidden flows certified by this audit.

## Coverage counts

| Area | Flows | Pass | UI | Blocked | Code | Gap |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Authentication | 18 | 5 | 4 | 0 | 9 | 0 |
| Workspace | 17 | 4 | 6 | 0 | 7 | 0 |
| Contacts and sharing | 11 | 1 | 4 | 0 | 6 | 0 |
| Public stays | 18 | 13 | 3 | 0 | 2 | 0 |
| Tours | 25 | 13 | 7 | 1 | 4 | 0 |
| Properties and rooms | 20 | 10 | 3 | 0 | 7 | 0 |
| Pricing and vacancy | 13 | 6 | 5 | 0 | 2 | 0 |
| Tasks | 19 | 11 | 4 | 1 | 3 | 0 |
| Landlord profiles | 6 | 1 | 2 | 0 | 3 | 0 |
| Lease templates | 13 | 5 | 3 | 0 | 5 | 0 |
| Lease management | 30 | 14 | 7 | 1 | 8 | 0 |
| Tenant rentals and maintenance | 13 | 7 | 4 | 0 | 2 | 0 |
| Payments | 29 | 4 | 3 | 4 | 18 | 0 |
| Administration | 11 | 4 | 2 | 0 | 5 | 0 |
| MCP and OAuth | 9 | 0 | 1 | 0 | 8 | 0 |
| Shared controls and recovery | 9 | 5 | 1 | 0 | 3 | 0 |
| **Total** | **261** | **103** | **59** | **7** | **92** | **0** |

## Inventory

Each row is a logical flow or a grouped family of closely related validation branches. Status applies to the precise observation in the last column, not to all unexecuted variants named in a row.

### Authentication

Sources: [server/src/auth.rs:288](/Users/eiston.wei/git/buildry/apps/property-catalog/server/src/auth.rs:288), [server/src/leasing.rs:264](/Users/eiston.wei/git/buildry/apps/property-catalog/server/src/leasing.rs:264), [server/src/google_auth.rs:99](/Users/eiston.wei/git/buildry/apps/property-catalog/server/src/google_auth.rs:99).

| ID | Actor / entry | Flow and branches | Expected behavior | Status | Evidence / limit |
| --- | --- | --- | --- | --- | --- |
| F001 | Visitor · `/login` | Sign in with an active password account | Redirect through / to the first installed app | **Pass** | Owner, tenant, worker, admin and fresh fixture accounts signed in |
| F002 | Visitor · `/login` | Submit an incorrect password | Stay on sign-in with a generic error and verification recovery link | **Pass** | Incorrect-password error observed |
| F003 | Visitor · `/login` | Submit missing or malformed credentials | Required/email validation; server rejects invalid credentials | **Code** | Form constraints and authenticate handler inspected |
| F004 | Visitor · `/login` | Sign in with an inactive or unverified account | Authentication denied; verification recovery remains available | **UI** | Nov 11: inactive synthetic account 84 failed browser sign-in; after reactivation, the same credentials signed in through the native API. Unverified-account branch remains code-reviewed. |
| F005 | Visitor · `/signup` | Open account creation | First/last name, email, password and confirmation fields | **UI** | Desktop/phone form inspected; no account submitted |
| F006 | Visitor · `/signup` | Create and verify an account | Inactive user; 24-hour verification email; activation only on POST confirmation | **Code** | Email configuration absent; successful creation/delivery not exercised |
| F007 | Visitor · `/signup` | Duplicate email, short password, mismatched confirmation or failed mail | Reject invalid input; duplicate conflict; roll back inactive user on delivery failure | **Code** | Signup handler inspected; requires email configuration before input validation |
| F008 | Visitor · `/verify-email/{token}` | Open an invalid, expired or consumed verification link | Expired screen links to resend verification | **Pass** | Invalid token displayed Verification link expired |
| F009 | Visitor · `/verify-email/{token}` | Open a valid link, then confirm verification | GET does not consume token; POST activates account once | **Code** | Verification handlers inspected; no fresh token consumed |
| F010 | Visitor · `/resend-verification` | Open verification recovery | Email entry and return-to-sign-in link | **UI** | Recovery form inspected |
| F011 | Visitor · `/resend-verification` | Request a new verification link, including unknown email or rapid repeat | Generic response; eligible unverified account only; two-minute throttle | **Code** | No outbound message requested |
| F012 | Visitor · `/forgot-password` | Open password recovery | Email entry and return-to-sign-in link | **UI** | Recovery form inspected |
| F013 | Visitor · `/forgot-password` | Request password reset | Generic response; active account; one-hour token; two-minute throttle | **Code** | No reset email requested |
| F014 | Visitor · `/set-password/{token}` | Open invalid or expired password invitation | Expired screen links to password recovery | **Pass** | Invalid token displayed Password link expired |
| F015 | Invited user · `/set-password/{token}` | Set/reset password using a valid invitation | Confirmation and minimum length; one-time token; sessions invalidated | **Code** | Credential change not submitted; setup invitations can last seven days |
| F016 | Visitor/user · `/auth/google/start; /auth/google/link` | Sign in, create Google user, or link an existing account | Provider state/cookie validation; verified identity; account matching rules | **Code** | Google is not configured locally; password login has no Google button |
| F017 | Visitor · `/login?next=...` | Follow a lease invitation through sign-in or signup | Only safe local return destinations survive authentication | **Code** | Return-path sanitization and invitation middleware inspected |
| F018 | Signed-in user · `Sign out` | End current session | Return to sign-in; authenticated routes require a new session | **Pass** | Desktop and mobile More-menu sign-out worked |

### Workspace

Sources: [src/main.rs:151](/Users/eiston.wei/git/buildry/apps/property-catalog/src/main.rs:151), [src/workspace.rs:109](/Users/eiston.wei/git/buildry/apps/property-catalog/src/workspace.rs:109), [src/sidebar.rs:1](/Users/eiston.wei/git/buildry/apps/property-catalog/src/sidebar.rs:1).

| ID | Actor / entry | Flow and branches | Expected behavior | Status | Evidence / limit |
| --- | --- | --- | --- | --- | --- |
| F019 | Fresh user · `/; /onboarding` | First login with no interests selected | Onboarding replaces the workspace; Continue disabled | **Pass** | Fresh stranger fixture reached onboarding |
| F020 | Fresh user · `/onboarding` | Select Tenant, Property owner, Worker, or a combination | Continue enables; starter apps are the union; interests grant no permissions | **UI** | Tenant+Worker checked; Continue enabled; preferences not submitted |
| F021 | Fresh user · `/onboarding` | Complete onboarding, including an invitation destination | Save interests/apps; return to requested page or first installed app | **Code** | Completion handler inspected |
| F022 | User · `/` | Open the workspace root | Redirect to first installed app, or Apps when none installed | **Pass** | Different account fixtures landed in Properties, My Rentals and Tasks |
| F023 | User · `/apps` | Browse installed and available apps | Eight apps; installed entries can open/remove and others can add | **UI** | Catalog and Add/Remove actions inspected |
| F024 | User · `/apps` | Search apps and clear the search | Filter catalog; show a no-results state when empty | **Code** | Search implementation inspected; not typed in this pass |
| F025 | User · `/apps` | Add/remove an app | Persist sidebar installation; preserve business data and resource access | **Code** | No installation changed |
| F026 | User · `Permitted direct links` | Open an app that is not installed | Resource permissions still govern access | **Pass** | Tenant opened Properties directly and saw an empty personal catalog |
| F027 | User · `/account` | Review/edit profile and contact information | Email displayed; first name required; save name/phone/photo | **UI** | Profile form inspected; no personal profile saved |
| F028 | User · `/account` | Upload/change profile photo | Shared upload flow; private visibility follows relationships | **Code** | Upload and profile-image authorization code inspected |
| F029 | User · `/account/security` | Review sign-in methods | Password form or Set password link; Google link/status; session revocation | **UI** | Password, confirmation, Google Connect and revoke controls inspected |
| F030 | User · `/account/security` | Change password | Current password and matching new passwords; return to sign-in | **Code** | Not submitted |
| F031 | User · `/account/security` | Sign out of all devices | Increment session version; revoke current and other sessions | **Code** | Control inspected; not submitted |
| F032 | User · `/account/preferences` | Change interests | Require at least one interest on save; no permission escalation | **UI** | Current choices and save control inspected |
| F033 | User · `/account/preferences` | Reorder/remove sidebar apps and save | Draft ordering; boundary buttons disabled; Apps utility remains available | **UI** | Move-up/down/remove controls and disabled first/last boundaries inspected |
| F034 | User · `Shared navigation` | Use sidebar or mobile More navigation | Open installed apps and permanent utilities; admin-only Administration link | **Pass** | Desktop links and phone More dialog opened; role-specific menus observed |
| F035 | User · `Shared navigation` | Collapse desktop sidebar and restore it | Persist compact navigation preference in browser storage | **Code** | Sidebar implementation inspected; default review surface was already compact |

### Contacts and sharing

Sources: [src/workspace.rs:185](/Users/eiston.wei/git/buildry/apps/property-catalog/src/workspace.rs:185), [src/workspace.rs:256](/Users/eiston.wei/git/buildry/apps/property-catalog/src/workspace.rs:256), [server/src/accounts.rs:168](/Users/eiston.wei/git/buildry/apps/property-catalog/server/src/accounts.rs:168).

| ID | Actor / entry | Flow and branches | Expected behavior | Status | Evidence / limit |
| --- | --- | --- | --- | --- | --- |
| F036 | User · `/contacts` | View accepted, outgoing and incoming contacts | Show name/email and state-appropriate actions | **UI** | Accepted owner-worker contact displayed |
| F037 | User · `/contacts` | Request a contact by email | Create outgoing request for an eligible other active account | **Code** | Request handler inspected; no new connection created |
| F038 | User · `/contacts` | Request an existing connection | Reject duplicate request without duplicating the connection | **Pass** | A connection or request already exists displayed |
| F039 | User · `/contacts` | Request self, invalid/unknown/inactive email | Reject invalid connection target | **Code** | Server validation inspected |
| F040 | Recipient · `/contacts` | Accept or decline an incoming request | Only recipient accepts; decline removes pending request | **Code** | Controls/handlers inspected; no incoming request changed |
| F041 | Requester · `/contacts` | Cancel an outgoing request | Remove pending connection | **Code** | Control/handler inspected |
| F042 | Connected user · `/contacts` | Remove accepted contact | Remove direct grants and assignments between users; retain independent ownership/leases/history | **Code** | Revocation implementation inspected; accepted fixture connection preserved |
| F043 | Task owner · `/sharing/task/{id}` | Open task sharing | Choose accepted contact and View/Work rights | **UI** | Synthetic task sharing form opened; save disabled without a contact |
| F044 | Property owner · `/sharing/property/{id}` | Open property sharing | View/Edit for details, pricing, tours, tasks, leases and finances; separate lease approval | **UI** | All permission groups inspected |
| F045 | Resource owner · `/sharing/{kind}/{id}` | Select contact and add/change/revoke permissions | Load existing rights; Edit implies View; empty rights revoke | **Code** | Permission editor and server grant rules inspected; grants not submitted |
| F046 | Nonowner · `/sharing/{kind}/{id}` | Attempt to share or broaden someone else's resource | Ownership required even if resource is editable | **UI** | Worker task has no Share action |

### Public stays

Sources: [src/guest_pages.rs:430](/Users/eiston.wei/git/buildry/apps/property-catalog/src/guest_pages.rs:430), [src/guest_pages.rs:626](/Users/eiston.wei/git/buildry/apps/property-catalog/src/guest_pages.rs:626), [src/guest_model.rs:1](/Users/eiston.wei/git/buildry/apps/property-catalog/src/guest_model.rs:1).

| ID | Actor / entry | Flow and branches | Expected behavior | Status | Evidence / limit |
| --- | --- | --- | --- | --- | --- |
| F047 | Visitor · `/stays` | Browse houses and displayed monthly prices | House cards, availability, public names and tour links | **Pass** | Six public house cards displayed |
| F048 | Visitor · `/stays` | Switch Full houses / Rooms | Filter the card type | **Pass** | Switched to Rooms on phone and opened a room |
| F049 | Visitor · `/stays` | Search city, university, landmark or listing text | Debounced place search; text fallback; matching results | **UI** | Search entry used; successful geographic match not asserted |
| F050 | Visitor · `/stays` | Search with no match and clear it | No stays match your search; results return after clearing | **Pass** | Empty state and restored room cards observed |
| F051 | Visitor · `/stays` | Open/close mobile search | Where, rental dates and type in a sheet; Show stays applies view | **Pass** | Opened, changed filters, closed and reopened |
| F052 | Visitor · `Rental calendar` | Choose January/May/September start and an eligible end | Four-month blocks; blocked starts/end ranges disabled | **UI** | Rental calendar and disabled month choices inspected |
| F053 | Visitor · `Rental calendar` | Choose 4/8/12/24/36 months and navigate years | Availability-aware choices; current year through nine years ahead | **Pass** | Changed 4 to 8 months; room URL and details preserved selection |
| F054 | Visitor · `Public listing/detail URLs` | Open a requested period with no opening | Keep the requested period; show unavailable dates until the visitor chooses another period | **Pass** | Fixed Nov 10: house 1 and room 9001 retain Jan–Dec 2030, hide booking, and show unavailable. Explicit Jan 2031 / 12-month selection updates links and URL and survives reload. |
| F055 | Visitor · `/stays/house/{id}` | Open house details | Gallery, rooms, amenities, prices, seasonal availability and tour action | **Pass** | House 1 details and its room links inspected |
| F056 | Visitor · `/stays/room/{property}/{room}` | Open room/suite details and related rooms | Bathroom/amenities, selected-period price, house and sibling links | **Pass** | Garden room, house breadcrumb and related room links displayed |
| F057 | Visitor · `Gallery` | Open photo, view gallery and return | Full-view gallery preserves underlying listing | **Pass** | Public room gallery opened and closed |
| F058 | Visitor · `Listing/detail → /book` | Start tour booking from a listing | Property/room/rental period and return destination carried forward | **Pass** | Nov 10: room 9001 → Book a tour preserved property, room, Jan 2031 and 12 months; completed synthetic booking and checked confirmation. |
| F059 | Visitor · `Occupied/unknown listing` | Inspect unavailable period or unknown occupancy | Distinguish availability and prevent invalid booking | **Code** | Occupancy and card rules inspected; exhaustive occupancy combinations not executed |
| F060 | Visitor · `Room fallback /stays/room/3/0` | Open listing for a house with no configured rentable spaces | Omit unconfigured rental offers; stale synthetic-room links show a missing state with recovery | **Pass** | Fixed Nov 10: removed invented room 0 and its house offer; stale /stays/room/3/0 shows Stay not found. Tour house selector offers only configured houses and recovers to a selectable house. |
| F061 | Visitor · `Contact us` | Open/close contact options | Contact dialog with available contact channels | **Pass** | Done Nov 11: shared contact dialog on listing, room and house pages links Email us to the address published in the privacy page. Tour link preserves room/dates/return URL; occupied stays offer Explore stays. Close, Escape, focus restoration and keyboard wrap verified. No email sent. |
| F062 | Visitor/user · `Guest navigation` | Sign in as visitor; Account when authenticated | Navigation follows current session | **UI** | Signed-out Sign in observed; authenticated behavior traced in GuestAccountLink |
| F063 | Visitor · `/privacy; /photo-id-terms` | Read privacy and ID notice and return | Public notices accessible without sign-in | **Pass** | Both pages loaded |
| F064 | Visitor · `Public listing/detail routes` | Missing house/room or failed listings API | Show missing/error state and recovery navigation | **Code** | Detail and listings error branches inspected |

### Tours

Sources: [src/tour_pages.rs:359](/Users/eiston.wei/git/buildry/apps/property-catalog/src/tour_pages.rs:359), [src/tour_pages.rs:599](/Users/eiston.wei/git/buildry/apps/property-catalog/src/tour_pages.rs:599), [src/tour_pages.rs:807](/Users/eiston.wei/git/buildry/apps/property-catalog/src/tour_pages.rs:807), [server/src/tours.rs:1](/Users/eiston.wei/git/buildry/apps/property-catalog/server/src/tours.rs:1), [src/tour_model.rs:3](/Users/eiston.wei/git/buildry/apps/property-catalog/src/tour_model.rs:3).

| ID | Actor / entry | Flow and branches | Expected behavior | Status | Evidence / limit |
| --- | --- | --- | --- | --- | --- |
| F065 | Visitor · `/book` | Open generic booking form | Choose house, rentable room, rental period, date, time and legal/contact details | **Pass** | Generic form used successfully |
| F066 | Visitor · `/book?property_id=...&room_id=...` | Open prefilled booking and change choices | Validate house and room against public availability | **Pass** | Nov 10: prefilled room 9001 booked successfully; unconfigured property 3 resets to Choose a house, and choosing house 1 enables the room selector. |
| F067 | Visitor · `/book` | Choose tour date | Toronto date limits today through 60 days ahead; past starts unavailable | **UI** | Date picker showed disabled past dates |
| F068 | Visitor · `/book` | Choose start time | Hourly starts 10 AM–6 PM, two-hour windows ending by 8 PM | **Pass** | Nine times displayed; 10 AM selected |
| F069 | Visitor · `/book` | Book with valid synthetic details | Create booking and show private management link | **Pass** | Flow Review booking created for Nov 12 at 10 AM in the test database |
| F070 | Visitor · `Booking confirmation` | Copy private booking link | Clipboard success/failure feedback; private link remains available | **UI** | Link and Copy booking link control inspected; clipboard not changed |
| F071 | Visitor · `/booking/{token}` | Open private management link | Display guest, property, room, rental period and Toronto schedule | **Pass** | New synthetic management link opened |
| F072 | Token holder · `/booking/{token}` | Reschedule and save | Recheck availability excluding current booking; persist new time | **Pass** | Moved same-day booking from 10 AM to noon; confirmation displayed |
| F073 | Token holder · `/booking/{token}` | Back out of rescheduling | Return to existing booking without saving | **UI** | Back to booking control inspected; no separate backout execution |
| F074 | Token holder · `/booking/{token}` | Start cancellation then Keep booking | Leave booking active and return to details | **Pass** | Keep booking returned to unchanged rescheduled booking |
| F075 | Token holder · `/booking/{token}` | Confirm cancellation and reload | Persist cancellation; retain token confirmation and Book another tour | **Pass** | Cancellation succeeded and remained after reload |
| F076 | Visitor · `/booking/{token}` | Use invalid token | Booking link not found and Book a tour recovery | **Pass** | Invalid token tested |
| F077 | Visitor/token holder · `Booking/rescheduling API` | Select conflicting time on same or another house | Atomic cross-house overlap rejection; refresh slots after conflict | **Code** | Date lock, overlap validation and conflict recovery inspected; no concurrent race executed |
| F078 | Visitor/token holder · `Booking form/API` | Submit invalid name/phone/email, room, rental period or date/time | Reject input; preserve recoverable form state | **Code** | Validation branches inspected; valid submission and UI prerequisites tested |
| F079 | Visitor/token holder · `Disabled tours` | Book or reschedule a disabled property | Reject booking/reschedule; existing bookings still viewable/cancellable | **Code** | House availability checks inspected; switch not changed |
| F080 | Host · `/calendar; /tours` | View weekly host calendar | Authorized property tours and scheduled tasks; Toronto current-time line | **Pass** | Host calendar showed synthetic booking at its new time |
| F081 | Host · `/calendar` | Previous/next week and Today | Navigate weeks and return to current week | **Pass** | Next week advanced Nov 9–15 to Nov 16–22; Today returned |
| F082 | Host · `Tour availability` | Enable/disable house tour booking | Persist per-house availability; update public choices | **UI** | Owner saw one authorized house enabled; switch preserved |
| F083 | Host · `Tour detail dialog` | Open booking details | Guest contacts, stay dates, manager, notice status and actions | **Pass** | Synthetic tour opened with matching details |
| F084 | Host · `Tour detail dialog` | Reschedule booking or cancel with confirmation | Apply same availability/cancellation rules as private flow | **UI** | Host action controls inspected; guest actions exercised instead |
| F085 | Host · `Tour detail → /leases/new?tour=...` | Create a lease from tour history | Guest/property/room/date/duration prefilled; complete template wizard | **UI** | Create lease link inspected; generic lease wizard walked separately |
| F086 | Host · `Manager notice` | Receive booking email and retry failed delivery | Booking survives mail failure; background worker retries pending notices | **Blocked** | Tour detail showed Email pending / Email is not configured |
| F087 | Phone host · `/calendar` | Use agenda date navigation and Today | Contained agenda scroll and day selection | **UI** | Phone calendar/agenda and date strip inspected |
| F088 | Phone host · `/calendar` | Search tours and clear/close search | Matching tours or explicit no-results state | **Pass** | Synthetic tour matched; unmatched search showed No tours match that search |
| F089 | Phone host · `/calendar?availability=1` | Open mobile availability settings | Accessible house toggles and return to agenda | **Code** | Route/query state inspected; mobile settings link not opened |

### Properties and rooms

Sources: [src/property_page.rs:238](/Users/eiston.wei/git/buildry/apps/property-catalog/src/property_page.rs:238), [src/photo_manager.rs:57](/Users/eiston.wei/git/buildry/apps/property-catalog/src/photo_manager.rs:57), [src/address_picker.rs:1](/Users/eiston.wei/git/buildry/apps/property-catalog/src/address_picker.rs:1), [server/src/main.rs:1071](/Users/eiston.wei/git/buildry/apps/property-catalog/server/src/main.rs:1071).

| ID | Actor / entry | Flow and branches | Expected behavior | Status | Evidence / limit |
| --- | --- | --- | --- | --- | --- |
| F090 | Owner/editor · `/properties` | View authorized property list and map | List only visible resources; location pins for coordinates | **Pass** | Owner saw house 1; admin saw six; tenant saw empty catalog |
| F091 | Owner/editor · `/properties/search` | Search name/address/House ID | Filtered list and empty state | **UI** | Search screen inspected; filter not typed this pass |
| F092 | User · `/properties/new` | Open property creation | Shared name/address/notes/amenities/photos editor | **UI** | New-property form inspected; property not created |
| F093 | User · `/properties/new` | Save valid or invalid new property | Require name/address and space names; server assigns independent ID and ownership | **Code** | Create/save validation inspected |
| F094 | Owner/editor · `/properties/{id}` | Open existing property | General, legal profile/sharing links, gallery and rentable spaces | **Pass** | House 1 management opened |
| F095 | Owner/editor · `Inline fields` | Edit property name, public name or notes | Existing records save committed changes in order; empty/invalid data rejected | **UI** | Inline editor opened without changing stored value; no content saved |
| F096 | Owner/editor · `Address search` | Look up address and return | Suggested addresses plus Use as entered | **Pass** | Three results and manual option appeared; Back to form worked |
| F097 | Owner/editor · `Address search` | Select suggested or manual address | Set address/coordinates for suggestion; manual entry clears pin; autosave existing property | **Code** | Selection handlers inspected; address not changed |
| F098 | Owner/editor · `Amenities` | Expand grouped amenities and change selections | Shared group selections update property/room draft | **Code** | Amenity group/editor implementation inspected |
| F099 | Owner/editor · `Add a room` | Persist existing property then open separate new-room editor | New room remains draft until Save room | **Pass** | Nov 11: Add a room on synthetic saved property 7 opened Room 2. Back to property discarded the unsaved room; only the saved Audit room remained. Synthetic property removed afterward. |
| F100 | User · `Unsaved new property → Add a room` | Try adding a room before first property save | Create the property before opening its separate new-room editor | **Pass** | Done Nov 11: new property Add a room validates name/address, creates property 7, and opens its new-room editor. Saved room 1 and reloaded successfully. Unsaved properties no longer expose sharing links. Synthetic property removed afterward. |
| F101 | Owner/editor · `/properties/{id}/rooms/{room}` | Open bedroom or suite editor | Name/public name, extra details, amenities, photos and base rent | **Pass** | Existing garden room opened |
| F102 | Owner/editor · `Room extra details` | Expand bathroom/notes and open name editor | Bedroom bathroom choice; suite omits bedroom-only field | **Pass** | More room details and Edit Name opened |
| F103 | Owner/editor · `Room editor` | Save new room or change existing room details | Validate named spaces; new room navigates to saved URL; existing fields autosave | **Pass** | Nov 11: named and saved a new synthetic room through the UI; URL changed from /rooms/new to /rooms/1 and Audit room survived reload. Existing-room autosave variants were not separately exercised in this pass. |
| F104 | Owner/editor · `Remove room` | Open confirmation and cancel | Keep room unchanged | **Pass** | Remove dialog opened and Cancel preserved room |
| F105 | Owner/editor · `Remove room` | Confirm removal | Save room list without selected room; clear child parent references | **Code** | Not submitted |
| F106 | Owner/editor · `Gallery` | Open and return from property photos | Shared full-view gallery and cover marker | **Pass** | Two-photo house gallery opened and closed |
| F107 | Owner/editor · `Gallery upload` | Upload JPEG/PNG/WebP within 5 MB; multiple files and retry failures | Per-file progress/results; retry failed files; explicit property save persists gallery | **Code** | Upload component and handlers inspected; files not uploaded |
| F108 | Owner/editor · `Gallery editing` | Reorder cover, remove photo, Undo and Save | Draft changes; Undo restores most recent removed photo; first image is cover | **Code** | Gallery implementation inspected; saved image lists preserved |
| F109 | Viewer/unrelated user · `Property/room routes` | Read-only, inaccessible or missing resource | Server resource filtering and read-only controls/missing state | **Code** | Capabilities and property access API traced; broad permission matrix not mutated |

### Pricing and vacancy

Sources: [src/pricing_page.rs:56](/Users/eiston.wei/git/buildry/apps/property-catalog/src/pricing_page.rs:56), [src/vacancy_page.rs:112](/Users/eiston.wei/git/buildry/apps/property-catalog/src/vacancy_page.rs:112), [src/model.rs:1](/Users/eiston.wei/git/buildry/apps/property-catalog/src/model.rs:1).

| ID | Actor / entry | Flow and branches | Expected behavior | Status | Evidence / limit |
| --- | --- | --- | --- | --- | --- |
| F110 | Pricing reader/editor · `/pricing` | View room monthly/base prices | 18-month desktop grid; six-month phone tiles | **Pass** | Desktop owner and phone admin layouts inspected |
| F111 | Pricing user · `/pricing` | Previous/next six months and Today | Move calendar range and restore current month | **Pass** | Phone Next six months changed Nov–Apr to May–Oct |
| F112 | Pricing editor · `/pricing` | Edit base monthly rent | Positive integer up to 100000; dirty house tracking; invalid base disables save | **UI** | Base field and disabled clean Save prices inspected |
| F113 | Pricing editor · `Monthly cell` | Open monthly override editor | Selected room/month, inherited price and period total | **Pass** | November 2026 editor showed C$975 and total |
| F114 | Pricing editor · `Monthly editor` | Choose Through, price and Apply price | Apply inclusive monthly overrides to draft; total/difference updated | **UI** | Range selector, price and Apply control inspected; no price applied |
| F115 | Pricing editor · `Monthly editor` | Clear existing override and close editor | Revert selected range to inherited pricing; close without draft changes | **UI** | Editor close worked; reset branch inspected in code |
| F116 | Pricing editor · `Save prices` | Persist dirty properties independently; retry remaining failures | Saved properties clear dirty state; failure preserves unsaved houses | **Code** | No pricing saved |
| F117 | Pricing reader · `/pricing` | Attempt edit without pricing.write | Editable controls disabled; pricing reads remain available | **Code** | Capability guards inspected |
| F118 | Vacancy reader · `/vacancy` | View vacant, occupied and unknown periods | Room/suite grid; signed/confirmed leases occupy periods; unknown stays distinct | **Pass** | Vacant periods and 2030 confirmed entire-house span observed |
| F119 | Vacancy reader · `/vacancy` | Inspect entire-house occupancy | Same lease span covers every rentable room | **Pass** | Confirmed 2030 span appeared across all three house-1 spaces |
| F120 | Lease editor · `Vacant period` | Start lease from a vacant month | property/room/start query retained in wizard | **Pass** | January 2027 link opened wizard with property=1, room=9001 and start date |
| F121 | Lease reader · `Occupied period` | Open occupancy lease | Navigate to exact lease details | **UI** | Occupied links inspected; lease opened separately from register |
| F122 | Phone user · `/vacancy` | Navigate six-month availability tiles | Labelled room/month status; eligible vacant/occupied actions | **UI** | Phone tiles inspected; month navigation implementation reviewed |

### Tasks

Sources: [src/housework_page.rs:198](/Users/eiston.wei/git/buildry/apps/property-catalog/src/housework_page.rs:198), [server/src/housework.rs:138](/Users/eiston.wei/git/buildry/apps/property-catalog/server/src/housework.rs:138).

| ID | Actor / entry | Flow and branches | Expected behavior | Status | Evidence / limit |
| --- | --- | --- | --- | --- | --- |
| F123 | User · `/tasks` | Browse visible tasks | Only owned/shared/assigned tasks; state and schedule summary | **Pass** | Owner list and restricted worker list observed |
| F124 | User · `/tasks; /tasks/search` | Search text, house, person or task ID | Matching list and no-results state; clear filters | **Pass** | No-match text displayed No matching tasks |
| F125 | User · `Task filters` | Filter status, one/multiple houses, or assignee | Combine filters; Clear resets them | **UI** | Status/Houses/Assignee controls inspected; combinations not selected |
| F126 | User · `/tasks/new` | Create standalone or property task | Title required; optional notes/property/contact/photos | **Pass** | November 11: Create task still creates a planned task at step 2; synthetic task 9 verified and removed. |
| F127 | User · `/tasks/new` | Create a task directly in backlog | Save an unscheduled backlog task without advancing | **Pass** | November 11: Save to backlog created tasks 8/10/11 at step 1; required-title validation and reload checked; fixtures removed. |
| F128 | Creator · `Task editor` | Assign/reassign accepted contact or leave unassigned | Accepted contacts only; server validates assignment rights | **UI** | Assignee controls inspected; no assignment changed |
| F129 | Task editor · `Existing backlog` | Plan task | Transition backlog → planned; scheduling may be added in planning stage | **Pass** | November 11: task 10 moved from backlog to planned through Plan task, then was scheduled and started. |
| F130 | Task editor · `Planned task` | Start work without complete schedule | Reject with date/time/estimate validation | **Pass** | Set a start date, time and estimate before continuing displayed |
| F131 | Task editor · `Planned task` | Choose date/time/estimate and start work | Transition planned → in_progress with schedule | **Pass** | Synthetic task scheduled Nov 11 at 9 AM for 60 minutes; Work in progress displayed |
| F132 | Task editor · `In-progress task` | Complete work | Transition in_progress → done; show completion state | **Pass** | Synthetic task reached Task complete, step 4 |
| F133 | Task editor · `Existing task` | Save current state without advancing | Persist edits; remain in current state | **Pass** | November 11: task 10 schedule saved as planned, persisted after reload, then Start work advanced separately. |
| F134 | Task editor · `In-progress/done/aborted task` | Change locked core details | Core title/property/description locked; no backward state transition | **UI** | In-progress task switched to read-only core values |
| F135 | Task editor · `Active task` | Abort backlog/planned/in-progress task | Backend supports terminal aborted state and removes schedule | **Pass** | November 11: backlog/planned/in-progress tasks aborted through UI; schedules cleared and state persisted. Cancel/Escape/focus, failure/retry, worker Work and view-only controls checked. Terminal reversal/view-only writes rejected by API. |
| F136 | Task creator · `Task detail` | Delete task and cancel confirmation | Creator-only deletion; existing bills prevent deletion | **UI** | Delete control inspected on own synthetic task; worker had none; final delete not submitted in browser |
| F137 | Worker/viewer · `Shared task` | Work permission versus view-only access | Work permits edits; creator controls sharing/deletion/reassignment | **Pass** | Worker could edit task 5; Assignee disabled and Share/Delete absent |
| F138 | Task editor · `Photos` | Upload/view/remove task photos | Image validation; at most 24 photos; save with task | **Code** | Handlers and validation inspected; no uploads tested |
| F139 | User · `Calendar task event` | Open planned/in-progress scheduled work | Calendar links back to task details | **Code** | Calendar task composition inspected; synthetic task completed before calendar view |
| F140 | User · `/tasks/{id}` | Missing or inaccessible task | Missing/unavailable state with Back to tasks | **Code** | Missing/error branch inspected; not opened with a bad task ID |
| F141 | Property owner · `Task → Bills` | Open task bills or Create bill | Property-owner-only financial action; task/property fixed | **Blocked** | Provider absence prevents task bill loading; direct compose showed unavailable with Retry |

### Landlord profiles

Sources: [src/workspace.rs:309](/Users/eiston.wei/git/buildry/apps/property-catalog/src/workspace.rs:309), [server/src/leasing.rs:404](/Users/eiston.wei/git/buildry/apps/property-catalog/server/src/leasing.rs:404).

| ID | Actor / entry | Flow and branches | Expected behavior | Status | Evidence / limit |
| --- | --- | --- | --- | --- | --- |
| F142 | Owner · `/landlords` | List legal landlord profiles | Profiles are separate from user accounts | **Pass** | Helen Bai and Review Legal Landlord listed |
| F143 | Owner · `/landlords/new` | Open legal profile creation | Legal name, service address, signer email/name/title, authority and properties | **UI** | Creation form inspected; no legal profile submitted |
| F144 | Owner · `/landlords/view?id=...` | Edit legal contact/signer details | Save profile; drafts use current profile while sent leases retain snapshots | **Code** | Editor/save and snapshot rules inspected |
| F145 | Owner · `Profile properties` | Assign/unassign owned properties | Only owned properties; independent resource access remains | **UI** | Property checklist inspected; assignments preserved |
| F146 | Owner · `Profile validation` | Save missing authority/contact or unowned assignments | Required fields and server ownership rules | **Code** | Validation inspected |
| F147 | User · `Missing/inaccessible profile` | Open invalid profile ID | Profile not found/error state | **Code** | Missing state inspected |

### Lease templates

Sources: [src/admin_pages.rs:2122](/Users/eiston.wei/git/buildry/apps/property-catalog/src/admin_pages.rs:2122), [server/src/leasing.rs:859](/Users/eiston.wei/git/buildry/apps/property-catalog/server/src/leasing.rs:859), [server/src/leasing.rs:1108](/Users/eiston.wei/git/buildry/apps/property-catalog/server/src/leasing.rs:1108).

| ID | Actor / entry | Flow and branches | Expected behavior | Status | Evidence / limit |
| --- | --- | --- | --- | --- | --- |
| F148 | Lease editor · `/leases/templates` | Browse templates and open existing one | Separate Templates section with editable template links | **Pass** | Two templates displayed; existing template opened |
| F149 | Lease editor · `/leases/templates/search` | Search templates | Name/landlord filtering and empty state | **Code** | Search implementation inspected |
| F150 | Lease editor · `/leases/templates/new` | Start template creation | Step 1 requires name and landlord | **UI** | Empty template form showed disabled Next step/Continue |
| F151 | Lease editor · `Template wizard step 1` | Choose identity and legal landlord | Filter/select landlord; enable advance when valid | **UI** | Existing populated template identity inspected |
| F152 | Lease editor · `Template wizard step 2` | Edit payment, parking, condo and optional email notices | Required values gate Next; email notices reveal contact fields | **Pass** | Advanced through payment step with fixture values |
| F153 | Lease editor · `Template wizard step 3` | Choose included services and utility payers | Required payer choices gate Next; notes use shared notebook | **Pass** | Services/utilities step opened and advanced |
| F154 | Lease editor · `Template wizard step 4` | Edit deposits, NSF fees, keys, smoking, insurance and extra terms | NSF range 0–20; optional attachment | **Pass** | Final terms step opened |
| F155 | Lease editor · `Additional PDF` | Upload/replace/remove additional pages | PDF <=5 MB and <=30 pages; copied into each lease snapshot | **Code** | Upload/remove validation and template-copy behavior inspected |
| F156 | Lease editor · `Review template` | Open review and return | Display name/landlord/email and Save only / Save and email landlord | **Pass** | Review dialog opened; closed without save |
| F157 | Lease editor · `Save only` | Persist a template without sending mail | Save template and optional PDF; return to Templates | **UI** | Save only control inspected; not submitted |
| F158 | Lease editor · `Save and email landlord` | Save template and email its PDF preview | Deliverable landlord address required; save survives attachment/email failures | **Code** | Current UI calls /additional-pdf then /email as needed; no outbound email requested |
| F159 | Designated template signer · `/api/lease-templates/{id}/send; /lease-template/{token}` | Request private template signature, review PDF and sign | Separate invitation endpoint; name match, consent, preview/final submission | **Code** | Signature-invitation API and token signing exist; current Save and email UI sends PDF instead; neither submitted |
| F160 | Token holder · `Template link/document` | Expired or invalid link; already signed template | Expired recovery or signed completion; protect document by token | **Code** | Template token/signature branches inspected |

### Lease management

Sources: [src/admin_pages.rs:1268](/Users/eiston.wei/git/buildry/apps/property-catalog/src/admin_pages.rs:1268), [server/src/leasing.rs:1263](/Users/eiston.wei/git/buildry/apps/property-catalog/server/src/leasing.rs:1263), [server/src/leasing.rs:2470](/Users/eiston.wei/git/buildry/apps/property-catalog/server/src/leasing.rs:2470), [server/src/photo_id.rs:384](/Users/eiston.wei/git/buildry/apps/property-catalog/server/src/photo_id.rs:384), [server/src/lease_pdf.rs:1](/Users/eiston.wei/git/buildry/apps/property-catalog/server/src/lease_pdf.rs:1).

| ID | Actor / entry | Flow and branches | Expected behavior | Status | Evidence / limit |
| --- | --- | --- | --- | --- | --- |
| F161 | Lease reader · `/leases` | Browse Active/Sent/Signed/Confirmed leases | Active excludes confirmed; filters choose other lifecycle states | **Pass** | Active showed sent lease; Confirmed filter showed confirmed lease 38 |
| F162 | Lease reader · `/leases/search` | Search tenant/email/property/lease ID | All includes confirmed; combine with lifecycle filter | **UI** | Search screen and both sent/confirmed fixtures inspected |
| F163 | Lease editor · `/leases/new` | Create lease through five-step wizard | Template → guest → property → rental unit → stay/rent | **Pass** | Nov 10: completed all five steps and saved manual draft 40 and tour-prospect draft 41 without email configuration. |
| F164 | Lease editor · `Wizard template selection` | Choose matching landlord's template | Only properties assigned to selected template landlord are offered | **Pass** | Sample template led to house 1 selection |
| F165 | Lease editor · `Wizard guest selection` | Choose prospect from tour history | Prefill tour identity, email, room, move-in date and duration | **Pass** | Nov 10: synthetic booked tour prefilled guest, email, phone, property, room, Jan 2031 and 12 months; saved draft 41. |
| F166 | Lease editor · `Wizard guest selection` | Add a new guest manually | Required first/last/email; Use this guest advances | **Pass** | Flow Review guest advanced to property step |
| F167 | Lease editor · `Wizard search` | Search templates, guests and properties; previous step | Filter available choices and keep draft selections | **UI** | Search/Previous controls inspected; no query typed in wizard |
| F168 | Lease editor · `Wizard property/unit` | Select property and bedroom/suite or Entire house | Offer matching authorized units; choose unit-specific rent | **Pass** | House 1 and Garden room selected; Entire house option inspected |
| F169 | Lease editor · `Wizard stay/rent` | Choose move-in, preset/custom duration, email and monthly rent | Duration 1–120 months; valid email/rent; preview contractual dates | **Pass** | Nov 10: missing date, zero rent, incomplete guest@ email and custom 0/121 months disable saving; valid 6-month custom duration enables Save draft. |
| F170 | Lease editor · `Wizard stay/rent` | Choose an overlapping signed/confirmed stay | Block lease creation for overlapping room/entire-house dates | **Pass** | Nov 10: selecting Nov 1, 2030 for room 9001 overlaps confirmed fixture 38; warning displayed and Save draft disabled. Nov 2031 restores eligibility. |
| F171 | Lease editor · `Wizard final action` | Review and email a new lease | Review dialog then explicit confirm/slide; save draft before attempting delivery | **Blocked** | Email unavailable; final Email lease disabled; no review or send triggered |
| F172 | Lease editor · `Wizard with no email provider` | Create a draft without sending | Save a draft independently of email configuration; sending remains separate | **Pass** | Fixed Nov 10: manual draft 40 and tour-prospect draft 41 saved with email unavailable; redirected without delivery-failure query, persisted as draft, and survived reload. |
| F173 | Lease editor · `/leases/view?lease=...` | Review draft/sent/signed/confirmed details | Facts, terms, PDF and status-dependent actions | **Pass** | Confirmed lease 38 facts and 15-page PDF loaded |
| F174 | Lease editor · `Draft → Edit terms` | Edit unsent tenant/home/date/rent/services/deposit terms | Save draft; sent/signed terms not editable | **Pass** | Nov 10: edited synthetic draft 40 rent from C$975 to C$1000; Save draft closes editor, updates details, reloads the 15-page PDF viewer and persists after reload. |
| F175 | Lease editor · `Draft → Change template` | Apply a landlord-compatible template | Replace draft template data; review PDF before send | **Code** | Apply-template handler and compatibility checks inspected |
| F176 | Lease editor · `Draft/Sent → Email tenant` | Send or resend private tenant invitation | Rotate hashed token; preserve snapshot; delivery failure keeps retryable draft/state | **Code** | No outbound invitation requested |
| F177 | Invited tenant · `/my-rentals/{id}/review → /lease/{token}` | Open an authenticated tenant invitation | Matching invited account required; login/signup preserve destination | **Pass** | Tenant's sent lease opened private review page |
| F178 | Invited tenant · `Tenant signing` | Read PDF/details, enter legal name, consent, preview and submit | Tenant signature moves sent → signed; do not alter account identity | **UI** | PDF/signature/consent/Preview controls inspected; no signature submitted |
| F179 | Designated landlord signer · `/leases/{id}/sign → /lease-landlord/{token}` | Countersign with the designated account | Admin/edit access cannot substitute for the designated signer | **Code** | Party and identity checks inspected; no landlord signature submitted |
| F180 | Lease editor · `Signed lease` | Resend landlord signing link | Retry landlord notification while signature is outstanding | **Code** | Control and handler inspected; no email requested |
| F181 | Tenant · `Signed lease → photo ID` | Open/upload passport front or ID card/licence front and back | Consent; file type/size/quality validation; encrypted private storage | **UI** | Nov 11: submitted a synthetic 700 × 700 test pattern through the real tenant passport upload form; private encrypted image opened for the approver. No real identity document used. Licence/card upload variants were not exercised. |
| F182 | Approver · `Signed lease → ID review` | View required sides then approve or request new ID | Separate leases.approve; audited view before approval; rejection removes prior upload | **UI** | Nov 11: approval before opening the synthetic passport image returned the required-review error; View front then Approve ID succeeded and survived reload. Worker/tenant image and decision APIs denied access. Request-new-ID branch remains code-reviewed. |
| F183 | Approver · `Signed lease ID review` | ID review while landlord signature is pending | Review ID independently of landlord signature | **Pass** | Done Nov 11: synthetic signed lease 43 had no landlord signature. ID review appeared; its uploaded test image was viewed and approved in the built-in browser. Reload retained Approved and Awaiting landlord signature; no Confirm tenancy control, and the confirmation API returned 409. Synthetic lease removed afterward. |
| F184 | Approver · `Confirm tenancy` | Require tenant signature, landlord signature and approved ID | signed → confirmed; link existing tenant/signer accounts; generate signed-copy delivery | **Code** | Confirmation handler inspected; no tenancy confirmed in this pass |
| F185 | Approver/editor · `Confirmed lease` | Retry signed-copy email after failed delivery | Confirmation remains; signed-copy delivery retried | **UI** | Done Nov 11 for conflicting messaging: confirmed fixture 38 shows exactly one Signed copy not sent state. Native resend reports Email is not configured, leaves tenancy confirmed, and reload retains the unsent state. A temporary database fixture verified Signed copy sent; removed afterward. Successful provider delivery and the post-send refresh remain unverified without email credentials. |
| F186 | Finance editor · `Signed/confirmed lease → Payments` | Record/edit/remove deposit receipt date | Last-month/key deposit amounts come from signed terms; receipt date <= today | **Code** | Receipt controls/handlers inspected; selected fixture had no deposits |
| F187 | Finance editor · `Lease autopay controls` | Close enrollment or allow voluntary setup again | Stop subscription before covered final period; update enrollment gate | **Code** | Close/reopen handlers inspected; Stripe unavailable |
| F188 | Finance editor · `Record tenancy end` | Open end-date control and back out | Actual end requires valid date; final action stops autopay | **Pass** | Record tenancy end opened; disabled submit; Back restored view |
| F189 | Finance editor · `Actual tenancy end` | End tenancy and stop autopay | Record actual end and stop subscription; retention uses actual end | **Code** | Not submitted; scheduled fixed-term end alone does not stop autopay |
| F190 | Lease reader · `Lease PDF` | Read pages, previous/next and Open PDF | Protected completed Ontario form, schedule and copied attachment | **UI** | 15-page PDF rendered; navigation and Open PDF inspected |

### Tenant rentals and maintenance

Sources: [src/workspace.rs:212](/Users/eiston.wei/git/buildry/apps/property-catalog/src/workspace.rs:212), [src/tenants_page.rs:291](/Users/eiston.wei/git/buildry/apps/property-catalog/src/tenants_page.rs:291), [server/src/tenants.rs:1](/Users/eiston.wei/git/buildry/apps/property-catalog/server/src/tenants.rs:1).

| ID | Actor / entry | Flow and branches | Expected behavior | Status | Evidence / limit |
| --- | --- | --- | --- | --- | --- |
| F191 | Tenant · `/my-rentals; /my-home` | View pending/current/past rentals | Status, unit, dates, rent, legal landlord and next action | **Pass** | Sent and confirmed fixtures displayed |
| F192 | Tenant · `Rental review / ID` | Return to already confirmed lease | Show current signature/ID/confirmation state | **Pass** | Done Nov 11: confirmed fixture 38 shows Tenancy confirmed, My Rentals and a Review lease rental action. No landlord-next or identity-upload prompt; reload persisted, and direct /photo-id returned to confirmed review. Screenshot checked at 390 pixels. |
| F193 | Tenant · `Rental → Lease document` | Open signed private document | Tenant-only document access after signing | **UI** | Protected document link inspected; PDF reviewed through management page |
| F194 | Tenant · `Rental → Payments/Maintenance` | Open related payment and maintenance apps | Confirmed rental exposes payment/history/request links | **UI** | All links inspected; maintenance entered |
| F195 | Tenant · `/my-home/requests` | View own maintenance requests | Empty state or own request list with lifecycle state | **Pass** | Initial empty list inspected; synthetic request created later |
| F196 | Tenant · `/my-home/requests/new` | Open from a rental with property preselection | Required title/description; optional photos | **UI** | Form inspected; plain Requests entry used for submission |
| F197 | Tenant · `Maintenance submission` | Submit title without description | Show concise description validation | **Pass** | Add a description of the issue displayed |
| F198 | Tenant · `Description notebook` | Open, edit, save and return | Shared notebook commits description to form draft | **Pass** | Synthetic description saved to form |
| F199 | Tenant · `Maintenance submission` | Submit valid synthetic request | Create linked backlog task; redirect to request details | **Pass** | Request 7 created and details showed Submitted and correct property/description |
| F200 | Tenant · `/my-home/requests/search` | Search own request text/property/description | Filtered list and no-results state | **UI** | Search page displayed new request; search query not typed |
| F201 | Tenant · `/my-home/requests/{id}` | Read request details and work progress | Read-only status, schedule, description and photos | **Pass** | Submitted synthetic request detail inspected |
| F202 | Tenant · `Maintenance photos` | Upload/view image and submit with request | Shared validation and private linked task images | **Code** | No file uploaded |
| F203 | Tenant/unrelated user · `Maintenance/private rental routes` | No tenancy, ended tenancy, inaccessible/missing request or PDF | Server verifies own linked resources; show error/missing state | **Code** | Ownership/participation handlers inspected |

### Payments

Sources: [src/payments.rs:104](/Users/eiston.wei/git/buildry/apps/property-catalog/src/payments.rs:104), [src/tenants_page.rs:81](/Users/eiston.wei/git/buildry/apps/property-catalog/src/tenants_page.rs:81), [src/payout_setup.rs:1](/Users/eiston.wei/git/buildry/apps/property-catalog/src/payout_setup.rs:1), [server/src/payments.rs:1](/Users/eiston.wei/git/buildry/apps/property-catalog/server/src/payments.rs:1), [server/src/billing.rs:628](/Users/eiston.wei/git/buildry/apps/property-catalog/server/src/billing.rs:628), [server/src/invoices.rs:1](/Users/eiston.wei/git/buildry/apps/property-catalog/server/src/invoices.rs:1).

| ID | Actor / entry | Flow and branches | Expected behavior | Status | Evidence / limit |
| --- | --- | --- | --- | --- | --- |
| F204 | Participant · `/payments` | Open account activity | Group paid/recorded/processing/attention activity by day and direction | **Blocked** | Provider absent; Some payments could not be loaded with Retry |
| F205 | Participant · `/payments` | Retry unavailable activity | Reload reconciliation without claiming an empty ledger | **UI** | Retry link inspected; retry outcome not asserted |
| F206 | Participant · `/payments/bills` | Browse outstanding/paid/closed bills | Group persisted bills by status; link exact bill | **UI** | Bills page opened with empty local results |
| F207 | Participant · `/payments/bills` | Handle partial/unavailable bill loading | Explain incomplete data rather than definite empty state | **Pass** | November 11: native unavailable response warns with Bills Retry and no false empty state. Controlled partial/populated/empty responses retain rows and show the correct warning/empty state; screenshots inspected. |
| F208 | Participant · `/payments/bills/view?id=...` | Open unknown or inaccessible bill | Bill not found | **Pass** | Missing bill ID displayed Bill not found |
| F209 | Participant · `Existing bill detail` | Review invoice, recipient, property, issue/due dates and source task/rent | Private invoice download and source navigation | **Code** | No payable/paid bill fixture available on this server |
| F210 | Property owner · `Task → Bills → Create bill` | Compose recipient email, CAD amount and due date | Task/property fixed; replay-safe creation; immutable parties/amount | **Blocked** | Direct task compose displayed Payments are temporarily unavailable with Retry |
| F211 | Property owner · `Task bill validation` | General/no-property/aborted task, unauthorized owner, duplicate key or invalid amount | Reject ineligible bill creation; reuse idempotent valid request | **Code** | Task bill server validation inspected |
| F212 | Bill payer · `Bill → Pay bill` | Create/reuse checkout and enter secure card | Provider account capability check; persist checkout session | **Code** | Stripe not configured; no checkout or payment submitted |
| F213 | Bill payer · `Checkout result` | Handle success, processing, failure, cancellation and reload | Reconcile persistent provider state; avoid duplicate charges | **Code** | Checkout handlers and browser integration inspected |
| F214 | Bill payer · `Requested bill → Cancel bill` | Confirm cancellation or Keep bill | Cancel only eligible pending request | **Code** | Bill control branches inspected; no bill changed |
| F215 | Participant · `Download invoice` | Download private PDF for unpaid/paid bill | Authorized invoice reflects immutable bill data | **Code** | Invoice generator/authorization inspected; no bill downloaded |
| F216 | Participant · `/payments/settings` | Open payment settings | Bank account, applicable Autopay and rent schedules | **Pass** | Owner settings and one rent schedule displayed |
| F217 | Participant · `/payments/settings/payouts` | Open bank setup | Show connection state or provider-unavailable error | **Blocked** | Payments are temporarily unavailable displayed |
| F218 | Participant · `Bank account form` | Set up individual/business account or update bank details | Tokenize institution/transit/account via Stripe; require agreement as needed | **Code** | No banking data entered; provider unavailable |
| F219 | Participant · `/payments/settings/verification` | Open required verification | Identity/company/representative/owners/directors/executives and documents as required | **Blocked** | Provider unavailable; no sensitive information entered |
| F220 | Participant · `Verification form` | Add/remove people, save requirements, pending review or hosted verification | Refresh outstanding Stripe requirements after each save | **Code** | Dynamic fields and fallback inspected |
| F221 | Participant · `Bank account` | Enable Canadian bank payments | Request PAD capability and complete additional requirements | **Code** | Enable bank payments branch inspected |
| F222 | Legacy connected participant · `Stripe-managed account` | Open Stripe dashboard/hosted verification and return/refresh | Retain legacy account model and reconcile connection status | **Code** | No external Stripe navigation performed |
| F223 | Participant · `/payments/settings/billing` | Review incoming/outgoing rent schedules | Monthly CAD amount, parties, dates; outgoing schedule links to autopay | **Pass** | Owner schedule showed house 1 at CAD950/month |
| F224 | Tenant · `/payments/settings/autopay` | Open own confirmed rental payment settings | Eligible lease billing or clear unavailable/no-schedule state | **UI** | Owner no-schedule state inspected; tenant page entered; enrollment not exercised |
| F225 | Tenant · `Autopay eligibility` | Unsupported due day, ended/closed lease, irregular rent or recipient not ready | No enrollment; clear eligibility reason | **Code** | Billing eligibility checks inspected; due day limited to 1–28 |
| F226 | Tenant · `Autopay setup → Card` | Choose card and authorize recurring monthly rent | CAD regular monthly rent; $0 setup; connected landlord account | **Code** | Stripe absent; no mandate or subscription created |
| F227 | Tenant · `Autopay setup → Bank debit PAD` | Enter bank data and authorize PAD | Scoped SetupIntent/mandate; optional microdeposit verification | **Code** | Provider integration and PAD form inspected |
| F228 | Tenant · `PAD verification` | Enter two microdeposit amounts and retry failures | Verified method activates billing; processing differs from paid | **Code** | Verification branch inspected |
| F229 | Tenant · `Pending autopay` | Continue setup or cancel setup | Reuse pending setup; explicit cancellation confirmation | **Code** | Pending/verifying branches inspected |
| F230 | Tenant · `Active autopay` | Turn off and confirm, or back out | Stop recurring subscription; preserve history | **Code** | No subscription changed |
| F231 | Tenant · `Payment method & invoices` | Open provider portal | Account-scoped payment methods and invoice history | **Code** | Portal action inspected; no external portal opened |
| F232 | Tenant · `Lease deposits in payment settings` | View amounts and recorded receipt dates | Rent/key deposit receipts tracked separately from Stripe collection | **Code** | Deposit summary component inspected |

### Administration

Sources: [src/admin_pages.rs:158](/Users/eiston.wei/git/buildry/apps/property-catalog/src/admin_pages.rs:158), [server/src/leasing.rs:585](/Users/eiston.wei/git/buildry/apps/property-catalog/server/src/leasing.rs:585).

| ID | Actor / entry | Flow and branches | Expected behavior | Status | Evidence / limit |
| --- | --- | --- | --- | --- | --- |
| F233 | Admin · `/access` | Open people directory | Users/Admins groups and active/setup-needed state | **Pass** | Admin directory displayed fixture accounts |
| F234 | Admin · `/access` | Filter All, Users or Admins | Only matching account types shown | **Pass** | Admins filter removed Users group |
| F235 | Admin · `/access/search` | Search name/email/phone | Filtered directory and no-results state | **Code** | Search route and implementation inspected |
| F236 | Admin · `/access/new` | Open Add person | Shared photo/name/email/phone/role controls; Save disabled until valid | **UI** | Empty form and disabled Save person inspected |
| F237 | Admin · `Create/edit person` | Save account profile and User/Admin role | Server validates email/role and protects required admin invariants | **Code** | No account saved or role changed |
| F238 | Admin · `/access/view?user=...` | Review account sign-in state | Password/Google status; setup/reset invitation action | **UI** | Worker detail showed password set, Google not connected and reset-email action |
| F239 | Admin · `Account activation` | Activate/deactivate an account | Backend accepts active field and denies inactive sign-in | **Pass** | Done Nov 11: shared Account status selector saved Inactive and Active on synthetic account 84; reload preserved state. Browser sign-in was denied while inactive, and the same credentials succeeded through the native API after restoration. Reviewed desktop and 390/320-pixel selectors. |
| F240 | Admin · `Password invitation` | Send password setup/reset to saved active account | Deliverable saved address; setup seven days/reset one hour; two-minute throttle | **Code** | No password email requested |
| F241 | Admin · `Person photo` | Upload/remove profile photo and save | Shared file validation and persisted account photo | **Code** | No file uploaded or photo changed |
| F242 | Ordinary user · `/access` | Attempt Administration directly | Redirect away; admin API returns forbidden | **Pass** | Tenant direct navigation returned to My Rentals |
| F243 | Admin · `Unknown person` | Open invalid user ID | Person not found and return to people | **Code** | Missing branch inspected |

### MCP and OAuth

Sources: [server/src/mcp.rs:92](/Users/eiston.wei/git/buildry/apps/property-catalog/server/src/mcp.rs:92), [server/src/mcp.rs:518](/Users/eiston.wei/git/buildry/apps/property-catalog/server/src/mcp.rs:518).

| ID | Actor / entry | Flow and branches | Expected behavior | Status | Evidence / limit |
| --- | --- | --- | --- | --- | --- |
| F244 | User · `Copy Buildry MCP instructions` | Copy connector prompt from Properties header | Clipboard contains remote endpoint/scopes and authentication steps | **UI** | Copy action inspected; clipboard not modified |
| F245 | Client/user · `OAuth discovery and registration` | Discover metadata and register public client | Validated redirect/client metadata; OAuth 2.1 public client | **Code** | mcp.rs routes and registration inspected |
| F246 | User · `/oauth/authorize` | Sign in and review requested scopes | Consent shows listings.read/tasks.read/tasks.write permissions | **Code** | No OAuth client/consent session created |
| F247 | User · `OAuth consent` | Allow or deny, including expired/invalid pending request | PKCE S256 authorization code or denial; limited pending lifetime | **Code** | No new connector access granted |
| F248 | Client · `/oauth/token` | Exchange code and rotate refresh token | One-hour access; 30-day rotating refresh; reject replay/invalid code | **Code** | Token handlers inspected |
| F249 | Authorized client · `read_guest_listings / get_listing_image` | Read public listing data/images | Require listings.read; image must belong to public listing | **Code** | Tool definitions and access checks inspected |
| F250 | Authorized client · `list_tasks / get_task / list_task_options` | Read allowed tasks and creation options | Resource permissions plus scoped token; accepted contact options | **Code** | Tool definitions and scope checks inspected |
| F251 | Authorized client · `create_task` | Create standalone/property task with assignee | tasks.write plus property/assignment access; explicit client operation | **Code** | Tool mutation handler inspected; no token issued |
| F252 | Client · `Expired/revoked/inactive account` | Use an invalid token or inactive user token | 401/challenge; active account check on every authorization | **Code** | Authorization query inspected |

### Shared controls and recovery

Sources: [src/shared_ui.rs:1](/Users/eiston.wei/git/buildry/apps/property-catalog/src/shared_ui.rs:1), [src/notebook_textarea.rs:1](/Users/eiston.wei/git/buildry/apps/property-catalog/src/notebook_textarea.rs:1), [src/date_picker.rs:1](/Users/eiston.wei/git/buildry/apps/property-catalog/src/date_picker.rs:1), [src/time_picker.rs:1](/Users/eiston.wei/git/buildry/apps/property-catalog/src/time_picker.rs:1), [server/src/main.rs:1587](/Users/eiston.wei/git/buildry/apps/property-catalog/server/src/main.rs:1587).

| ID | Actor / entry | Flow and branches | Expected behavior | Status | Evidence / limit |
| --- | --- | --- | --- | --- | --- |
| F253 | User · `Shared notebook` | Edit long notes/description with save, back, undo and redo | Draft changes stay in editor until saved; return preserves form | **Pass** | Maintenance description opened, filled, saved and submitted |
| F254 | User · `Shared selectors` | Open/select/close dropdowns and disabled options | Labelled button/listbox/options; disable ineligible choices | **Pass** | Tour house/room/time selectors used successfully |
| F255 | User · `Shared date/time pickers` | Choose date, AM/PM/hour/minute, apply/cancel and navigate calendars | Constraints from calling flow; state returns to form | **Pass** | Task date and default 9 AM time committed; tour date chosen |
| F256 | User · `Shared confirmations` | Close/back out of a pending destructive/status action | Do not mutate until confirmation | **Pass** | Room removal and tour cancellation backout verified |
| F257 | User · `Uploads` | Drop/browse, multi-file progress, invalid size/type and retry | Use shared upload component; validation and file-specific feedback | **Code** | Shared dropzone and upload handlers inspected; no file transferred |
| F258 | User · `Loading/empty/error/missing states` | Wait for data, receive no records or fail a request | Status/alert and relevant recovery links | **UI** | Empty tasks/search, missing booking/bill, payment unavailable and expired token paths observed |
| F259 | User · `Browser history / direct reload` | Navigate back/forward or reload a nested route | Route resolves and state is reloaded from database | **Pass** | Nested room, booking and request routes loaded; cancelled booking persisted on reload |
| F260 | User · `Legacy routes` | /availability, /tenants, /housework, old payment/home/landlord paths | Redirect to modern app routes with appropriate query context | **Code** | Server redirects and frontend legacy branches inspected; see legacy mismatch finding |
| F261 | User · `UI revision watcher` | Open app while a new built frontend is deployed | Poll revision and reload when it changes | **Code** | No build/reload revision change triggered |

## Remaining execution coverage

The Code/UI rows are the remaining checklist, not implied successes. Full execution would require a local email-capture setup, a configured Stripe test-mode account and fixtures for each billing state, controlled signed/unsigned ID fixtures, and the requested final-action authorization for signatures or credential changes. A separate browser pass should cover file selection/drop/oversize/mixed-batch retries, real view-only versus work/edit grants, concurrent tour/lease races, expired/replayed invitations and tokens, and every listed terminal payment state. Financial and contractual final actions were not performed.

The baseline audit made no application fixes. The November 10 and November 11 remediation passes above updated the application and verified the findings marked Done; the remaining findings are still open.
