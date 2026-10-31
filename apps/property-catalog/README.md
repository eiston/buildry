# Property catalog

## MCP access

Connect an MCP client to `https://app.buildry.ca/mcp`. The server advertises OAuth 2.1 metadata, registers public clients dynamically, and asks the user to approve the requested permissions in a browser. Signed-out users go through the normal Buildry sign-in page first, then return to the consent page. Authorization uses an S256 PKCE code; access tokens last one hour and rotating refresh tokens last 30 days. Deactivating a Buildry account immediately blocks its tokens.

`read_guest_listings` returns the same house and room data as `/api/guest-listings`, including absolute image URLs, amenities, prices, and occupancy. `get_listing_image` returns the image bytes for a numeric image ID referenced by one of those public listings. Both tools are read-only and require the `listings.read` OAuth scope. To add the server to Codex CLI, run `codex mcp add buildry --url https://app.buildry.ca/mcp`, then `codex mcp login buildry` and approve the browser prompt.

Admin, staff, and landlord accounts can request `tasks.read` for `list_tasks`, `get_task`, and `list_task_options`, and `tasks.write` for `create_task` and `list_task_options`. Task creation starts in Backlog and requires a house ID and assignee name from `list_task_options`; landlords can create tasks only for their houses. A creator sees their own tasks and can select additional viewers. Shared viewers can read tasks but cannot change or delete them. MCP task tools enforce the same visibility. Existing `listings.read` tokens do not gain task access. Reconnect an existing MCP client with the task scopes to make these tools available.

A small Dioxus 0.7 web app for recording the six houses. Properties, their physical spaces, current rent observations, and map pins are stored in PostgreSQL.

## Admin property management

Each property has exactly one landlord owner. The existing **Property managers**
field assigns staff collaborators. The owner and administrators can change those
assignments; assigned managers can work on the property's records, host tours,
and receive tasks. A manager can be assigned to several properties, and the
portfolio pages show records from all properties they manage. The owner and
administrators always have editing access. Only an administrator can change the
legal landlord owner through Access → Landlords. Property access is checked by
the API, including MCP task creation.

The catalog at `/` shows a stable **House ID** and street address for every property. Select a house to open `/properties/{id}`, a full management page for its details, map pin, spaces, room pricing, and images. Use `/properties/new` to add a house. The ID appears only in the admin interface. `/pricing` provides a separate view of the current and seasonal rent for every bedroom and suite; changes are saved per house. Seasonal rent must decrease from September to January to May.

Admins can add JPEG, PNG, or WebP images up to 5 MB from the property details gallery, drag them into order, or remove them with the control on each image. The top-left image is the guest cover. Clicking an image opens a full-screen view. Uploaded files are stored in PostgreSQL's `property_images` table and served from `/images/{id}`. Save the property after changing its photo list or order.

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

## Signup, access, and leases

Email verification links open a confirmation page before activating a guest account, so link previews do not consume them. Expired links can be replaced at `/resend-verification` (two-minute resend limit). `/forgot-password` sends a one-hour password reset link to an active account, limited to one request every two minutes. Signup and password setup require password confirmation.

The public listing and login page link to `/signup`. Signup creates a `guest` account and emails a 24-hour verification link; a guest cannot sign in before verifying their email. `/access` is the central account directory. Admins can search and filter guests, tenants, landlords, staff, and admins; create accounts; edit shared name, email, and phone fields; assign a role; activate or deactivate access; link a landlord account to its legal landlord profile; and send password setup instructions when email is configured. It prevents removal of the last active admin with a password. Staff can manage operational pages but cannot manage Access. Tenant room assignments and landlord legal and property ownership details are in the Leases dashboard. A landlord account sees only its linked properties at `/landlord-home`. New manually created accounts have no password until an admin sends setup instructions. A signed lease upgrades a verified guest with the same email to tenant. If no account exists, confirmation creates a tenant with no password and emails a seven-day password setup link. A privileged account is never silently converted to tenant.

**Leases → Landlords** manages multiple landlords and their property assignments. Its searchable, scrollable property selector supports larger portfolios and can show only assigned properties. The migration creates a Helen Bai record and assigns existing properties to her; `backfill-helen-contact` fills missing contact fields with non-deliverable example data. An authorized signatory must still be confirmed before emailing any lease. New properties initially use Helen Bai until reassigned. A new tour asks for the guest's email, rental room, move-in date, and duration.

`/leases` is the lease management dashboard. Search or filter drafts, sent leases, signed leases, and confirmed tenancies; select a lease to review its property, room, dates, rent, landlord, PDF, and next action. **Create a lease** opens a two-step flow: choose a potential guest from their tour history, then choose the property, room, move-in date, duration (a preset or a custom number of months), recipient email, and monthly rent. The toured property and known details are preselected; staff can change them before preparing a draft. The lease fills the property landlord and standard terms automatically. Full edit opens only when requested. Drafts reflect landlord edits; sent leases keep their original snapshot. Review the PDF and all standard terms before emailing. The PDF uses the [official Ontario Standard Form of Lease 2229E (2020/12)](https://forms.mgcs.gov.on.ca/en/dataset/047-2229/resource/05677ea2-3173-4c0e-9a14-7a06cbcb41b9), completed with the entered values and an attached schedule. The original government form pages are retained.

Lease templates can include one additional PDF of up to 30 pages and 5 MB. Its pages follow the completed form and schedule in the combined PDF. Each new lease copies the template's PDF so later template changes do not alter a sent lease.

Emailing a draft creates a new private 14-day URL. The email contains the signing link rather than an unsigned PDF attachment. The private page guides the tenant through reviewing the completed PDF and attached schedule, typing their legal name, accepting the electronic-signature consent, and selecting **Sign lease**. SMS verification is not enabled. The landlord's authorized signer appears on the PDF when it is sent; the tenant's signature appears after signing. Signing records the signature, time, IP, and user agent. Immediately after signing a real lease, the tenant is taken to the Photo ID upload page. They choose a Canadian driver's licence or provincial/territorial photo card (front and back) or a Canadian/foreign passport (photo page), read the linked `/photo-id-terms` notice, and submit the images. The images are encrypted in a separate `lease_photo_ids` table and are served only through staff-authorized routes; each view is logged. Staff open each required image and explicitly approve it. A rejected ID is deleted so the tenant can upload a replacement. Admin confirmation requires approval. Confirmation upgrades or creates the tenant account, emails the signed PDF copy, and includes a password setup link only when needed. If that email fails, the confirmed row shows a retry action. Private links are stored as SHA-256 hashes and are replaced on resend.

Set `PHOTO_ID_ENCRYPTION_KEY` to a stable, randomly generated **base64-encoded 32-byte key** in the API service environment before accepting ID uploads. For example, `openssl rand -base64 32` generates a suitable value. Keep it outside the repository and back it up securely: losing or changing it makes stored ID images unreadable. Without the key, the upload endpoint returns a service-unavailable response and stores nothing. Signing extends the private link for at least seven days so the ID upload can finish. Staff can email a new seven-day ID link from the lease dashboard if it expires.

Staff record the actual tenancy end date in the lease dashboard. The API deletes ID images one year after that recorded date; cleanup runs when the date is saved, on startup, and daily. A lease's scheduled end date does not start the retention clock. Database backups must follow the same secure retention policy.

The server uses the Rust `image` library to fully decode each upload before storage. It rejects unsupported or damaged files, images smaller than 600 pixels on either side, images larger than 6,000 pixels on a side or 20 million pixels total, and nearly blank images. This is image integrity screening, not proof that a document is genuine; staff still inspect and approve each ID before confirming the tenancy.

Email sending requires these server environment variables, kept outside the repository: `RESEND_API_KEY`, `RESEND_FROM_EMAIL` (a sender on a verified Resend domain), and `APP_PUBLIC_URL` (the HTTPS origin recipients can reach). The app refuses to email a lease or create a signup if they are missing. Do not use a localhost URL in email. To populate sample accounts without real email addresses or passwords, run `cargo run --release -- seed-demo-tenants` and `cargo run --release -- seed-demo-guests` from `server/`. These commands can be rerun without duplicating records. Their addresses use the reserved `example.invalid` domain.

The API's `email-smoke-test` command sends a small attachment to Resend's test recipient `delivered@resend.dev` and requires a provider message ID. Pass a real address as the next argument (for example, `email-smoke-test you@example.com`) to check delivery to an inbox. It requires the same environment variables as live email. The server unit must receive the sender and key before running the command. A local-provider test also verifies the request recipient, bearer token, and attachment shape without sending email.

Demo account rows created by the optional seed commands have `users.is_seeded = TRUE`. `backfill-helen-access` creates Helen Bai's linked landlord account with no password or email delivery; an admin can update the address and send setup instructions from Access after email is configured.

## Run

From this directory, start PostgreSQL with Docker Compose:

```sh
docker compose up -d --wait
```

Build the web app, set your admin password, then run the API from WSL:

```sh
dx build --web
cd server
cargo run -- set-admin
cargo run
```

Open [http://127.0.0.1:8080](http://127.0.0.1:8080). `set-admin` prompts for an email and a private password of at least 12 characters and stores an Argon2 hash in PostgreSQL. Run it again to change the password; existing sessions are then invalidated. The Rust API serves both the built Dioxus app and its APIs from the same origin. Rebuild with `dx build --web` and reload the browser after frontend changes. The API uses `postgres://buildry:buildry_dev_only@127.0.0.1:5433/buildry` by default; override it with `DATABASE_URL`. The named Docker volume keeps data across container restarts.

The admin pages redirect to `/login` when signed out; tenant accounts are redirected to `/my-home`. Admin APIs require the admin role. Guest booking (`/book`) and each unique guest booking link remain accessible. The login uses `axum-login` with a signed, HTTP-only, SameSite=Lax session cookie so a signed-in account can reach the OAuth consent page from an external MCP client. The cookie lasts seven days. Session data and the signing key are kept in PostgreSQL, so a server restart does not sign everyone out. Sign out revokes the session; changing a password invalidates that account's existing sessions. The local HTTP setup disables the Secure cookie flag; set `APP_SECURE_COOKIES=1` when serving over HTTPS.

Google sign-in uses a Google Cloud **Web application** OAuth client with redirect URI `https://app.buildry.ca/auth/google/callback`. Set `GOOGLE_CLIENT_ID` and `GOOGLE_CLIENT_SECRET` in the API service environment alongside `APP_PUBLIC_URL=https://app.buildry.ca`; keep the secret outside the repository. The login page shows **Continue with Google** only when these values are configured. The server uses a one-time state and cookie, exchanges the authorization code directly with Google, and reads the verified identity from Google's userinfo endpoint. A new Google user receives a guest account. Existing Gmail accounts are linked on the first Google login. For an existing account using another email domain, sign in with its password and visit `/auth/google/link` once to link it securely. Deactivated accounts remain blocked. The Google Cloud consent screen must list test users while its publishing status is Testing; public access requires completing Branding and publishing the app there.

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

`design/live.png` is a screenshot of the earlier catalog before the portfolio map and space inventory. The app source is in `src/main.rs`, the shared model is in `src/model.rs`, and the API is in `server/src/main.rs`.

## Verification

`cargo check --target wasm32-unknown-unknown` passes for the web app and `cargo check` passes for the API in WSL. The browser render, PostgreSQL import, map pins, space creation, and parent-area grouping were checked. Temporary test spaces were removed afterward.

## Map lookup

The **Locations** map stays visible above the catalog and shows every property with saved coordinates in one interactive Leaflet/OpenStreetMap view. Click a pin to see its address; zoom and pan to separate nearby houses. The six supplied starter addresses have house-level Nominatim matches checked and stored in the starter catalog. Existing browser catalogs gain those starter pins only when the matching address has no coordinates.

The property management page offers **Locate address**. This sends its address to Nominatim once, with a Waterloo/Ontario hint; use **Save property** to keep the returned coordinates and match label in PostgreSQL. It does not geocode on typing or page load. Changing a property's address clears its old pin so the address can be located again. The map result should be checked against the property.

The public Nominatim service is limited to one request per second and requires attribution and cached results; this app enforces a 1.1-second gap using browser storage. The portfolio map uses Leaflet 1.9.4 from a CDN and OpenStreetMap standard tiles. See the [Nominatim usage policy](https://operations.osmfoundation.org/policies/nominatim/) and [OpenStreetMap tile policy](https://operations.osmfoundation.org/policies/tiles/). A deployed multi-user version should use a separately provisioned geocoder rather than treating the public endpoint as production infrastructure.
