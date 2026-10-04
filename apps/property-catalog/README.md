# Property catalog

A small Dioxus 0.7 web app for recording the six houses. Properties, their physical spaces, current rent observations, and map pins are stored in PostgreSQL.

## Admin property management

The catalog at `/` shows a stable **House ID** and street address for every property. Select a house to open `/properties/{id}`, a full management page for its details, map pin, spaces, room pricing, and images. Use `/properties/new` to add a house. The ID appears only in the admin interface. `/pricing` provides a separate view of the current and seasonal rent for every bedroom and suite; changes are saved per house. Seasonal rent must decrease from September to January to May.

Admins can upload JPEG, PNG, or WebP images up to 5 MB, reorder or remove them by editing the URL lists, and then save the property. Uploaded files are stored in PostgreSQL's `property_images` table and served from `/images/{id}`. HTTPS image URLs and site asset paths are also accepted. The first house or room image is the guest cover; additional images appear on its detail page. Properties without custom photos keep their existing illustrative images.

## House tours

Open `/tours` for the host's weekly calendar with time on the vertical axis. Use the arrows to move between weeks and **Today** to return to the current week. A red line shows the current Toronto time in today's column during calendar hours and refreshes every minute. Select a tour to see its details. The **Open guest page** and **Copy booking link** actions sit together above the calendar. The sidebar can be collapsed to icons.

The host can turn tour booking on or off for each house from the switches above the calendar. The setting is stored in PostgreSQL. Only enabled houses appear on the guest booking form, and the API checks the setting again when checking times, booking, or rescheduling. Existing tours remain visible and can still be cancelled when a house is turned off.

Open `/book` for the guest form. Guests choose a house, date, and start time, then enter a name and phone number. Starts are available each hour from 10 AM through 4 PM in the `America/Toronto` timezone; each booking reserves a two-hour window, ending no later than 6 PM. Dates are available up to 60 days ahead.

After booking, the confirmation shows a unique, unguessable `/booking/{token}` link with a copy button. A guest with that link can view, reschedule, or cancel that booking. Rescheduling follows the same cross-house conflict rules. The link continues to show a cancellation confirmation after cancellation. It is shown to the guest at booking time; the app does not send it by SMS or email.

In the host calendar, select a tour to reschedule its date and start time or cancel it. Rescheduling checks the same cross-house availability as a new booking. Cancellation is confirmed in the popup and stored with a cancellation timestamp; cancelled tours leave the calendar and free their time.

Bookings are stored in `tour_bookings`. Eight labeled sample bookings populate the calendar for layout preview; they do not block guest availability. PostgreSQL has a partial unique index on `(property_id, tour_date, slot_start)` for real bookings. The API takes a PostgreSQL transaction lock for each date and returns HTTP 409 if a new two-hour window overlaps any real booking, regardless of house. Availability hides overlapping and past starts across all houses. The guest form checks availability again after a conflict.

The current server binds to `127.0.0.1`, so its URLs are local previews and cannot be opened by guests on other devices. Public deployment needs a reachable host and HTTPS before any guest link is shared.

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

Open [http://127.0.0.1:8080](http://127.0.0.1:8080). `set-admin` prompts privately for a password of at least 12 characters and stores an Argon2 hash in PostgreSQL. Run it again to change the password; existing sessions are then invalidated. The Rust API serves both the built Dioxus app and `/api/properties` from the same origin. Rebuild with `dx build --web` and reload the browser after frontend changes. The API uses `postgres://buildry:buildry_dev_only@127.0.0.1:5433/buildry` by default; override it with `DATABASE_URL`. The named Docker volume keeps data across container restarts.

The owner dashboard (`/`, `/tours`) redirects to `/login` when signed out. Property and tour management APIs return HTTP 401 without an admin session. Guest booking (`/book`), availability, and each unique guest booking link remain accessible. The login uses `axum-login` with a signed, HTTP-only, SameSite=Strict session cookie. The cookie lasts seven days and authenticated activity renews that seven-day window. Session data and the signing key are kept in PostgreSQL, so a server restart does not sign everyone out. Sign out revokes the session; changing the admin password invalidates existing sessions. The local HTTP setup disables the Secure cookie flag; set `APP_SECURE_COOKIES=1` when serving over HTTPS.

The installed `property-catalog.service` serves the release web output. After frontend changes in that setup, run `dx build --web --release` from this directory and reload the browser. A debug build does not update the running service's files.

On the first browser visit, the API copies any existing catalog from the old `buildry.properties.v2` browser storage into an empty database. If there is no browser catalog, it imports the six starter addresses. Once PostgreSQL has records, it is authoritative; later browser caches do not overwrite it.

## Physical space model

Each property has its address, notes, and optional map pin. Its physical spaces are stored separately in the `property_spaces` PostgreSQL table. A space has a stable ID within its property, name, type, optional level and notes, and an optional parent area or suite. Types currently include Bedroom, Bathroom, Area, Suite, Kitchen, Living space, and Other. Bedrooms can record ensuite or separate bathroom access. A space can also hold a **current monthly rent** observation; this is distinct from a future listing's asking price. The old fixed rental layout and room counts are no longer edited; any nonzero legacy counts are converted to basic space records on import.

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
