use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, Path, State},
    http::{header::CONTENT_TYPE, HeaderMap, StatusCode},
    middleware,
    routing::{get, post, put},
    Json, Router,
};
mod auth;
#[path = "../../src/guest_model.rs"]
// This shared model also contains browser-only rental helpers.
#[allow(dead_code)]
mod guest_model;
#[path = "../../src/model.rs"]
#[allow(dead_code)]
mod model;
mod tours;
use axum_login::{
    tower_sessions::{
        cookie::{Key, SameSite},
        Expiry, SessionManagerLayer,
    },
    AuthManagerLayerBuilder,
};
use guest_model::{GuestListing, GuestRoom};
use model::{Property, Space};
use std::{env, sync::Arc};
use time::Duration;
use tokio::sync::Mutex;
use tokio_postgres::{Client, NoTls, Transaction};
use tower_http::services::{ServeDir, ServeFile};
use tower_sessions_sqlx_store::{sqlx::PgPool, PostgresStore};

type ApiResult<T> = Result<T, (StatusCode, String)>;
type Database = auth::Database;

fn default_seasonal_rents(base: u32) -> (u32, u32, u32) {
    let difference = if base >= 1_400 {
        150
    } else if base >= 1_000 {
        125
    } else {
        100
    };
    (base + difference, base, base.saturating_sub(difference))
}

fn database_error(error: impl std::fmt::Display) -> (StatusCode, String) {
    eprintln!("database error: {error}");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "Database operation failed".into(),
    )
}

async fn upload_image(
    State(db): State<Database>,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<String>> {
    let content_type = headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if !["image/jpeg", "image/png", "image/webp"].contains(&content_type) {
        return Err((
            StatusCode::BAD_REQUEST,
            "Upload a JPEG, PNG, or WebP image.".into(),
        ));
    }
    if body.is_empty() || body.len() > 5_000_000 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Image must be smaller than 5 MB.".into(),
        ));
    }
    let client = db.lock().await;
    let row = client
        .query_one(
            "INSERT INTO property_images (content_type, data) VALUES ($1, $2) RETURNING id",
            &[&content_type, &body.as_ref()],
        )
        .await
        .map_err(database_error)?;
    let id: i64 = row.get(0);
    Ok(Json(format!("/images/{id}")))
}

async fn get_image(
    State(db): State<Database>,
    Path(id): Path<i64>,
) -> ApiResult<(HeaderMap, Vec<u8>)> {
    let client = db.lock().await;
    let row = client
        .query_opt(
            "SELECT content_type, data FROM property_images WHERE id = $1",
            &[&id],
        )
        .await
        .map_err(database_error)?
        .ok_or((StatusCode::NOT_FOUND, "Image not found.".into()))?;
    let content_type: String = row.get(0);
    let data: Vec<u8> = row.get(1);
    let mut headers = HeaderMap::new();
    headers.insert(
        CONTENT_TYPE,
        content_type
            .parse()
            .map_err(|_| database_error("Invalid stored image type"))?,
    );
    Ok((headers, data))
}

async fn load_properties(client: &Client) -> ApiResult<Json<Vec<Property>>> {
    let rows = client
        .query("SELECT data::text FROM properties ORDER BY id", &[])
        .await
        .map_err(database_error)?;
    let mut properties: Vec<Property> = rows
        .into_iter()
        .map(|row| {
            serde_json::from_str::<Property>(&row.get::<_, String>(0)).map_err(database_error)
        })
        .collect::<Result<_, _>>()?;
    let spaces = client
        .query(
            "SELECT property_id, data::text FROM property_spaces ORDER BY id",
            &[],
        )
        .await
        .map_err(database_error)?;
    for row in spaces {
        let property_id: i64 = row.get(0);
        let space: Space =
            serde_json::from_str(&row.get::<_, String>(1)).map_err(database_error)?;
        if let Some(property) = properties
            .iter_mut()
            .find(|item| item.id as i64 == property_id)
        {
            property.spaces.push(space);
        }
    }
    for property in &mut properties {
        property.migrate_legacy_counts();
    }
    Ok(Json(properties))
}

async fn list(State(db): State<Database>) -> ApiResult<Json<Vec<Property>>> {
    let client = db.lock().await;
    load_properties(&client).await
}

async fn guest_listings(State(db): State<Database>) -> ApiResult<Json<Vec<GuestListing>>> {
    let client = db.lock().await;
    let Json(properties) = load_properties(&client).await?;
    let rows = client
        .query(
            "SELECT property_id, available FROM tour_house_settings",
            &[],
        )
        .await
        .map_err(database_error)?;
    let tour_settings: std::collections::HashMap<u32, bool> = rows
        .into_iter()
        .map(|row| (row.get::<_, i64>(0) as u32, row.get(1)))
        .collect();
    Ok(Json(
        properties
            .into_iter()
            .map(|property| {
                let mut amenities = property.amenities.clone();
                if property.spaces.iter().any(|space| space.kind == "Kitchen") {
                    amenities.push("Kitchen".to_string());
                }
                if property
                    .spaces
                    .iter()
                    .any(|space| space.kind == "Living space")
                {
                    amenities.push("Living area".to_string());
                }
                if property.spaces.iter().any(|space| space.kind == "Bathroom") {
                    amenities.push("Bathroom".to_string());
                }
                if property
                    .spaces
                    .iter()
                    .any(|space| space.bathroom_access == "Ensuite")
                {
                    amenities.push("Rooms with ensuite bathrooms".to_string());
                }
                GuestListing {
                    id: property.id,
                    name: property.name,
                    address: property.address,
                    area: property.area,
                    photos: property.photos,
                    amenities,
                    rooms: property
                        .spaces
                        .into_iter()
                        .filter(|space| space.kind == "Bedroom" || space.kind == "Suite")
                        .map(|space| {
                            let fallback = space.current_monthly_rent.map(default_seasonal_rents);
                            let sep = space.rent_sep_dec.or(fallback.map(|rates| rates.0));
                            let jan = space.rent_jan_apr.or(fallback.map(|rates| rates.1));
                            let may = space.rent_may_aug.or(fallback.map(|rates| rates.2));
                            GuestRoom {
                                id: space.id,
                                name: space.name,
                                kind: space.kind.clone(),
                                bathroom_access: space.bathroom_access.clone(),
                                photos: space.photos,
                                monthly_price: [sep, jan, may].into_iter().flatten().min(),
                                price_is_estimate: !space.seasonal_prices_confirmed,
                                rent_sep_dec: sep,
                                rent_jan_apr: jan,
                                rent_may_aug: may,
                                amenities: {
                                    let mut details = space.amenities.clone();
                                    if !space.bathroom_access.is_empty() {
                                        details.push(format!("{} bathroom", space.bathroom_access));
                                    }
                                    if !space.level.is_empty() {
                                        details.push(space.level.clone());
                                    }
                                    if space.kind == "Suite" {
                                        details.push("Private suite".into());
                                    }
                                    details
                                },
                                unavailable_periods: space.unavailable_periods,
                                availability_confirmed: space.availability_confirmed,
                            }
                        })
                        .collect(),
                    tours_available: tour_settings.get(&property.id).copied().unwrap_or(true),
                }
            })
            .collect(),
    ))
}

// First browser visit copies its existing local catalog into an empty DB.
// Once the DB has records, it is authoritative and subsequent browsers only read it.
async fn bootstrap(
    State(db): State<Database>,
    Json(initial): Json<Vec<Property>>,
) -> ApiResult<Json<Vec<Property>>> {
    let mut client = db.lock().await;
    let transaction = client.transaction().await.map_err(database_error)?;
    let row = transaction
        .query_one("SELECT COUNT(*) FROM properties", &[])
        .await
        .map_err(database_error)?;
    let count: i64 = row.get(0);
    if count == 0 {
        for mut property in initial {
            property.migrate_legacy_counts();
            validate(&property)?;
            save_property(&transaction, &property).await?;
        }
    }
    transaction.commit().await.map_err(database_error)?;
    load_properties(&client).await
}

fn validate(property: &Property) -> ApiResult<()> {
    if property.id == 0 || property.name.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Property needs an ID and name".into(),
        ));
    }
    if property.amenities.len() > 30 || property.amenities.iter().any(|item| item.len() > 80) {
        return Err((
            StatusCode::BAD_REQUEST,
            "Too many property amenities.".into(),
        ));
    }
    let valid_photo = |url: &str| {
        url.len() <= 1000
            && (url.starts_with("https://")
                || url.starts_with("/assets/")
                || url.starts_with("/images/"))
    };
    if property.photos.len() > 20 || property.photos.iter().any(|url| !valid_photo(url)) {
        return Err((
            StatusCode::BAD_REQUEST,
            "Use up to 20 HTTPS or site image URLs.".into(),
        ));
    }
    if property
        .latitude
        .is_some_and(|value| !(-90.0..=90.0).contains(&value))
        || property
            .longitude
            .is_some_and(|value| !(-180.0..=180.0).contains(&value))
    {
        return Err((StatusCode::BAD_REQUEST, "Invalid coordinates".into()));
    }
    let mut ids = std::collections::HashSet::new();
    for space in &property.spaces {
        if space.photos.len() > 20 || space.photos.iter().any(|url| !valid_photo(url)) {
            return Err((
                StatusCode::BAD_REQUEST,
                "Use up to 20 HTTPS or site image URLs per room.".into(),
            ));
        }
        if space.amenities.len() > 30 || space.amenities.iter().any(|item| item.len() > 80) {
            return Err((StatusCode::BAD_REQUEST, "Too many room amenities.".into()));
        }
        if space.id == 0 || space.name.trim().is_empty() || !ids.insert(space.id) {
            return Err((StatusCode::BAD_REQUEST, "Invalid or duplicate space".into()));
        }
        if let (Some(sep), Some(jan), Some(may)) =
            (space.rent_sep_dec, space.rent_jan_apr, space.rent_may_aug)
        {
            if may == 0 || sep <= jan || jan <= may {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "Seasonal rent must decrease from September to January to May.".into(),
                ));
            }
        }
        if space.seasonal_prices_confirmed
            && (space.rent_sep_dec.is_none()
                || space.rent_jan_apr.is_none()
                || space.rent_may_aug.is_none())
        {
            return Err((
                StatusCode::BAD_REQUEST,
                "Enter all three seasonal rates before confirming them.".into(),
            ));
        }
        let mut seen_periods = std::collections::HashSet::new();
        for period in &space.unavailable_periods {
            let valid = period.split_once('-').is_some_and(|(year, season)| {
                year.parse::<i32>()
                    .ok()
                    .is_some_and(|year| (2020..=2100).contains(&year))
                    && ["jan", "may", "sep"].contains(&season)
            });
            if !valid || !seen_periods.insert(period) {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "Invalid availability period.".into(),
                ));
            }
        }
    }
    for space in &property.spaces {
        if let Some(parent_id) = space.parent_id {
            let parent = property.spaces.iter().find(|item| item.id == parent_id);
            if parent.is_none_or(|item| {
                (item.kind != "Area" && item.kind != "Suite") || item.parent_id.is_some()
            }) {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "Space needs a top-level area".into(),
                ));
            }
        }
    }
    Ok(())
}

async fn save_property(transaction: &Transaction<'_>, property: &Property) -> ApiResult<()> {
    let mut core = serde_json::to_value(property).map_err(database_error)?;
    for old_field in ["spaces", "kind", "ensuite_rooms", "shared_rooms", "suites"] {
        core.as_object_mut().unwrap().remove(old_field);
    }
    let data = core.to_string();
    transaction
        .execute(
            "INSERT INTO properties (id, data) VALUES ($1, $2::text::jsonb) \
             ON CONFLICT (id) DO UPDATE SET data = EXCLUDED.data, updated_at = NOW()",
            &[&(property.id as i64), &data],
        )
        .await
        .map_err(database_error)?;
    let ids: Vec<i64> = property
        .spaces
        .iter()
        .map(|space| space.id as i64)
        .collect();
    transaction
        .execute(
            "DELETE FROM property_spaces WHERE property_id = $1 AND id <> ALL($2::bigint[])",
            &[&(property.id as i64), &ids],
        )
        .await
        .map_err(database_error)?;
    for space in &property.spaces {
        let data = serde_json::to_string(space).map_err(database_error)?;
        transaction
            .execute(
                "INSERT INTO property_spaces (property_id, id, data) VALUES ($1, $2, $3::text::jsonb) \
                 ON CONFLICT (property_id, id) DO UPDATE SET data = EXCLUDED.data",
                &[&(property.id as i64), &(space.id as i64), &data],
            )
            .await
            .map_err(database_error)?;
    }
    Ok(())
}

async fn save(
    Path(id): Path<u32>,
    State(db): State<Database>,
    Json(property): Json<Property>,
) -> ApiResult<Json<Property>> {
    if id != property.id {
        return Err((StatusCode::BAD_REQUEST, "Property ID mismatch".into()));
    }
    validate(&property)?;
    let mut client = db.lock().await;
    let transaction = client.transaction().await.map_err(database_error)?;
    save_property(&transaction, &property).await?;
    transaction.commit().await.map_err(database_error)?;
    Ok(Json(property))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://buildry:buildry_dev_only@127.0.0.1:5433/buildry".into());
    let (client, connection) = tokio_postgres::connect(&url, NoTls).await?;
    tokio::spawn(async move {
        if let Err(error) = connection.await {
            eprintln!("PostgreSQL connection closed: {error}");
        }
    });
    client
        .batch_execute(
            "CREATE TABLE IF NOT EXISTS properties (
                id BIGINT PRIMARY KEY,
                data JSONB NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )",
        )
        .await?;
    client
        .batch_execute(
            "CREATE TABLE IF NOT EXISTS property_spaces (
                property_id BIGINT NOT NULL REFERENCES properties(id) ON DELETE CASCADE,
                id BIGINT NOT NULL,
                data JSONB NOT NULL,
                PRIMARY KEY (property_id, id)
            )",
        )
        .await?;
    client
        .batch_execute(
            "CREATE TABLE IF NOT EXISTS property_images (
            id BIGSERIAL PRIMARY KEY,
            content_type TEXT NOT NULL,
            data BYTEA NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )",
        )
        .await?;
    client
        .batch_execute(
            "CREATE TABLE IF NOT EXISTS tour_house_settings (
                property_id BIGINT PRIMARY KEY REFERENCES properties(id) ON DELETE CASCADE,
                available BOOLEAN NOT NULL DEFAULT TRUE
            )",
        )
        .await?;
    client
        .batch_execute(
            "CREATE TABLE IF NOT EXISTS tour_bookings (
                id BIGSERIAL PRIMARY KEY,
                property_id BIGINT NOT NULL REFERENCES properties(id),
                tour_date DATE NOT NULL,
                slot_start TEXT NOT NULL,
                guest_name TEXT NOT NULL,
                guest_phone TEXT NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                UNIQUE (property_id, tour_date, slot_start)
            )",
        )
        .await?;
    client
        .batch_execute(
            "ALTER TABLE tour_bookings ADD COLUMN IF NOT EXISTS is_demo BOOLEAN NOT NULL DEFAULT FALSE;
             ALTER TABLE tour_bookings ADD COLUMN IF NOT EXISTS cancelled_at TIMESTAMPTZ;
             ALTER TABLE tour_bookings ADD COLUMN IF NOT EXISTS guest_token TEXT NOT NULL DEFAULT gen_random_uuid()::text;
             ALTER TABLE tour_bookings ADD COLUMN IF NOT EXISTS room_id BIGINT;
             ALTER TABLE tour_bookings ADD COLUMN IF NOT EXISTS room_name TEXT;
             ALTER TABLE tour_bookings ADD COLUMN IF NOT EXISTS rental_start TEXT;
             ALTER TABLE tour_bookings ADD COLUMN IF NOT EXISTS rental_year INTEGER;
             ALTER TABLE tour_bookings ADD COLUMN IF NOT EXISTS rental_months INTEGER;
             ALTER TABLE tour_bookings ADD COLUMN IF NOT EXISTS quoted_monthly_rent INTEGER;
             ALTER TABLE tour_bookings ADD COLUMN IF NOT EXISTS quote_is_estimate BOOLEAN NOT NULL DEFAULT TRUE;
             ALTER TABLE tour_bookings DROP CONSTRAINT IF EXISTS tour_bookings_property_id_tour_date_slot_start_key;
             DROP INDEX IF EXISTS tour_bookings_real_slot_unique;
             CREATE UNIQUE INDEX tour_bookings_real_slot_unique
               ON tour_bookings (property_id, tour_date, slot_start) WHERE NOT is_demo AND cancelled_at IS NULL;
             CREATE UNIQUE INDEX IF NOT EXISTS tour_bookings_guest_token_unique
               ON tour_bookings (guest_token);",
        )
        .await?;
    client
        .batch_execute(
            "CREATE TABLE IF NOT EXISTS admin_users (
            id BIGINT PRIMARY KEY CHECK (id = 1),
            password_hash TEXT NOT NULL
        )",
        )
        .await?;
    client
        .batch_execute(
            "CREATE TABLE IF NOT EXISTS auth_settings (
            id BIGINT PRIMARY KEY CHECK (id = 1),
            signing_key BYTEA NOT NULL
        )",
        )
        .await?;
    let generated_key = Key::generate();
    client
        .execute(
            "INSERT INTO auth_settings (id, signing_key) VALUES (1, $1)
         ON CONFLICT (id) DO NOTHING",
            &[&generated_key.master()],
        )
        .await?;
    let key_bytes: Vec<u8> = client
        .query_one("SELECT signing_key FROM auth_settings WHERE id = 1", &[])
        .await?
        .get(0);
    let signing_key = Key::from(key_bytes.as_slice());
    let db = Arc::new(Mutex::new(client));
    if env::args().nth(1).as_deref() == Some("set-admin") {
        auth::create_admin(&db).await?;
        return Ok(());
    }
    let app_dist = env::var("APP_DIST")
        .unwrap_or_else(|_| "../target/dx/buildry-property-catalog/debug/web/public".into());
    let index = format!("{app_dist}/index.html");
    let admin_api = Router::new()
        .route("/api/properties", get(list))
        .route("/api/properties/bootstrap", post(bootstrap))
        .route("/api/properties/{id}", put(save))
        .route(
            "/api/images",
            post(upload_image).layer(DefaultBodyLimit::max(5_000_000)),
        )
        .route("/api/tours", get(tours::list))
        .route("/api/tours/houses", get(tours::houses))
        .route("/api/tours/houses/{id}", put(tours::set_house_availability))
        .route(
            "/api/tours/{id}",
            put(tours::reschedule).delete(tours::cancel),
        )
        .route_layer(middleware::from_fn(auth::require_api));
    let admin_pages = Router::new()
        .route_service("/", ServeFile::new(index.clone()))
        .route_service("/index.html", ServeFile::new(index.clone()))
        .route_service("/tours", ServeFile::new(index.clone()))
        .route_service("/availability", ServeFile::new(index.clone()))
        .route_service("/pricing", ServeFile::new(index.clone()))
        .route_service("/properties/{id}", ServeFile::new(index.clone()))
        .route_layer(middleware::from_fn(auth::require_page));
    let session_pool = PgPool::connect(&url).await?;
    let session_store = PostgresStore::new(session_pool);
    session_store.migrate().await?;
    let session_layer = SessionManagerLayer::new(session_store)
        .with_secure(env::var("APP_SECURE_COOKIES").as_deref() == Ok("1"))
        .with_same_site(SameSite::Strict)
        .with_expiry(Expiry::OnInactivity(Duration::days(7)))
        .with_signed(signing_key);
    let auth_layer =
        AuthManagerLayerBuilder::new(auth::Backend::new(db.clone()), session_layer).build();
    let app = Router::new()
        .merge(admin_api)
        .merge(admin_pages)
        .route("/login", get(auth::login_page).post(auth::login))
        .route("/logout", post(auth::logout))
        .route("/api/tours/options", get(tours::options))
        .route("/api/guest-listings", get(guest_listings))
        .route("/images/{id}", get(get_image))
        .route_service("/stays", ServeFile::new(index.clone()))
        .route_service("/stays/house/{id}", ServeFile::new(index.clone()))
        .route_service(
            "/stays/room/{property_id}/{room_id}",
            ServeFile::new(index.clone()),
        )
        .route("/api/tours/availability", get(tours::availability))
        .route("/api/tours", post(tours::book))
        .route(
            "/api/guest-bookings/{token}",
            get(tours::guest_lookup)
                .put(tours::guest_reschedule)
                .delete(tours::guest_cancel),
        )
        .route_service("/book", ServeFile::new(index.clone()))
        .route_service("/booking/{token}", ServeFile::new(index.clone()))
        .fallback_service(ServeDir::new(app_dist))
        .with_state(db)
        .layer(auth_layer);
    let bind = env::var("APP_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".into());
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    println!("Property catalog and API listening on http://{bind}");
    axum::serve(listener, app).await?;
    Ok(())
}
