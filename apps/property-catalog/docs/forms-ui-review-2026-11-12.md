# Forms and shared controls — November 12, 2026

Audited the active Dioxus page modules, the shared form library, and server-rendered authentication, OAuth, signature, and identity forms. The retained, inactive `occupancy_page.rs` is outside the active module tree.

## Changes

- Migrated the remaining page-level native buttons to `FormButton`, retaining their existing classes, handlers, submit behavior, and accessible labels. All active page input, select, textarea, button, and form roots now come from shared components. Native elements inside the shared picker/editor/upload implementations remain implementation details of those components.
- Moved the lease-template toggle into the shared form library. `FormButton` now supports switch role and checked state.
- Added server-rendered shared control adapters for authentication, recovery, password setup, OAuth actions, and the three signature flows. Kept native submission, validation, autocomplete, field names, consent, signature preview, and hidden-submit hooks. Authentication pages share one control stylesheet. Identity forms retain their existing shared upload/select adapters and now share button/form/checkbox treatment.
- Removed the OAuth sentence that duplicated the application identity card.
- Fixed nested month/year menus in the shared date picker. On phones, the form-card bottom-sheet rule was positioning a month menu outside the transformed date-picker panel, leaving its first choices behind the heading. The nested menu now fills the picker surface and scrolls there.

## Browser validation

Used a native API on `127.0.0.1:8093` with the existing `buildry_unified_test` database and freshly built release assets. The working catalog, PostgreSQL container, image, and volume were preserved.

The main sweep captured 42 routes at 1440, 390, and 320 pixels (126 captures). It covered property/room creation and editing, pricing, vacancy, calendar, task forms/search/details, leases/templates, landlord profiles, resource sharing, apps, contacts, profile/security/preferences, rentals, maintenance, payments, public stays, booking, and recovery states. The authenticated login/signup URLs redirect to Properties; separate anonymous captures cover their actual public states.

Additional coverage:

- 33 captures of anonymous authentication/recovery, real administrator list/search/new-person, fresh-user onboarding, and tenant rentals/maintenance at all three widths.
- Real administrator person editing and tenant private lease signing at all three widths. Signature UI was inspected without signing.
- Bank setup and identity-verification forms at all three widths using controlled setup responses. These are UI fixtures, not successful Stripe verification.
- OAuth consent at all three widths using an isolated local test client. No consent was granted. The temporary client and pending requests were removed.
- Required task-title validation; notebook editing; keyboard opening/dismissal of the shared selector; real task creation, reload, and persistence. Both synthetic tasks created while developing the browser check were removed, confirmed against the test database.
- All four template steps, landlord selection, toggles, utility selectors, and review dialog. No template was saved or emailed.
- All five lease steps using a manual guest, property/room selection, month/year/date selection, and custom/standard durations. No lease was saved or emailed.
- Booking date and start-time availability menus. The nested month/year regression was rechecked at all three widths, selecting January 5, 2027 through the actual picker.

The main sweep found no document overflow, unlabelled visible native inputs, page-owned raw inputs, unshared form roots, or browser page errors. Supplementary role, private, fixture, wizard, and date checks also found no document overflow. Screenshots and contact sheets were visually inspected.

## Checks and limits

- `cargo test --workspace`: 14 passed.
- `cargo test --manifest-path server/Cargo.toml`: 45 passed, 3 ignored database/provider tests.
- New adapter regressions check authentication submission names/validation, signature-name escaping, and preview/consent/hidden-submit contracts; both pass.
- `cargo check --target wasm32-unknown-unknown`, `dx build --web --release`, and `git diff --check` pass.

Outbound mail and Stripe credentials are absent. Signup's unavailable state, recovery forms, payment recovery states, and controlled bank/verification forms were reviewed; live signup/email delivery, payment/PAD submission, bank verification, and real identity uploads were not completed. Valid password-setup and manual photo-ID submission are covered by source review/adapter checks rather than a live token submission. No legal signature was submitted. This is broad UI and selected end-to-end coverage, not every conditional business workflow.

Screenshots, route/journey scripts, and JSON measurements are local under the ignored `ui-review/2026-11-12-forms/` directory. Representative evidence: `date-menu-fixed-320.png`, `date-applied-320.png`, `lease-step-5-custom.png`, `template-review.png`, `fixture-bank-320.png`, `fixture-verification-320.png`, and `detail-admin-390.png`.
