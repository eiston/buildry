# Property catalog

## MCP access

Connect an MCP client to `https://app.buildry.ca/mcp`. The server advertises OAuth 2.1 metadata, registers public clients dynamically, and asks the user to approve the requested permissions in a browser. Signed-out users go through the normal Buildry sign-in page first, then return to the consent page. Authorization uses an S256 PKCE code; access tokens last one hour and rotating refresh tokens last 30 days. Deactivating a Buildry account immediately blocks its tokens.

`read_guest_listings` returns the same house and room data as `/api/guest-listings`, including absolute image URLs, amenities, prices, and occupancy. `get_listing_image` returns the image bytes for a numeric image ID referenced by one of those public listings. Both tools are read-only and require the `listings.read` OAuth scope. To add the server to Codex CLI, run `codex mcp add buildry --url https://app.buildry.ca/mcp`, then `codex mcp login buildry` and approve the browser prompt.

Every active user can request `tasks.read` for task queries and `tasks.write` for creation. Resource permissions still apply. Tasks may stand alone; an optional property requires task-edit access. Assignees use accepted contact user IDs from `list_task_options`. Existing listing-only tokens do not gain task scopes automatically.

A small Dioxus 0.7 web app for recording the six houses. Properties, their physical spaces, current rent observations, and map pins are stored in PostgreSQL.

## Accounts, apps, and resource access

Ordinary accounts use `user`; `admin` remains a system privilege. The same account can own properties, rent another home, and work on shared tasks. First-login onboarding asks for Tenant, Property owner, and Worker interests and installs the union of their starter apps. These answers never grant permissions.

`/apps` manages installed sidebar apps. `/account` contains profile/photo/contact details, password and Google sign-in settings, session revocation, interests, and sidebar ordering. Removing an app preserves its data and access; permitted direct links still work. Apps, Contacts, and Account remain available even with no installed apps.

Contacts at `/contacts` require an accepted connection before task/property sharing. Owners share resources using View/Work for tasks and function-specific View/Edit grants for properties. Lease ID review and confirmation have a separate approval permission. Only owners manage sharing. Revoking a connection removes direct grants and assignments between its users, while ownership, lease participation, and payment history remain independent.

Legal landlord profiles at `/landlords` describe the legal party and designated signer; they are separate from user accounts. Property owners manage those profiles and their own property assignments. Resource checks run on the server for lists, mutations, images, documents, and MCP requests.

The catalog at `/properties` shows a stable **House ID** and street address for every property. Select a house to open `/properties/{id}`, a full management page for its details, map pin, spaces, room pricing, and images. Use `/properties/new` to add a house. The ID appears only in the admin interface. `/pricing` provides a separate view of the current and seasonal rent for every bedroom and suite; changes are saved per house. Seasonal rent must decrease from September to January to May.

Property editors can add JPEG, PNG, or WebP images up to 5 MB from the property details gallery, drag them into order, or remove them with the control on each image. The top-left image is the guest cover. Clicking an image opens a full-screen view. Uploaded files are stored in PostgreSQL's `property_images` table and served from `/images/{id}`. On saved properties and rooms, text, amenity, and photo changes save automatically. New properties and rooms require Save.

The 26 rentable rooms have distinct AI-generated illustrative JPEGs in `assets/generated-rooms/`. Each of the six houses also has an illustrative image in its gallery; the first house includes a newly generated living room view. These images are uploaded to `property_images` with an `?illustrative=ai` marker so guest cards and room details label them clearly. Run `cargo run --release --example upload_generated_room_photos` and `cargo run --release --example upload_generated_house_photos` from `server/` to seed a fresh catalog; the examples leave existing galleries intact.

Supplementary, distinct room and shared-space illustrations are in `assets/generated-gallery/`. Each rentable room receives two additional images and each house receives two additional images. Run `cargo run --release --example upload_generated_gallery` from `server/` after the base images are seeded. The command appends images without replacing covers or duplicating previously uploaded gallery assets. All generated views are illustrative and are not photographs of the actual properties.

## House tours

## Tasks

Tasks are created when a person submits the task form, a tenant submits a maintenance request, or an authorized client explicitly calls the task API.

Open `/tours` for the host's weekly calendar with time on the vertical axis. Use the arrows to move between weeks and **Today** to return to the current week. A red line shows the current Toronto time in today's column during calendar hours and refreshes every minute. Select a tour to see its details. The **Open guest page** and **Copy booking link** actions sit together above the calendar. The sidebar can be collapsed to icons.

The host can turn tour booking on or off for each house from the switches above the calendar. The setting is stored in PostgreSQL. Only enabled houses appear on the guest booking form, and the API checks the setting again when checking times, booking, or rescheduling. Existing tours remain visible and can still be cancelled when a house is turned off.

Open `/book` for the guest form. Guests choose a house, date, and start time, then enter a name and phone number. Starts are available each hour from 10 AM through 4 PM in the `America/Toronto` timezone; each booking reserves a two-hour window, ending no later than 6 PM. Dates are available up to 60 days ahead.

After booking, the confirmation shows a unique, unguessable `/booking/{token}` link with a copy button. A guest with that link can view, reschedule, or cancel that booking. Rescheduling follows the same cross-house conflict rules. The link continues to show a cancellation confirmation after cancellation. It is shown to the guest at booking time; the app does not send it by SMS or email.

In the host calendar, select a tour to reschedule its date and start time or cancel it. Rescheduling checks the same cross-house availability as a new booking. Cancellation is confirmed in the popup and stored with a cancellation timestamp; cancelled tours leave the calendar and free their time.

Bookings are stored in `tour_bookings`. PostgreSQL has a partial unique index on `(property_id, tour_date, slot_start)` for real bookings. The API takes a PostgreSQL transaction lock for each date and returns HTTP 409 if a new two-hour window overlaps any real booking, regardless of house. Availability hides overlapping and past starts across all houses. The guest form checks availability again after a conflict.

For older bookings created before email was required, run `cargo run --release -- backfill-missing-tour-emails` from `server/` to fill blank emails with `tour-<id>@example.invalid`. These addresses are explicitly non-deliverable; replace one with a real address before emailing a lease.

The current server binds to `127.0.0.1`, so its URLs are local previews and cannot be opened by guests on other devices. Public deployment needs a reachable host and HTTPS before any guest link is shared.

## Tenants and vacancy

The admin `/vacancy` page shows one continuously scrolling monthly grid for every house, room, and suite. Year bands and numbered month columns keep the grid compact. Signed and confirmed room leases appear once as a bar spanning their occupied months. An **Entire house** lease draws the same occupancy span across every rentable room in that house. Open future months link to the lease creation flow with the house, room, and move-in month preselected. The old `/availability` URL redirects to `/vacancy`.

Tenants sign in with their email and password at `/login` and see their confirmed leases at `/my-home`, where they can change their password. Admin accounts can manage properties, tours, pricing, availability, and tenants. Existing installations migrate the old single admin password to `admin@buildry.local`; use `set-admin` to choose a real admin email and password. Deactivation blocks sign-in without deleting lease history.

## Rent payments and deposits

Tenants can opt in from **Payments → Settings → Autopay** for eligible confirmed Ontario residential leases. Stripe's embedded Payment Element handles the card fields and Stripe Billing charges the lease's regular monthly rent in CAD on the contractual due day. Setup charges $0. The flow excludes deposits, arrears, partial rent, discounts and separate fees. Supported due days are 1–28. Qualifying residential rent is GST/HST-exempt.

To set deposit amounts, open a draft under **Leases → Edit terms → Deposits and terms**. The last month's rent deposit cannot exceed one month's rent. A refundable key deposit covers replacement costs for the keys or access devices. After signing, open **Lease details → Payments** to record each deposit received outside Stripe. The tenant sees the recorded date after confirmation. Buildry tracks receipt dates. Collection, interest, applying the rent deposit, and returning the key deposit are handled separately.

Tenants can turn autopay off or open **Payment method & invoices**. Both roles use **Payments → Account activity**, showing Stripe rent invoices, recorded deposit receipts and task bills. If a deposit covers the final rent period, landlords can **Close autopay** before that period's billing date. This stops the Stripe subscription and blocks tenant re-enrollment. **Allow autopay setup** reopens voluntary enrollment. Recording the actual tenancy end also stops autopay. A fixed term's scheduled end does not stop autopay, since a tenancy can continue month to month.

Configure these variables on the API process:

- `STRIPE_SECRET_KEY` is the server-only key for the Stripe Connect platform. Use a test key locally.
- `STRIPE_PUBLISHABLE_KEY` is the matching platform publishable key for embedded card entry. New sessions use branded hosted Checkout when this is absent.
- `APP_PUBLIC_URL` is Buildry's public HTTPS origin. A loopback HTTP origin is supported with a test key.
- `STRIPE_PORTAL_CONFIGURATION` is optional and applies to older subscriptions created on the platform account. Buildry provisions invoice history and payment method updates separately for each connected account.

Both tenants and landlords use **Payments → Settings → Bank account**. New accounts use Custom Connect and shared Buildry form controls. Known profile details prefill setup, and Buildry supplies its website and payment description. The bank form collects account-holder name, institution, transit and account number; Stripe.js tokenizes bank and identity details directly to Stripe. Missing legal identity, address, date of birth or other Stripe verification is collected separately when required. Existing Stripe-managed accounts retain their account model and hosted verification. Buildry checks the recipient’s payment and payout capabilities before starting checkout.

New subscriptions use direct charges on the Canadian Stripe account linked to the landlord in the signed lease. Each subscription stays in its original Stripe account, including older platform subscriptions. Existing Stripe-managed accounts retain their Stripe Dashboard. Custom account setup is managed inside Buildry; advanced verification can require Stripe’s hosted flow. A landlord who disconnects from Buildry in Stripe must manage any remaining subscriptions from their own Stripe Dashboard. Keep credentials outside the repository and use separate test and live databases.


The shared `/payments` page shows account activity with persistent Bills and Payment settings icon links. Bills have downloadable private PDF invoices in unpaid and paid states. Rent bills originate from lease billing. Property owners create task bills from **Task → Bills → Create bill**, with the task and property fixed by the server. Owners enter recipient email, amount and due date; they pay from the resulting bill. There is no standalone Pay/Receive form or generic bill-creation endpoint. Task bills require property ownership, have immutable amounts and parties, replay-safe creation and persistent checkout sessions. Tasks with bills cannot be deleted or moved to another property. Historical submitted/settled requests remain visible; old unsubmitted standalone requests cannot be paid. Activity is reconciled with Stripe on load; it is not a bank balance or a complete Stripe accounting ledger. Refunds, disputes, and bank payout transactions are not aggregated into this activity view yet.

Rent autopay supports cards and Canadian PAD. PAD uses a subscription SetupIntent and Stripe.js bank authorization with microdeposit verification when required. Processing debits stay distinct from completed payments. Payment methods and mandates are scoped to their connected account; one-time property payments currently accept cards. Production rollout still needs operational webhook/reconciliation handling and the recipient’s live Stripe verification. Task bills use the entered CAD total; they do not calculate additional tax. Existing residential-rent exemption behavior is preserved.

## Signup, access, and leases

Email verification links open a confirmation page before activating a user account, so link previews do not consume them. Expired links can be replaced at `/resend-verification` (two-minute resend limit). `/forgot-password` sends a one-hour password reset link to an active account, limited to one request every two minutes. Signup and password setup require password confirmation.

Signup creates an inactive `user` and sends a 24-hour verification link. `/access` remains an admin-only account directory with User/Admin account types, activation controls, and password invitations. All business access follows resource relationships. Lease confirmation links the existing tenant account and never changes its role, password, or profile.

**Leases → Landlords** manages multiple landlords and their property assignments. Its searchable, scrollable property selector supports larger portfolios and can show only assigned properties. The migration creates a Helen Bai record and assigns existing properties to her; `backfill-helen-contact` fills missing contact fields with non-deliverable example data. An authorized signatory must still be confirmed before emailing any lease. New properties initially use Helen Bai until reassigned. A new tour asks for the guest's email, rental room, move-in date, and duration.

`/leases` is the lease management dashboard. Search or filter drafts, sent leases, signed leases, and confirmed tenancies; select a lease to review its property, room, dates, rent, landlord, PDF, and next action. **Create a lease** opens a two-step flow: choose a potential guest from their tour history, then choose the property, room, move-in date, duration (a preset or a custom number of months), recipient email, and monthly rent. The toured property and known details are preselected; staff can change them before preparing a draft. The lease fills the property landlord and standard terms automatically. Full edit opens only when requested. Drafts reflect landlord edits; sent leases keep their original snapshot. Review the PDF and all standard terms before emailing. The PDF uses the [official Ontario Standard Form of Lease 2229E (2020/12)](https://forms.mgcs.gov.on.ca/en/dataset/047-2229/resource/05677ea2-3173-4c0e-9a14-7a06cbcb41b9), completed with the entered values and an attached schedule. The original government form pages are retained.

Lease templates can include one additional PDF of up to 30 pages and 5 MB. Its pages follow the completed form and schedule in the combined PDF. Each new lease copies the template's PDF so later template changes do not alter a sent lease.

`/leases` is **Lease Management**; `/my-rentals` is the separate tenant app. Creating a lease saves a draft. Sending is a separate action, and a send failure keeps the draft for retry. Email invitations require sign-in or verified signup with the invited address; a contact connection is not required. Safe invitation destinations survive the signup/login flow.

The tenant reviews and signs, and the designated legal-landlord signer countersigns using their own account. Neither admin status nor lease-edit permission substitutes for the designated signer. After the tenant signature, ID upload and landlord signing can proceed independently. ID images are encrypted separately, and each review is audited. An owner or explicit lease approver must view the required sides, approve ID, and confirm the tenancy after both signatures. Confirmation preserves account identity. Failed signed-copy delivery remains retryable without undoing confirmation. My Rentals includes pending, current, and past rentals, lease PDFs, and links to payments and maintenance. Private invitation tokens are stored as hashes and replaced on resend.

Set `PHOTO_ID_ENCRYPTION_KEY` to a stable, randomly generated **base64-encoded 32-byte key** in the API service environment before accepting ID uploads. For example, `openssl rand -base64 32` generates a suitable value. Keep it outside the repository and back it up securely: losing or changing it makes stored ID images unreadable. Without the key, the upload endpoint returns a service-unavailable response and stores nothing. Signing extends the private link for at least seven days so the ID upload can finish. Staff can email a new seven-day ID link from the lease dashboard if it expires.

To use Stripe Identity instead of collecting new ID images in Buildry, set `STRIPE_IDENTITY_ENABLED=1`, `STRIPE_SECRET_KEY`, the matching `STRIPE_PUBLISHABLE_KEY`, and `APP_PUBLIC_URL` on the API process. Enable Identity in the Stripe account first. Use test keys and a loopback origin locally; live mode requires HTTPS. The tenant consents, then Stripe's official Stripe.js SDK opens `verifyIdentity` and collects the document directly. Buildry stores the session reference, consent and outcome, and shows the verified name transiently to an approver. A selfie check is not requested. Existing encrypted ID uploads retain their review flow.

Stripe verification is separate from Buildry approval. An approver refreshes the provider result and compares the verified name with the lease before approving ID. Landlord signing and identity approval can proceed independently; confirmation still requires both signatures and approved ID. The server retrieves Stripe's current result on refresh, approval and confirmation; modal completion is never treated as verification, and no webhook is required for this on-demand reconciliation. Session creation is idempotent and retries reuse the same session. Client secrets are neither persisted nor placed in URLs. Missing Stripe configuration or failed provider lookup blocks Stripe approval and confirmation. When disabled, new uploads retain the existing manual review path.

Stripe session redaction is requested by the daily retention worker one year after the recorded tenancy end, unless the session has a legal hold. Failed redaction is retried on a later run; keep the original Stripe account and matching mode configured for cleanup. Stripe can continue legally required retention under its policy. Configure and test Identity before enabling it for tenants. Local SDK contract tests run with `node --test scripts/test-stripe-identity.mjs`; the ignored `identity_session_reuses_and_reconciles_provider_state` server test uses an explicitly selected `_test` database and a local simulated Stripe API, without sending documents to Stripe. See [Stripe's SDK integration guide](https://docs.stripe.com/identity/verify-identity-documents?platform=web&type=modal).

Staff record the actual tenancy end date in the lease dashboard. The API deletes ID images one year after that recorded date; cleanup runs when the date is saved, on startup, and daily. A lease's scheduled end date does not start the retention clock. Database backups must follow the same secure retention policy.

The server uses the Rust `image` library to fully decode each upload before storage. It rejects unsupported or damaged files, images smaller than 600 pixels on either side, images larger than 6,000 pixels on a side or 20 million pixels total, and nearly blank images. This is image integrity screening, not proof that a document is genuine; staff still inspect and approve each ID before confirming the tenancy.

Email sending requires these server environment variables, kept outside the repository: `RESEND_API_KEY`, `RESEND_FROM_EMAIL` (a sender on a verified Resend domain), and `APP_PUBLIC_URL` (the HTTPS origin recipients can reach). The app refuses to email a lease or create a signup if they are missing. Do not use a localhost URL in email. To populate sample accounts without real email addresses or passwords, run `cargo run --release -- seed-demo-tenants` and `cargo run --release -- seed-demo-guests` from `server/`. These commands can be rerun without duplicating records. Their addresses use the reserved `example.invalid` domain.

The API's `email-smoke-test` command sends a small attachment to Resend's test recipient `delivered@resend.dev` and requires a provider message ID. Pass a real address as the next argument (for example, `email-smoke-test you@example.com`) to check delivery to an inbox. It requires the same environment variables as live email. The server unit must receive the sender and key before running the command. A local-provider test also verifies the request recipient, bearer token, and attachment shape without sending email.

Demo account rows created by the optional seed commands have `users.is_seeded = TRUE`. `backfill-helen-access` creates Helen Bai's linked landlord account with no password or email delivery; an admin can update the address and send setup instructions from Access after email is configured.

## Project structure

```text
client/src/
  main.rs         Browser startup, routing, and app composition
  components/     Shared galleries, dialogs, and UI
    forms/        Form controls, actions, fields, pickers, and uploads
  layout/         Admin shell, sidebar, and mobile navigation
  pages/          Feature pages and page-specific flows
  services/       Browser API clients and map/geocoding access
server/
  src/            API startup, domain handlers, authentication, and integrations
    components/   Shared server-rendered UI
  templates/      Sign-in, signup, OAuth consent, and privacy HTML
  migrations/     Database migrations
  examples/       Data maintenance and image seeding commands
shared/src/       Models and pure helpers used by both client and server
assets/           Build assets, fonts, forms, and generated illustrations
public/           Static files copied by Dioxus
scripts/          Development utilities and integration checks
docs/             Audits and supporting documentation
```

The root `Cargo.toml` defines the browser binary at `client/src/main.rs` and a workspace containing `shared` (`buildry-models`). It stays at the project root so `dx build`, asset URLs, and the existing output paths stay consistent. The API keeps its own manifest, lockfile, and target directory in `server/`.

Reuse controls from `client/src/components/`, with form controls in `client/src/components/forms/`; keep app navigation in `layout/` and feature-specific UI in `pages/`. Put browser requests in `services/`. Models shared with the API belong in `shared/src/`, which must remain independent of browser and server frameworks. `pages/occupancy_page.rs` is retained legacy code and remains outside the active module tree.

## Run locally

Run the application directly on the host; keep the existing PostgreSQL container and named volume. Do not recreate or clear the database.

```sh
dx build --web --release
cd server
APP_DIST=../target/dx/buildry-property-catalog/release/web/public cargo run
```

Open [http://127.0.0.1:8080](http://127.0.0.1:8080). PostgreSQL defaults to port 5433; override `DATABASE_URL` when needed. On a fresh database, run `cargo run -- set-admin` from `server/` to configure the administrator. Do not run it merely to restart the existing app.

### Resource migration

Back up PostgreSQL before the first run. `server/migrations/001_resource_access.sql` preserves property IDs/content, spaces, photos, legal landlord profiles, and business records. Linked owners become explicit resource owners. Properties without linked accounts use the sole active administrator; ambiguous fallback ownership aborts migration. Legacy manager assignments stop granting access. The original task assignee labels remain in `legacy_task_assignments` for reference. All non-admin accounts become users; normal users must sign in again. Existing users complete onboarding once.

`cargo run -- migrate-only` applies migrations without starting workers or listening. Rerunning it preserves grants and onboarding preferences. Legacy initialization runs once; startup no longer performs the old catalog/template/lease seed rewrites or deletions.

### Checks

```sh
cargo test --workspace
cargo test --manifest-path server/Cargo.toml
cargo check --target wasm32-unknown-unknown
dx build --web --release
```

Database/API regressions use a **disposable restored test database**, never the working catalog:

```sh
TEST_DATABASE_URL=postgres://buildry:buildry_dev_only@127.0.0.1:5433/buildry_unified_test \
  cargo test --manifest-path server/Cargo.toml resource_grants_and_relationships_are_isolated -- --ignored
TEST_DATABASE_URL=postgres://buildry:buildry_dev_only@127.0.0.1:5433/buildry_unified_test \
  cargo test --manifest-path server/Cargo.toml seed_api_review_accounts -- --ignored
```

Start the native API against that test database on port 8089 with the built `APP_DIST`, then run `python3 scripts/test-resource-access.py`. `scripts/test-lease-relationships.py` also exercises synthetic ID upload/signatures/confirmation and needs a disposable `PHOTO_ID_ENCRYPTION_KEY`. It simulates email delivery locally and never calls an email/payment provider. Fixture credentials exist only in the explicitly selected test database. The database tests refuse database names without the `_test` suffix.

Authentication uses signed HTTP-only SameSite=Lax session cookies. Password changes and account session revocation invalidate existing sessions. Set `APP_SECURE_COOKIES=1` when serving over HTTPS. The public guest listing/tour pages remain separate from the authenticated workspace.

Google sign-in uses a Google Cloud **Web application** OAuth client with redirect URI `https://app.buildry.ca/auth/google/callback`. Set `GOOGLE_CLIENT_ID` and `GOOGLE_CLIENT_SECRET` in the API service environment alongside `APP_PUBLIC_URL=https://app.buildry.ca`; keep the secret outside the repository. The login page shows **Continue with Google** only when these values are configured. The server uses a one-time state and cookie, exchanges the authorization code directly with Google, and reads the verified identity from Google's userinfo endpoint. A new Google user receives a user account. Existing Gmail accounts are linked on the first Google login. For an existing account using another email domain, sign in with its password and visit `/auth/google/link` once to link it securely. Deactivated accounts remain blocked. The Google Cloud consent screen must list test users while its publishing status is Testing; public access requires completing Branding and publishing the app there.

The installed `property-catalog.service` serves the release web output. After frontend changes in that setup, run `dx build --web --release` from this directory and reload the browser. A debug build does not update the running service's files.

On the first browser visit, the API copies any existing catalog from the old `buildry.properties.v2` browser storage into an empty database. If there is no browser catalog, it imports the six starter addresses. Once PostgreSQL has records, it is authoritative; later browser caches do not overwrite it.

## Physical space model

Each property has its address, notes, and optional map pin. Its physical spaces are stored separately in the `property_spaces` PostgreSQL table. A space has a stable ID within its property, name, type, and optional notes and parent area or suite. Types currently include Bedroom, Bathroom, Area, Suite, Kitchen, Living space, and Other; the property details editor creates new bedrooms and still displays existing suites. Bedrooms can record ensuite or separate bathroom access. A space can also hold a **current monthly rent** observation; this is distinct from a future listing's asking price. The old fixed rental layout and room counts are no longer edited; any nonzero legacy counts are converted to basic space records on import.

```mermaid
erDiagram
    PROPERTY ||--o{ PHYSICAL_SPACE : contains
    PHYSICAL_SPACE o|--o{ PHYSICAL_SPACE : groups
    PROPERTY ||--o{ FUTURE_LISTING : offers
    FUTURE_LISTING }o--o{ PHYSICAL_SPACE : includes
```

For example, **Basement** can be an Area with its bedrooms, bathroom, and kitchen inside it. A future listing can include the basement group, select two bedroom IDs plus one bathroom ID, or select all spaces except the basement group. Listing combinations, asking prices, and availability are future features; they are not guessed from the property inventory. The six supplied room breakdowns have been entered; unspecified bathroom and suite interiors remain unfilled.

`design/live.png` is a screenshot of the earlier catalog before the portfolio map and space inventory. The browser entry point is `client/src/main.rs`, shared models are in `shared/src/`, and the API entry point is `server/src/main.rs`.

## Verification

`cargo check --target wasm32-unknown-unknown` passes for the web app and `cargo check` passes for the API in WSL. The browser render, PostgreSQL import, map pins, space creation, and parent-area grouping were checked. Temporary test spaces were removed afterward.

## Map lookup

The **Locations** map stays visible above the catalog and shows every property with saved coordinates in one interactive Leaflet/OpenStreetMap view. Click a pin to see its address; zoom and pan to separate nearby houses. The six supplied starter addresses have house-level Nominatim matches checked and stored in the starter catalog. Existing browser catalogs gain those starter pins only when the matching address has no coordinates.

The property management page offers **Locate address**. This sends its address to Nominatim once, with a Waterloo/Ontario hint; use **Save property** to keep the returned coordinates and match label in PostgreSQL. It does not geocode on typing or page load. Changing a property's address clears its old pin so the address can be located again. The map result should be checked against the property.

The public Nominatim service is limited to one request per second and requires attribution and cached results; this app enforces a 1.1-second gap using browser storage. The portfolio map uses Leaflet 1.9.4 from a CDN and OpenStreetMap standard tiles. See the [Nominatim usage policy](https://operations.osmfoundation.org/policies/nominatim/) and [OpenStreetMap tile policy](https://operations.osmfoundation.org/policies/tiles/). A deployed multi-user version should use a separately provisioned geocoder rather than treating the public endpoint as production infrastructure.
