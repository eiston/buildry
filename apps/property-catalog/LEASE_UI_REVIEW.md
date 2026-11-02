# Lease UI review

Reviewed locally at desktop 1440 × 1000 and mobile 390 × 844.

## Changes

- Compact lease and template wizards, consistent spacing, and fewer repeated labels.
- Two-column stay details and template fields on desktop; single-column mobile forms.
- Distinct lease status badges and responsive register rows.
- Inactive lease/template panels remain available for swipe transitions but are excluded from focus and accessibility navigation.
- Lease details show the property, unit, rent, and move-in date immediately. Template replacement is tucked into a disclosure.
- Full terms editor has semantic field groups and a sticky close control.
- Reusable upload dropzone supports template PDFs, filename display, reselection, clearing, size validation, and PDF signature validation.
- Shared PDF reader has page controls, page count, keyboard navigation, and reduced-motion support. Existing lazy page rendering remains.
- Missing lease IDs no longer fall back to another tenant's lease.
- Tenant, landlord, template, and ID links share a styled expired-link state.
- Identity upload uses the same global dropzone as gallery and template uploads. Privacy and consent text remain.

## Coverage

| Area | Reviewed |
| --- | --- |
| Lease register | Active, sent, signed, confirmed filters; desktop and mobile |
| Lease search | Results and no results; desktop and mobile |
| Create lease | All five steps; manual guest, property/unit selection, custom duration, back navigation |
| Lease details | Draft, sent, signed, confirmed; terms disclosure and full editor |
| Templates | Register, search, new and edit pages |
| Template editor | All four steps, review dialog, invalid PDF rejection, valid selection and clearing |
| Private views | Tenant signing/completion, landlord signing, template review, identity upload and passport mode |
| PDF reader | Rendering, page controls, keyboard navigation |
| Error views | Missing lease and expired private links |

Validation: production Docker build, healthy local services, 36 passing server tests, JavaScript syntax check, and clean whitespace check. Screenshot and DOM audit artifacts are in the chat's `ui-review/leases` directory. The audit records viewport width and document overflow.

Local fixtures use reserved email addresses. No emails, signatures, identity documents, identity approvals, or tenancy confirmations were submitted. Physical OS file dragging was not simulated; native picker selection and validation were tested. The local email provider is unavailable, so the lease-send confirmation was not submitted.

## Shared uploader follow-up

`public/assets/upload-dropzone.mjs` now owns the common markup and drop handling for both Dioxus and server-rendered forms. The Rust `UploadDropzone` wrapper keeps the existing gallery/template callbacks; ID forms retain their native multipart file inputs. Passport selection clears, disables, and hides the back input. Selected ID filenames appear in the dropzone.

Verified desktop/mobile rendering, ID picker selection, passport switching, two consecutive gallery selections after input replacement, and template PDF selection/clearing. Gallery test additions were discarded without saving the property. No ID form was submitted. All 36 server tests pass. Screenshots: `shared-id-desktop.jpg`, `shared-id-mobile.jpg`, and `shared-id-passport-mobile.jpg`. A clearly named local identity fixture remains for visual review and is cleared by the existing startup migration.

## ID type dropdown follow-up

The ID type field now uses the FormSelect layout, chevron, option states, shared keyboard behavior, and mobile bottom sheet. A reusable server-form adapter in `public/assets/form-select.mjs` retains the native select solely for validation and multipart submission, with a native fallback if JavaScript is unavailable. Verified ArrowDown, End, Enter, Escape, passport/back-upload switching, and desktop/mobile rendering. Screenshots: `id-dropdown-desktop.jpg` and `id-dropdown-mobile.jpg`. All 36 server tests pass; no ID form was submitted.
