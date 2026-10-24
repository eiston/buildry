use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{
        header::{CACHE_CONTROL, CONTENT_TYPE},
        HeaderMap, StatusCode,
    },
    middleware::{self, Next},
    response::Response,
    routing::{get, post, put},
    Json, Router,
};
mod auth;
mod automations;
mod avatar;
mod collapsible_panel;
mod google_auth;
#[path = "../../src/guest_model.rs"]
// This shared model also contains browser-only rental helpers.
#[allow(dead_code)]
mod guest_model;
mod housework;
mod intersection;
mod lease_pdf;
mod leasing;
mod mcp;
#[path = "../../src/model.rs"]
#[allow(dead_code)]
mod model;
mod photo_id;
mod tenants;
mod tours;
use axum_login::{
    tower_sessions::{
        cookie::{Key, SameSite},
        Expiry, SessionManagerLayer,
    },
    AuthManagerLayerBuilder,
};
use guest_model::{GuestListing, GuestRoom, OccupiedPeriod};
use model::{Property, Space};
use sha2::{Digest, Sha256};
use std::{env, sync::Arc};
use time::Duration;
use tokio::sync::Mutex;
use tokio_postgres::{Client, NoTls, Transaction};
use tower_http::services::{ServeDir, ServeFile};
use tower_sessions_sqlx_store::{sqlx::PgPool, PostgresStore};

type ApiResult<T> = Result<T, (StatusCode, String)>;
type Database = auth::Database;

fn account_name_parts(name: &str) -> (String, String) {
    let mut words = name.split_whitespace();
    let first = words.next().unwrap_or_default().to_string();
    let last = words.collect::<Vec<_>>().join(" ");
    (first, last)
}

#[cfg(test)]
#[test]
fn account_name_parts_preserve_multiword_last_names() {
    assert_eq!(
        account_name_parts("  Mary   Van Buren  "),
        ("Mary".to_string(), "Van Buren".to_string())
    );
}

#[derive(serde::Deserialize)]
struct AddressQuery {
    q: String,
}

#[derive(serde::Deserialize)]
struct PhotonResponse {
    features: Vec<PhotonFeature>,
}

#[derive(serde::Deserialize)]
struct PhotonFeature {
    properties: PhotonProperties,
    geometry: PhotonGeometry,
}

#[derive(serde::Deserialize)]
struct PhotonProperties {
    name: Option<String>,
    osm_key: Option<String>,
    osm_value: Option<String>,
    housenumber: Option<String>,
    street: Option<String>,
    city: Option<String>,
    state: Option<String>,
    postcode: Option<String>,
    countrycode: Option<String>,
}

#[derive(serde::Deserialize)]
struct PhotonGeometry {
    coordinates: Vec<f64>,
}

#[derive(serde::Serialize)]
struct AddressSuggestion {
    label: String,
    latitude: f64,
    longitude: f64,
}

#[derive(serde::Serialize)]
struct GuestPlaceResult {
    matching_property_ids: Vec<u32>,
    label: String,
}

fn distance_km(a_lat: f64, a_lon: f64, b_lat: f64, b_lon: f64) -> f64 {
    let (a, b) = (a_lat.to_radians(), b_lat.to_radians());
    let delta_lat = b - a;
    let delta_lon = (b_lon - a_lon).to_radians();
    6371.0
        * 2.0
        * ((delta_lat / 2.0).sin().powi(2) + a.cos() * b.cos() * (delta_lon / 2.0).sin().powi(2))
            .sqrt()
            .asin()
}

fn public_area(address: &str) -> String {
    let city = address
        .split(',')
        .nth(1)
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .unwrap_or("Waterloo");
    format!("{city}, ON")
}

#[cfg(test)]
mod public_listing_tests {
    use super::public_area;

    #[test]
    fn guest_area_hides_postal_code() {
        let address = "315 Dearborn Blvd, Waterloo, ON N2J 4Y9";
        assert_eq!(public_area(address), "Waterloo, ON");
    }
}

async fn guest_place_search(
    State(db): State<Database>,
    Query(query): Query<AddressQuery>,
) -> ApiResult<Json<GuestPlaceResult>> {
    let search = query.q.trim();
    if search.len() < 3 || search.len() > 120 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Enter at least three place characters.".into(),
        ));
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|_| (StatusCode::BAD_GATEWAY, "Place search unavailable".into()))?;
    let response = client
        .get("https://photon.komoot.io/api/")
        .query(&[
            ("q", search),
            ("limit", "5"),
            ("lat", "43.4643"),
            ("lon", "-80.5204"),
            ("lang", "en"),
        ])
        .send()
        .await
        .map_err(|_| (StatusCode::BAD_GATEWAY, "Place search unavailable".into()))?;
    if !response.status().is_success() {
        return Err((StatusCode::BAD_GATEWAY, "Place search unavailable".into()));
    }
    let data: PhotonResponse = response
        .json()
        .await
        .map_err(|_| (StatusCode::BAD_GATEWAY, "Place search unavailable".into()))?;
    let places: Vec<_> = data
        .features
        .into_iter()
        .filter(|feature| {
            feature
                .properties
                .countrycode
                .as_deref()
                .is_some_and(|code| code.eq_ignore_ascii_case("CA"))
                && feature.geometry.coordinates.len() >= 2
        })
        .collect();
    let place = places
        .iter()
        .find(|feature| {
            feature
                .properties
                .name
                .as_deref()
                .is_some_and(|name| name.eq_ignore_ascii_case(search))
                && feature.properties.osm_key.as_deref() == Some("place")
                && matches!(
                    feature.properties.osm_value.as_deref(),
                    Some("city" | "town" | "village")
                )
        })
        .or_else(|| {
            places
                .iter()
                .filter(|feature| {
                    feature
                        .properties
                        .name
                        .as_deref()
                        .is_some_and(|name| name.eq_ignore_ascii_case(search))
                })
                .min_by(|a, b| {
                    distance_km(
                        43.4643,
                        -80.5204,
                        a.geometry.coordinates[1],
                        a.geometry.coordinates[0],
                    )
                    .total_cmp(&distance_km(
                        43.4643,
                        -80.5204,
                        b.geometry.coordinates[1],
                        b.geometry.coordinates[0],
                    ))
                })
        })
        .or_else(|| places.first())
        .ok_or((StatusCode::NOT_FOUND, "Place not found".into()))?;
    let latitude = place.geometry.coordinates[1];
    let longitude = place.geometry.coordinates[0];
    let mut label_parts = Vec::<&str>::new();
    for part in [
        place.properties.name.as_deref(),
        place.properties.city.as_deref(),
        place.properties.state.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        if !label_parts
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(part))
        {
            label_parts.push(part);
        }
    }
    let label = label_parts.join(", ");
    let city_search = place.properties.osm_key.as_deref() == Some("place")
        && matches!(
            place.properties.osm_value.as_deref(),
            Some("city" | "town" | "village")
        );
    let radius_km = if city_search { 10.0 } else { 3.5 };
    let db = db.lock().await;
    let Json(properties) = load_properties(&db).await?;
    let matching_property_ids = properties
        .into_iter()
        .filter(|property| match (property.latitude, property.longitude) {
            (Some(lat), Some(lon)) => distance_km(latitude, longitude, lat, lon) <= radius_km,
            _ => false,
        })
        .map(|property| property.id)
        .collect();
    Ok(Json(GuestPlaceResult {
        matching_property_ids,
        label,
    }))
}

async fn address_suggestions(
    Query(query): Query<AddressQuery>,
) -> ApiResult<Json<Vec<AddressSuggestion>>> {
    let search = query.q.trim();
    if search.len() < 4 || search.len() > 120 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Enter at least four address characters.".into(),
        ));
    }
    let response = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|_| {
            (
                StatusCode::BAD_GATEWAY,
                "Address suggestions unavailable".into(),
            )
        })?
        .get("https://photon.komoot.io/api/")
        .query(&[
            ("q", search),
            ("limit", "5"),
            ("lat", "43.4643"),
            ("lon", "-80.5204"),
            ("lang", "en"),
        ])
        .send()
        .await
        .map_err(|_| {
            (
                StatusCode::BAD_GATEWAY,
                "Address suggestions unavailable".into(),
            )
        })?;
    if !response.status().is_success() {
        return Err((
            StatusCode::BAD_GATEWAY,
            "Address suggestions unavailable".into(),
        ));
    }
    let data: PhotonResponse = response.json().await.map_err(|_| {
        (
            StatusCode::BAD_GATEWAY,
            "Address suggestions unavailable".into(),
        )
    })?;
    let matches = data
        .features
        .into_iter()
        .filter_map(|feature| {
            let p = feature.properties;
            if p.countrycode
                .as_deref()
                .map(|code| !code.eq_ignore_ascii_case("CA"))
                .unwrap_or(false)
            {
                return None;
            }
            let street = p.street.or(p.name)?;
            let street = match p.housenumber {
                Some(number) => format!("{number} {street}"),
                None => street,
            };
            let label = [Some(street), p.city, p.state, p.postcode]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(", ");
            Some(AddressSuggestion {
                label,
                longitude: *feature.geometry.coordinates.first()?,
                latitude: *feature.geometry.coordinates.get(1)?,
            })
        })
        .collect();
    Ok(Json(matches))
}

// Each page loads a fingerprinted web bundle. Revalidate the HTML on navigation
// so an open browser does not keep selecting a previous release's bundle.
async fn prevent_stale_pages(request: axum::extract::Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    if response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("text/html"))
    {
        response
            .headers_mut()
            .insert(CACHE_CONTROL, "no-store".parse().unwrap());
    }
    response
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

async fn upload_task_image(
    State(db): State<Database>,
    auth: auth::AuthSession,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult<Json<String>> {
    let user = auth.user.ok_or((
        StatusCode::UNAUTHORIZED,
        "Sign in to upload task images.".into(),
    ))?;
    if !matches!(
        user.role.as_str(),
        "admin" | "staff" | "landlord" | "tenant"
    ) {
        return Err((
            StatusCode::FORBIDDEN,
            "Account cannot upload request images.".into(),
        ));
    }
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
    let mut client = db.lock().await;
    let tx = client.transaction().await.map_err(database_error)?;
    let image_id: i64 = tx
        .query_one(
            "INSERT INTO property_images(content_type,data) VALUES($1,$2) RETURNING id",
            &[&content_type, &body.as_ref()],
        )
        .await
        .map_err(database_error)?
        .get(0);
    tx.execute(
        "INSERT INTO task_image_uploads(image_id,owner_id) VALUES($1,$2)",
        &[&image_id, &user.id],
    )
    .await
    .map_err(database_error)?;
    tx.commit().await.map_err(database_error)?;
    Ok(Json(format!("/images/{image_id}")))
}

async fn get_image(
    State(db): State<Database>,
    auth: auth::AuthSession,
    Path(id): Path<i64>,
    Query(variant): Query<ImageVariant>,
) -> ApiResult<(HeaderMap, Vec<u8>)> {
    let client = db.lock().await;
    let path = format!("/images/{id}");
    let user_id = auth.user.map(|user| user.id).unwrap_or(0);
    let visibility = client.query_one(&format!("SELECT EXISTS(SELECT 1 FROM housework_tickets h WHERE $2=ANY(h.photos)), EXISTS(SELECT 1 FROM housework_tickets h WHERE $2=ANY(h.photos) AND (h.creator_id=$1 OR {}))", housework::manager_access_sql(1)), &[&user_id,&path]).await.map_err(database_error)?;
    let upload_owner = client
        .query_opt(
            "SELECT owner_id FROM task_image_uploads WHERE image_id=$1",
            &[&id],
        )
        .await
        .map_err(database_error)?
        .map(|row| row.get::<_, i64>(0));
    if (visibility.get::<_, bool>(0) || upload_owner.is_some())
        && !visibility.get::<_, bool>(1)
        && upload_owner != Some(user_id)
    {
        return Err((StatusCode::NOT_FOUND, "Image not found.".into()));
    }
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
    drop(client);
    if variant.thumb == Some(1) {
        let thumbnail = tokio::task::spawn_blocking(move || make_thumbnail(&data))
            .await
            .map_err(database_error)?
            .map_err(|_| {
                (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "Could not decode image.".into(),
                )
            })?;
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, "image/jpeg".parse().unwrap());
        headers.insert(CACHE_CONTROL, "private, max-age=604800".parse().unwrap());
        return Ok((headers, thumbnail));
    }
    let mut headers = HeaderMap::new();
    headers.insert(
        CONTENT_TYPE,
        content_type
            .parse()
            .map_err(|_| database_error("Invalid stored image type"))?,
    );
    Ok((headers, data))
}

#[derive(serde::Deserialize)]
struct ImageVariant {
    thumb: Option<u8>,
}

fn make_thumbnail(data: &[u8]) -> Result<Vec<u8>, image::ImageError> {
    let image = image::load_from_memory(data)?.thumbnail(240, 180);
    let mut output = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut output, 75).encode_image(&image)?;
    Ok(output)
}

#[cfg(test)]
mod image_tests {
    use super::make_thumbnail;
    use image::{DynamicImage, GenericImageView, ImageFormat, RgbaImage};
    use std::io::Cursor;

    #[test]
    fn thumbnail_reduces_large_transparent_cover_to_jpeg() {
        let source = DynamicImage::ImageRgba8(RgbaImage::from_pixel(
            1200,
            900,
            image::Rgba([40, 100, 80, 180]),
        ));
        let mut original = Cursor::new(Vec::new());
        source.write_to(&mut original, ImageFormat::Png).unwrap();
        let thumbnail = make_thumbnail(original.get_ref()).unwrap();
        assert!(thumbnail.starts_with(&[0xff, 0xd8]));
        assert_eq!(
            image::load_from_memory(&thumbnail).unwrap().dimensions(),
            (240, 180)
        );
    }
}

async fn load_properties(client: &Client) -> ApiResult<Json<Vec<Property>>> {
    let rows = client
        .query("SELECT p.data::text, p.landlord_id, COALESCE(l.legal_name, ''), COALESCE(l.email, '') FROM properties p LEFT JOIN landlords l ON l.id=p.landlord_id ORDER BY p.id", &[])
        .await
        .map_err(database_error)?;
    let mut properties: Vec<Property> = rows
        .into_iter()
        .map(|row| {
            let mut property = serde_json::from_str::<Property>(&row.get::<_, String>(0))
                .map_err(database_error)?;
            property.owner_id = row.get(1);
            property.owner_name = row.get(2);
            property.owner_email = row.get(3);
            Ok(property)
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
        property.ensure_external_names();
    }
    Ok(Json(properties))
}

async fn backfill_external_names(client: &Client) -> Result<(), Box<dyn std::error::Error>> {
    for row in client
        .query("SELECT id, data::text FROM properties", &[])
        .await?
    {
        let id: i64 = row.get(0);
        let mut property: Property = serde_json::from_str(&row.get::<_, String>(1))?;
        let property_missing = property.external_name.trim().is_empty();
        let room_rows = client
            .query(
                "SELECT id, data::text FROM property_spaces WHERE property_id=$1",
                &[&id],
            )
            .await?;
        let mut missing_rooms = Vec::new();
        for room_row in room_rows {
            let room_id: i64 = room_row.get(0);
            let room: Space = serde_json::from_str(&room_row.get::<_, String>(1))?;
            if room.external_name.trim().is_empty() {
                missing_rooms.push(room_id);
            }
            property.spaces.push(room);
        }
        property.ensure_external_names();
        if property_missing {
            client.execute("UPDATE properties SET data=jsonb_set(data,'{external_name}',to_jsonb($2::text)) WHERE id=$1", &[&id, &property.external_name]).await?;
        }
        for room_id in missing_rooms {
            if let Some(room) = property.spaces.iter().find(|room| {
                room.id as i64 == room_id && (room.kind == "Bedroom" || room.kind == "Suite")
            }) {
                client.execute("UPDATE property_spaces SET data=jsonb_set(data,'{external_name}',to_jsonb($3::text)) WHERE property_id=$1 AND id=$2", &[&id, &room_id, &room.external_name]).await?;
            }
        }
    }
    Ok(())
}

async fn list(State(db): State<Database>) -> ApiResult<Json<Vec<Property>>> {
    let client = db.lock().await;
    load_properties(&client).await
}

async fn task_properties(
    State(db): State<Database>,
    auth: auth::AuthSession,
) -> ApiResult<Json<Vec<Property>>> {
    let user = auth
        .user
        .ok_or((StatusCode::UNAUTHORIZED, "Sign in to view houses.".into()))?;
    let client = db.lock().await;
    let Json(mut properties) = load_properties(&client).await?;
    if user.role == "landlord" {
        let landlord_id: Option<i64> = client
            .query_one("SELECT landlord_id FROM users WHERE id=$1", &[&user.id])
            .await
            .map_err(database_error)?
            .get(0);
        properties
            .retain(|property| property.owner_id.is_some() && property.owner_id == landlord_id);
    }
    Ok(Json(properties))
}

async fn task_users(State(db): State<Database>) -> ApiResult<Json<Vec<serde_json::Value>>> {
    let rows = db.lock().await.query("SELECT id,first_name,last_name,role,photo_url FROM users WHERE active AND role IN ('admin','staff','landlord') ORDER BY first_name,last_name,id", &[]).await.map_err(database_error)?;
    Ok(Json(rows.into_iter().map(|row| serde_json::json!({
        "id":row.get::<_,i64>(0),"first_name":row.get::<_,String>(1),"last_name":row.get::<_,String>(2),
        "role":row.get::<_,String>(3),"active":true,"photo_url":row.get::<_,String>(4)
    })).collect()))
}

#[derive(serde::Serialize)]
struct StaffManager {
    id: i64,
    name: String,
    email: String,
    active: bool,
    photo_url: String,
}

async fn staff_managers(State(db): State<Database>) -> ApiResult<Json<Vec<StaffManager>>> {
    let rows = db.lock().await.query(
        "SELECT id, btrim(first_name || ' ' || last_name), email, active, photo_url FROM users WHERE role='staff' ORDER BY first_name,last_name,id",
        &[],
    ).await.map_err(database_error)?;
    Ok(Json(
        rows.into_iter()
            .map(|row| StaffManager {
                id: row.get(0),
                name: row.get(1),
                email: row.get(2),
                active: row.get(3),
                photo_url: row.get(4),
            })
            .collect(),
    ))
}

async fn guest_listings(State(db): State<Database>) -> ApiResult<Json<Vec<GuestListing>>> {
    let client = db.lock().await;
    let Json(properties) = load_properties(&client).await?;
    drop(client);
    let mut intersection_jobs = tokio::task::JoinSet::new();
    for property in &properties {
        if let (Some(latitude), Some(longitude)) = (property.latitude, property.longitude) {
            let id = property.id;
            intersection_jobs
                .spawn(async move { (id, intersection::nearest(latitude, longitude).await) });
        }
    }
    let mut intersections = std::collections::HashMap::new();
    while let Some(Ok((id, label))) = intersection_jobs.join_next().await {
        intersections.insert(id, label);
    }
    let client = db.lock().await;
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
    let occupancy_rows = client.query(
        "SELECT property_id, room_id, details->>'start_date', COALESCE(NULLIF(details->>'end_date',''),'9999-12-31') FROM leases WHERE status IN ('signed','confirmed') AND NOT is_demo AND details->>'start_date' IS NOT NULL",
        &[],
    ).await.map_err(database_error)?;
    let mut occupied: std::collections::HashMap<(u32, u32), Vec<OccupiedPeriod>> =
        std::collections::HashMap::new();
    for row in occupancy_rows {
        occupied
            .entry((row.get::<_, i64>(0) as u32, row.get::<_, i64>(1) as u32))
            .or_default()
            .push(OccupiedPeriod {
                start_date: row.get(2),
                end_date: row.get(3),
            });
    }
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
                    area: public_area(&property.address),
                    name: property.external_name.clone(),
                    nearby_intersection: intersections.get(&property.id).cloned().flatten(),
                    photos: property.photos,
                    amenities,
                    rooms: property
                        .spaces
                        .into_iter()
                        .filter(|space| space.kind == "Bedroom" || space.kind == "Suite")
                        .map(|space| {
                            let base = space
                                .current_monthly_rent
                                .or(space.rent_jan_apr)
                                .or(space.rent_sep_dec)
                                .or(space.rent_may_aug);
                            GuestRoom {
                                id: space.id,
                                name: space.external_name,
                                kind: space.kind.clone(),
                                bathroom_access: space.bathroom_access.clone(),
                                photos: space.photos,
                                monthly_price: base,
                                price_is_estimate: false,
                                rent_sep_dec: space.rent_sep_dec,
                                rent_jan_apr: space.rent_jan_apr,
                                rent_may_aug: space.rent_may_aug,
                                price_curve: space.price_curve,
                                monthly_price_overrides: space.monthly_price_overrides,
                                amenities: {
                                    let mut details = space.amenities.clone();
                                    if !space.bathroom_access.is_empty() {
                                        details.push(format!("{} bathroom", space.bathroom_access));
                                    }
                                    if space.kind == "Suite" {
                                        details.push("Private suite".into());
                                    }
                                    details
                                },
                                occupied_periods: occupied
                                    .get(&(property.id, 0))
                                    .into_iter()
                                    .flatten()
                                    .chain(
                                        occupied
                                            .get(&(property.id, space.id))
                                            .into_iter()
                                            .flatten(),
                                    )
                                    .cloned()
                                    .collect(),
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
            property.ensure_external_names();
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
    if property.external_name.chars().count() > 100
        || property
            .spaces
            .iter()
            .any(|space| space.external_name.chars().count() > 100)
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "Keep external names under 100 characters.".into(),
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
        for curve in [&space.price_curve] {
            let mut days = std::collections::HashSet::new();
            if (!curve.is_empty() && curve.len() < 2)
                || curve.len() > 12
                || curve.iter().any(|point| {
                    !(1..=12).contains(&point.month)
                        || !point.percent.is_finite()
                        || !(1.0..=500.0).contains(&point.percent)
                        || !days.insert(point.month)
                })
            {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "Use 2–12 distinct months and 1–500% for the price curve.".into(),
                ));
            }
        }
        let mut override_months = std::collections::HashSet::new();
        if space.monthly_price_overrides.len() > 120
            || space.monthly_price_overrides.iter().any(|item| {
                !(2020..=2100).contains(&item.year)
                    || !(1..=12).contains(&item.month)
                    || item.price == 0
                    || item.price > 100_000
                    || !override_months.insert((item.year, item.month))
            })
        {
            return Err((
                StatusCode::BAD_REQUEST,
                "Use valid, distinct monthly price overrides.".into(),
            ));
        }
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
        if [space.rent_sep_dec, space.rent_jan_apr, space.rent_may_aug]
            .into_iter()
            .flatten()
            .any(|rent| rent == 0)
        {
            return Err((
                StatusCode::BAD_REQUEST,
                "Seasonal rents must be greater than zero.".into(),
            ));
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
    for old_field in [
        "spaces",
        "kind",
        "ensuite_rooms",
        "shared_rooms",
        "suites",
        "owner_id",
        "owner_name",
        "owner_email",
    ] {
        core.as_object_mut().unwrap().remove(old_field);
    }
    let data = core.to_string();
    transaction
        .execute(
            "INSERT INTO properties (id, data, landlord_id) VALUES ($1, $2::text::jsonb, (SELECT landlord_id FROM landlord_defaults WHERE id=1)) \
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
    Json(mut property): Json<Property>,
) -> ApiResult<Json<Property>> {
    if id != property.id {
        return Err((StatusCode::BAD_REQUEST, "Property ID mismatch".into()));
    }
    property.ensure_external_names();
    validate(&property)?;
    let mut client = db.lock().await;
    let transaction = client.transaction().await.map_err(database_error)?;
    let is_new_property = transaction
        .query_opt(
            "SELECT 1 FROM properties WHERE id=$1",
            &[&(property.id as i64)],
        )
        .await
        .map_err(database_error)?
        .is_none();
    if property.manager_staff_ids.len() > 100
        || property.manager_staff_ids.iter().any(|id| *id <= 0)
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "Choose valid staff managers.".into(),
        ));
    }
    let selected: std::collections::HashSet<i64> =
        property.manager_staff_ids.iter().copied().collect();
    if selected.len() != property.manager_staff_ids.len() {
        return Err((
            StatusCode::BAD_REQUEST,
            "A manager was selected more than once.".into(),
        ));
    }
    let valid_staff: i64 = transaction
        .query_one(
            "SELECT COUNT(*) FROM users WHERE id=ANY($1::bigint[]) AND role='staff' AND active",
            &[&property.manager_staff_ids],
        )
        .await
        .map_err(database_error)?
        .get(0);
    if valid_staff as usize != selected.len() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Choose active staff members as managers.".into(),
        ));
    }
    let leased_rooms = transaction.query(
        "SELECT DISTINCT s.id FROM leases l JOIN property_spaces s ON s.property_id=l.property_id AND s.data->>'kind' IN ('Bedroom','Suite') AND (l.room_id=0 OR l.room_id=s.id) WHERE l.property_id = $1 AND l.status IN ('signed','confirmed') AND NOT l.is_demo",
        &[&(property.id as i64)],
    ).await.map_err(database_error)?;
    for row in leased_rooms {
        let room_id: i64 = row.get(0);
        if !property.spaces.iter().any(|room| room.id as i64 == room_id) {
            return Err((
                StatusCode::CONFLICT,
                "A signed lease uses this rental unit. Keep it in the property record.".into(),
            ));
        }
    }
    save_property(&transaction, &property).await?;
    if is_new_property {
        transaction.execute("INSERT INTO automation_events(property_id,event_type) VALUES($1,'property_created')", &[&(property.id as i64)]).await.map_err(database_error)?;
    }
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
        .batch_execute("UPDATE properties SET data = data - 'area' WHERE data ? 'area'")
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
    client.batch_execute("CREATE TABLE IF NOT EXISTS housework_tickets (
        id BIGSERIAL PRIMARY KEY,
        title TEXT NOT NULL,
        description TEXT NOT NULL DEFAULT '',
        property_id BIGINT REFERENCES properties(id) ON DELETE SET NULL,
        assignee TEXT NOT NULL DEFAULT '',
        status TEXT NOT NULL DEFAULT 'backlog' CHECK (status IN ('backlog','planned','in_progress','done','aborted')),
        start_date DATE,
        start_time TEXT,
        estimate_minutes INTEGER,
        created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
        updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
        CONSTRAINT housework_schedule_status_check CHECK ((status IN ('backlog','aborted') AND start_date IS NULL AND start_time IS NULL AND estimate_minutes IS NULL)
            OR (status = 'planned' AND ((start_date IS NULL AND start_time IS NULL AND estimate_minutes IS NULL)
                OR (start_date IS NOT NULL AND start_time IS NOT NULL AND estimate_minutes BETWEEN 1 AND 10080)))
            OR (status IN ('in_progress','done') AND start_date IS NOT NULL AND start_time IS NOT NULL AND estimate_minutes BETWEEN 1 AND 10080))
    ); ALTER TABLE housework_tickets ADD COLUMN IF NOT EXISTS photos TEXT[] NOT NULL DEFAULT '{}'; ALTER TABLE housework_tickets DROP COLUMN IF EXISTS priority; CREATE INDEX IF NOT EXISTS housework_schedule_idx ON housework_tickets(start_date,start_time) WHERE start_date IS NOT NULL;").await?;
    client.batch_execute(r#"DO $$
        DECLARE legacy_name text;
        BEGIN
            IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conrelid = 'housework_tickets'::regclass AND conname = 'housework_schedule_status_check') THEN
                SELECT conname INTO legacy_name FROM pg_constraint
                WHERE conrelid = 'housework_tickets'::regclass AND contype = 'c'
                  AND pg_get_constraintdef(oid) LIKE '%start_date IS NOT NULL%'
                  AND pg_get_constraintdef(oid) LIKE '%estimate_minutes%'
                LIMIT 1;
                IF legacy_name IS NOT NULL THEN
                    EXECUTE format('ALTER TABLE housework_tickets DROP CONSTRAINT %I', legacy_name);
                END IF;
                ALTER TABLE housework_tickets ADD CONSTRAINT housework_schedule_status_check CHECK
                    ((status IN ('backlog','aborted') AND start_date IS NULL AND start_time IS NULL AND estimate_minutes IS NULL)
                    OR (status = 'planned' AND ((start_date IS NULL AND start_time IS NULL AND estimate_minutes IS NULL)
                        OR (start_date IS NOT NULL AND start_time IS NOT NULL AND estimate_minutes BETWEEN 1 AND 10080)))
                    OR (status IN ('in_progress','done') AND start_date IS NOT NULL AND start_time IS NOT NULL AND estimate_minutes BETWEEN 1 AND 10080));
            END IF;
        END $$;"#).await?;
    client.batch_execute("CREATE TABLE IF NOT EXISTS catalog_seed_runs (name TEXT PRIMARY KEY);
        DO $$ BEGIN
            IF NOT EXISTS (SELECT 1 FROM catalog_seed_runs WHERE name='housework_mock_tickets_v1') THEN
                IF NOT EXISTS (SELECT 1 FROM housework_tickets) THEN
                    INSERT INTO housework_tickets (title,description,property_id,status) VALUES
                        ('Repair kitchen cabinet hinge','Adjust the upper cabinet hinge and check that the door closes evenly.',(SELECT id FROM properties WHERE data->>'name' ILIKE '%Dearborn%' LIMIT 1),'backlog'),
                        ('Inspect basement window seal','Check for drafts and photograph the window trim.',(SELECT id FROM properties WHERE data->>'name' ILIKE '%Dearborn%' LIMIT 1),'backlog');
                    INSERT INTO housework_tickets (title,description,property_id,status,start_date,start_time,estimate_minutes) VALUES
                        ('Replace hallway light fixture','Replace the fixture and confirm the switch works.',(SELECT id FROM properties WHERE data->>'name' ILIKE '%Dearborn%' LIMIT 1),'planned',CURRENT_DATE + 1,'10:00',60),
                        ('Deep clean common kitchen','Clean counters, appliances and cabinet fronts.',(SELECT id FROM properties WHERE data->>'name' ILIKE '%Dearborn%' LIMIT 1),'done',CURRENT_DATE,'09:00',120);
                END IF;
                INSERT INTO catalog_seed_runs (name) VALUES ('housework_mock_tickets_v1');
            END IF;
        END $$;").await?;
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
             ALTER TABLE tour_bookings ADD COLUMN IF NOT EXISTS manager_kind TEXT;
             ALTER TABLE tour_bookings ADD COLUMN IF NOT EXISTS manager_id BIGINT;
             ALTER TABLE tour_bookings ADD COLUMN IF NOT EXISTS manager_notice_error TEXT;
             DO $$ BEGIN
               IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_schema=current_schema() AND table_name='tour_bookings' AND column_name='manager_notice_sent_at') THEN
                 ALTER TABLE tour_bookings ADD COLUMN manager_notice_sent_at TIMESTAMPTZ;
                 UPDATE tour_bookings SET manager_notice_sent_at=NOW(), manager_notice_error='legacy';
               END IF;
             END $$;
             ALTER TABLE tour_bookings ADD COLUMN IF NOT EXISTS manager_notice_claimed_at TIMESTAMPTZ;
             ALTER TABLE tour_bookings ADD COLUMN IF NOT EXISTS manager_notice_attempts INTEGER NOT NULL DEFAULT 0;
             ALTER TABLE tour_bookings ADD COLUMN IF NOT EXISTS cancelled_at TIMESTAMPTZ;
             ALTER TABLE tour_bookings ADD COLUMN IF NOT EXISTS guest_token TEXT NOT NULL DEFAULT gen_random_uuid()::text;
             ALTER TABLE tour_bookings ADD COLUMN IF NOT EXISTS room_id BIGINT;
             ALTER TABLE tour_bookings ADD COLUMN IF NOT EXISTS room_name TEXT;
             ALTER TABLE tour_bookings ADD COLUMN IF NOT EXISTS rental_start TEXT;
             ALTER TABLE tour_bookings ADD COLUMN IF NOT EXISTS rental_year INTEGER;
             ALTER TABLE tour_bookings ADD COLUMN IF NOT EXISTS rental_months INTEGER;
             ALTER TABLE tour_bookings ADD COLUMN IF NOT EXISTS quoted_monthly_rent INTEGER;
             ALTER TABLE tour_bookings ADD COLUMN IF NOT EXISTS quote_is_estimate BOOLEAN NOT NULL DEFAULT TRUE;
             ALTER TABLE tour_bookings ADD COLUMN IF NOT EXISTS guest_email TEXT NOT NULL DEFAULT '';
             ALTER TABLE tour_bookings ADD COLUMN IF NOT EXISTS legal_first_name TEXT;
             ALTER TABLE tour_bookings ADD COLUMN IF NOT EXISTS legal_last_name TEXT;
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
    client.batch_execute(
        "CREATE TABLE IF NOT EXISTS users (
            id BIGSERIAL PRIMARY KEY,
            first_name TEXT NOT NULL DEFAULT '',
            last_name TEXT NOT NULL DEFAULT '',
            email TEXT NOT NULL UNIQUE,
            phone TEXT NOT NULL DEFAULT '',
            role TEXT NOT NULL CHECK (role IN ('admin', 'tenant', 'guest')),
            password_hash TEXT NOT NULL,
            active BOOLEAN NOT NULL DEFAULT TRUE
        );
        ALTER TABLE users ADD COLUMN IF NOT EXISTS first_name TEXT NOT NULL DEFAULT '';
        ALTER TABLE users ADD COLUMN IF NOT EXISTS last_name TEXT NOT NULL DEFAULT '';
        ALTER TABLE users ADD COLUMN IF NOT EXISTS photo_url TEXT NOT NULL DEFAULT '';
        UPDATE users SET
            first_name = split_part(btrim(regexp_replace(to_jsonb(users)->>'name', '^Demo · ', '')), ' ', 1),
            last_name = btrim(substr(btrim(regexp_replace(to_jsonb(users)->>'name', '^Demo · ', '')), length(split_part(btrim(regexp_replace(to_jsonb(users)->>'name', '^Demo · ', '')), ' ', 1)) + 1))
        WHERE to_jsonb(users) ? 'name' AND btrim(first_name) = '' AND btrim(last_name) = '';
        ALTER TABLE users DROP COLUMN IF EXISTS name;
        INSERT INTO users (first_name, email, role, password_hash)
            SELECT 'Administrator', 'admin@buildry.local', 'admin', password_hash FROM admin_users WHERE id = 1
            ON CONFLICT (email) DO NOTHING;
        DROP TABLE IF EXISTS tenant_assignments;
        UPDATE property_spaces AS space SET data =
            space.data - 'availability_confirmed' - 'unavailable_periods'
        WHERE space.data ? 'availability_confirmed' OR space.data ? 'unavailable_periods';"
    ).await?;
    client
        .batch_execute("UPDATE property_spaces SET data = data - 'level' WHERE data ? 'level';")
        .await?;
    client.batch_execute("ALTER TABLE users DROP CONSTRAINT IF EXISTS users_role_check;
        ALTER TABLE users ADD CONSTRAINT users_role_check CHECK (role IN ('admin', 'staff', 'landlord', 'tenant', 'guest'));").await?;
    leasing::migrate(&client).await?;
    photo_id::migrate(&client).await?;
    client.batch_execute("ALTER TABLE users ADD COLUMN IF NOT EXISTS landlord_id BIGINT REFERENCES landlords(id);
        ALTER TABLE users ADD COLUMN IF NOT EXISTS is_seeded BOOLEAN NOT NULL DEFAULT FALSE;
        ALTER TABLE users ADD COLUMN IF NOT EXISTS google_sub TEXT UNIQUE;
        ALTER TABLE users ADD COLUMN IF NOT EXISTS auth_version BIGINT NOT NULL DEFAULT 0;
        CREATE TABLE IF NOT EXISTS google_login_states (
            state_hash TEXT PRIMARY KEY,
            user_id BIGINT REFERENCES users(id) ON DELETE CASCADE,
            next_path TEXT,
            expires_at TIMESTAMPTZ NOT NULL
        );
        UPDATE users SET is_seeded=TRUE WHERE email LIKE '%@example.invalid' AND role IN ('guest','tenant');
        UPDATE users SET is_seeded=TRUE WHERE role='landlord' AND email='helen.bai@example.invalid';").await?;
    client.batch_execute("ALTER TABLE housework_tickets ADD COLUMN IF NOT EXISTS creator_id BIGINT REFERENCES users(id) ON DELETE SET NULL;
        DROP INDEX IF EXISTS housework_viewers_idx;
        ALTER TABLE housework_tickets DROP COLUMN IF EXISTS viewer_ids;
        CREATE TABLE IF NOT EXISTS task_image_uploads (image_id BIGINT PRIMARY KEY REFERENCES property_images(id) ON DELETE CASCADE, owner_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE);
        CREATE TABLE IF NOT EXISTS tenant_maintenance_requests (task_id BIGINT PRIMARY KEY REFERENCES housework_tickets(id) ON DELETE CASCADE, tenant_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE);
        CREATE INDEX IF NOT EXISTS tenant_maintenance_requests_tenant_idx ON tenant_maintenance_requests(tenant_id);
        DO $$ BEGIN
            IF NOT EXISTS (SELECT 1 FROM catalog_seed_runs WHERE name='housework_creator_backfill_v1') THEN
                UPDATE housework_tickets SET creator_id=(SELECT id FROM users WHERE role='admin' AND active ORDER BY id LIMIT 1) WHERE creator_id IS NULL;
                INSERT INTO catalog_seed_runs(name) VALUES ('housework_creator_backfill_v1');
            END IF;
        END $$;
        CREATE INDEX IF NOT EXISTS housework_creator_idx ON housework_tickets(creator_id);").await?;
    automations::migrate(&client).await?;
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
    mcp::migrate(&client).await?;
    backfill_external_names(&client).await?;
    avatar::backfill_user_avatars(&client).await?;
    let db = Arc::new(Mutex::new(client));
    if env::args().nth(1).as_deref() == Some("seed-demo-tenants") {
        db.lock()
            .await
            .batch_execute(
                "INSERT INTO users (first_name,last_name,email,phone,role,password_hash,active,is_seeded) VALUES
            ('Jordan','Lee','demo.jordan@example.invalid','519-555-0101','tenant','',TRUE,TRUE),
            ('Priya','Shah','demo.priya@example.invalid','519-555-0102','tenant','',TRUE,TRUE),
            ('Morgan','Chen','demo.morgan@example.invalid','519-555-0103','tenant','',TRUE,TRUE),
            ('Avery','Nguyen','demo.avery@example.invalid','519-555-0104','tenant','',TRUE,TRUE),
            ('Noah','Patel','demo.noah@example.invalid','519-555-0105','tenant','',TRUE,TRUE),
            ('Samira','Ali','demo.samira@example.invalid','519-555-0106','tenant','',TRUE,TRUE),
            ('Maya','Singh','demo.maya@example.invalid','519-555-0107','tenant','',TRUE,TRUE),
            ('Zara','Ahmed','demo.zara@example.invalid','519-555-0108','tenant','',TRUE,TRUE),
            ('Grace','Kim','demo.grace@example.invalid','519-555-0109','tenant','',TRUE,TRUE)
            ON CONFLICT (email) DO NOTHING;",
            )
            .await?;
        println!("Demo tenants are available in Tenants and Access management.");
        return Ok(());
    }
    if env::args().nth(1).as_deref() == Some("seed-demo-guests") {
        db.lock()
            .await
            .batch_execute(
                "INSERT INTO users (first_name,last_name,email,phone,role,password_hash,active,is_seeded) VALUES
            ('Sofia','Rivera','demo.sofia@example.invalid','519-555-0201','guest','',TRUE,TRUE),
            ('Daniel','Wu','demo.daniel@example.invalid','519-555-0202','guest','',TRUE,TRUE),
            ('Isabella','Brown','demo.isabella@example.invalid','519-555-0203','guest','',TRUE,TRUE),
            ('Harper','Davis','demo.harper@example.invalid','519-555-0204','guest','',TRUE,TRUE),
            ('Leo','Garcia','demo.leo@example.invalid','519-555-0205','guest','',TRUE,TRUE),
            ('Fatima','Hassan','demo.fatima@example.invalid','519-555-0206','guest','',TRUE,TRUE),
            ('Ethan','Clarke','demo.ethan@example.invalid','519-555-0207','guest','',TRUE,TRUE),
            ('Chloe','Bennett','demo.chloe@example.invalid','519-555-0208','guest','',TRUE,TRUE)
            ON CONFLICT (email) DO NOTHING;",
            )
            .await?;
        println!("Demo guests are available in Access management.");
        return Ok(());
    }
    if env::args().nth(1).as_deref() == Some("backfill-missing-tour-emails") {
        let updated = db.lock().await.execute(
            "UPDATE tour_bookings SET guest_email='tour-' || id || '@example.invalid' WHERE btrim(guest_email)=''",
            &[],
        ).await?;
        println!("Backfilled {updated} tour bookings with non-deliverable example.invalid email addresses.");
        return Ok(());
    }
    if env::args().nth(1).as_deref() == Some("backfill-helen-contact") {
        let updated = db.lock().await.execute(
            "UPDATE landlords SET
                service_address=CASE WHEN btrim(service_address)='' THEN '100 Example Street, Waterloo, ON N2L 0A0' ELSE service_address END,
                email=CASE WHEN btrim(email)='' THEN 'helen.bai@example.invalid' ELSE email END,
                phone=CASE WHEN btrim(phone)='' THEN '519-555-0148' ELSE phone END,
                signer_name=CASE WHEN btrim(signer_name)='' THEN 'Helen Bai' ELSE signer_name END,
                signer_title=CASE WHEN btrim(signer_title)='' THEN 'Owner' ELSE signer_title END
             WHERE lower(legal_name)='helen bai'",
            &[],
        ).await?;
        println!(
            "Updated {updated} Helen Bai landlord record(s). Signing authorization was unchanged."
        );
        return Ok(());
    }
    if env::args().nth(1).as_deref() == Some("backfill-helen-access") {
        let added=db.lock().await.execute("INSERT INTO users (first_name,last_name,email,phone,role,password_hash,active,landlord_id,is_seeded)
            SELECT split_part(btrim(legal_name),' ',1),btrim(substr(btrim(legal_name),length(split_part(btrim(legal_name),' ',1))+1)),email,phone,'landlord','',TRUE,id,TRUE FROM landlords
            WHERE lower(legal_name)='helen bai' AND lower(email)='helen.bai@example.invalid'
            ON CONFLICT (email) DO NOTHING",&[]).await?;
        println!("Added {added} pending Helen Bai landlord account(s). No setup email was sent.");
        return Ok(());
    }
    if env::args().nth(1).as_deref() == Some("email-smoke-test") {
        let recipient = env::args()
            .nth(2)
            .unwrap_or_else(|| "delivered@resend.dev".into());
        leasing::email_smoke_test(&recipient)
            .await
            .map_err(|(status, message)| {
                std::io::Error::other(format!("Email smoke test failed ({status}): {message}"))
            })?;
        println!("Resend accepted the Buildry test email for {recipient}.");
        return Ok(());
    }
    if env::args().nth(1).as_deref() == Some("set-admin") {
        auth::create_admin(&db).await?;
        return Ok(());
    }
    tours::start_manager_notice_worker(db.clone());
    automations::start_worker(db.clone());
    let retention_db = db.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(24 * 60 * 60));
        loop {
            interval.tick().await;
            if let Err(error) = photo_id::delete_expired(&retention_db).await {
                eprintln!("Photo ID retention cleanup failed: {error}");
            }
        }
    });
    let app_dist = env::var("APP_DIST")
        .unwrap_or_else(|_| "../target/dx/buildry-property-catalog/debug/web/public".into());
    let index = format!("{app_dist}/index.html");
    let revision_index = index.clone();
    let admin_api = Router::new()
        .route("/api/address-suggestions", get(address_suggestions))
        .route("/api/properties", get(list))
        .route("/api/property-managers/staff", get(staff_managers))
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
        .route("/api/tenants", get(tenants::list))
        .route(
            "/api/landlords",
            get(leasing::list_landlords).post(leasing::create_landlord),
        )
        .route("/api/email/status", get(leasing::email_status))
        .route("/api/landlords/{id}", put(leasing::save_landlord))
        .route(
            "/api/leases",
            get(leasing::list_leases).post(leasing::create_lease),
        )
        .route("/api/lease-templates", get(leasing::list_templates))
        .route("/api/lease-templates/{id}", put(leasing::save_template))
        .route(
            "/api/lease-templates/{id}/additional-pdf",
            put(leasing::upload_template_pdf)
                .delete(leasing::remove_template_pdf)
                .layer(axum::extract::DefaultBodyLimit::max(5_000_000)),
        )
        .route(
            "/api/lease-templates/{id}/email",
            post(leasing::email_template_pdf),
        )
        .route(
            "/api/lease-templates/{id}/send",
            post(leasing::send_template_to_landlord),
        )
        .route("/api/tours/{id}/lease", post(leasing::create_from_tour))
        .route("/api/prospects/{id}/lease", post(leasing::create_from_tour))
        .route(
            "/api/tours/{id}/leasing-info",
            put(tours::update_leasing_info),
        )
        .route("/api/leases/{id}", put(leasing::update_lease))
        .route("/api/leases/{id}/template", post(leasing::apply_template))
        .route("/api/leases/{id}/send", post(leasing::send_lease))
        .route(
            "/api/leases/{id}/landlord-link",
            post(leasing::resend_landlord_signing_link),
        )
        .route("/api/leases/{id}/confirm", post(leasing::confirm_lease))
        .route(
            "/api/leases/{id}/photo-id-link",
            post(leasing::resend_photo_id_link),
        )
        .route(
            "/api/leases/{id}/tenancy-end",
            post(leasing::record_tenancy_end),
        )
        .route(
            "/api/leases/{id}/resend-confirmation",
            post(leasing::resend_confirmation),
        )
        .route("/api/leases/{id}/document", get(lease_pdf::admin_document))
        .route(
            "/api/leases/{id}/photo-id/{side}",
            get(photo_id::admin_image),
        )
        .route("/api/leases/{id}/photo-id/approve", post(photo_id::approve))
        .route("/api/leases/{id}/photo-id/reject", post(photo_id::reject))
        .route_layer(middleware::from_fn(auth::require_admin_api));
    let automation_admin_api = Router::new()
        .route(
            "/api/automations/templates",
            get(automations::list_templates).post(automations::create_template),
        )
        .route(
            "/api/automations/templates/{id}",
            put(automations::save_template),
        )
        .route_layer(middleware::from_fn(auth::require_access_api));
    let task_api = Router::new()
        .route("/api/tasks", get(housework::list).post(housework::create))
        .route(
            "/api/tasks/{id}",
            put(housework::update).delete(housework::delete),
        )
        .route("/api/tasks/properties", get(task_properties))
        .route("/api/tasks/users", get(task_users))
        .route(
            "/api/properties/{id}/automations",
            get(automations::property_presets),
        )
        .route(
            "/api/properties/{id}/automations/{template_id}",
            put(automations::set_property_preset),
        )
        .route(
            "/api/properties/{id}/automation-history",
            get(automations::history),
        )
        .route(
            "/api/properties/{id}/automation-events",
            post(automations::record_event),
        )
        .route(
            "/api/tasks/images",
            post(upload_task_image).layer(DefaultBodyLimit::max(5_000_000)),
        )
        .route(
            "/api/housework",
            get(housework::list).post(housework::create),
        )
        .route("/api/housework/{id}", put(housework::update))
        .route_layer(middleware::from_fn(auth::require_task_api));
    let access_api = Router::new()
        .route(
            "/api/access/users",
            get(leasing::users).post(leasing::create_user),
        )
        .route("/api/access/users/{id}", put(leasing::update_user))
        .route("/api/access/users/{id}/invite", post(leasing::invite_user))
        .route_layer(middleware::from_fn(auth::require_access_api));
    let admin_pages = Router::new()
        .route_service("/", ServeFile::new(index.clone()))
        .route_service("/index.html", ServeFile::new(index.clone()))
        .route_service("/tours", ServeFile::new(index.clone()))
        .route_service("/calendar", ServeFile::new(index.clone()))
        .route_service("/vacancy", ServeFile::new(index.clone()))
        .route(
            "/availability",
            get(|| async { axum::response::Redirect::to("/vacancy") }),
        )
        .route(
            "/tenants",
            get(|| async { axum::response::Redirect::to("/leases") }),
        )
        .route(
            "/landlord",
            get(|| async { axum::response::Redirect::to("/access") }),
        )
        .route_service("/leases", ServeFile::new(index.clone()))
        .route_service("/leases/search", ServeFile::new(index.clone()))
        .route_service("/leases/new", ServeFile::new(index.clone()))
        .route_service("/leases/view", ServeFile::new(index.clone()))
        .route(
            "/leases/landlords/new",
            get(|| async { axum::response::Redirect::to("/access/new") }),
        )
        .route(
            "/leases/landlords/view",
            get(
                |Query(params): Query<std::collections::HashMap<String, String>>| async move {
                    if let Some(id) = params.get("id").and_then(|value| value.parse::<i64>().ok()) {
                        axum::response::Redirect::to(&format!("/access/landlords/view?id={id}"))
                    } else {
                        axum::response::Redirect::to("/access/landlords")
                    }
                },
            ),
        )
        .route_service("/leases/templates", ServeFile::new(index.clone()))
        .route_service("/leases/templates/search", ServeFile::new(index.clone()))
        .route_service("/leases/templates/new", ServeFile::new(index.clone()))
        .route_service("/pricing", ServeFile::new(index.clone()))
        .route_service("/properties/search", ServeFile::new(index.clone()))
        .route_service("/properties/{id}", ServeFile::new(index.clone()))
        .route_service("/automations", ServeFile::new(index.clone()))
        .route_layer(middleware::from_fn(auth::require_admin_page));
    let task_pages = Router::new()
        .route_service("/my-properties", ServeFile::new(index.clone()))
        .route_service("/my-properties/{id}", ServeFile::new(index.clone()))
        .route_service("/tasks", ServeFile::new(index.clone()))
        .route_service("/tasks/search", ServeFile::new(index.clone()))
        .route_service("/tasks/new", ServeFile::new(index.clone()))
        .route_service("/tasks/{id}", ServeFile::new(index.clone()))
        .route_service(
            "/properties/{id}/tasks/automatic-maintenance",
            ServeFile::new(index.clone()),
        )
        .route_service("/housework", ServeFile::new(index.clone()))
        .route_service("/housework/new", ServeFile::new(index.clone()))
        .route_service("/housework/ticket/{id}", ServeFile::new(index.clone()))
        .route_layer(middleware::from_fn(auth::require_task_page));
    let access_page = Router::new()
        .route_service("/access", ServeFile::new(index.clone()))
        .route_service("/access/search", ServeFile::new(index.clone()))
        .route_service("/access/new", ServeFile::new(index.clone()))
        .route_service("/access/view", ServeFile::new(index.clone()))
        .route_service("/access/landlords", ServeFile::new(index.clone()))
        .route_service("/access/landlords/new", ServeFile::new(index.clone()))
        .route_service("/access/landlords/view", ServeFile::new(index.clone()))
        .route_layer(middleware::from_fn(auth::require_access_page));
    let tenant_page = Router::new()
        .route_service("/my-home", ServeFile::new(index.clone()))
        .route_service("/my-home/account", ServeFile::new(index.clone()))
        .route_service("/my-home/requests", ServeFile::new(index.clone()))
        .route_service("/my-home/requests/search", ServeFile::new(index.clone()))
        .route_service("/my-home/requests/new", ServeFile::new(index.clone()))
        .route_service("/my-home/requests/{id}", ServeFile::new(index.clone()))
        .route_layer(middleware::from_fn(auth::require_tenant_page));
    let session_pool = PgPool::connect(&url).await?;
    let session_store = PostgresStore::new(session_pool);
    session_store.migrate().await?;
    let session_layer = SessionManagerLayer::new(session_store)
        .with_secure(env::var("APP_SECURE_COOKIES").as_deref() == Ok("1"))
        .with_same_site(SameSite::Lax)
        .with_expiry(Expiry::OnInactivity(Duration::days(7)))
        .with_signed(signing_key);
    let auth_layer =
        AuthManagerLayerBuilder::new(auth::Backend::new(db.clone()), session_layer).build();
    let app = Router::new()
        .merge(mcp::routes())
        .merge(admin_api)
        .merge(automation_admin_api)
        .merge(task_api)
        .merge(access_api)
        .merge(admin_pages)
        .merge(task_pages)
        .merge(access_page)
        .merge(tenant_page)
        .route("/login", get(auth::login_page).post(auth::login))
        .route(
            "/privacy",
            get(|| async { axum::response::Html(include_str!("privacy.html")) }),
        )
        .route("/auth/google/start", get(google_auth::start))
        .route("/auth/google/link", get(google_auth::link))
        .route("/auth/google/callback", get(google_auth::callback))
        .route("/signup", get(auth::signup_page).post(auth::signup))
        .route(
            "/forgot-password",
            get(auth::forgot_password_page).post(auth::forgot_password),
        )
        .route(
            "/resend-verification",
            get(auth::resend_verification_page).post(auth::resend_verification),
        )
        .route(
            "/verify-email/{token}",
            get(leasing::verify_email_page).post(leasing::verify_email),
        )
        .route("/guest-home", get(auth::guest_home))
        .route("/landlord-home", get(auth::landlord_home))
        .route(
            "/set-password/{token}",
            get(leasing::set_password_page).post(leasing::set_password),
        )
        .route(
            "/lease/{token}",
            get(leasing::lease_page).post(leasing::sign_lease),
        )
        .route(
            "/lease/{token}/photo-id",
            get(photo_id::upload_page)
                .post(photo_id::submit)
                .layer(DefaultBodyLimit::max(11_000_000)),
        )
        .route("/photo-id-terms", get(photo_id::terms_page))
        .route(
            "/lease-template/{token}",
            get(leasing::template_signing_page).post(leasing::sign_template),
        )
        .route(
            "/lease-template/{token}/document",
            get(lease_pdf::template_document),
        )
        .route("/lease/{token}/document", get(lease_pdf::guest_document))
        .route(
            "/lease-landlord/{token}",
            get(leasing::landlord_lease_page).post(leasing::sign_landlord_lease),
        )
        .route(
            "/lease-landlord/{token}/document",
            get(lease_pdf::landlord_document),
        )
        .route("/logout", post(auth::logout))
        .route("/api/me", get(auth::me))
        .route("/api/change-password", post(auth::change_password))
        .route("/api/my-tenancies", get(tenants::my_tenancies))
        .route(
            "/api/my-maintenance-requests",
            get(tenants::list_maintenance_requests).post(tenants::create_maintenance_request),
        )
        .route(
            "/api/my-maintenance-requests/{id}",
            get(tenants::get_maintenance_request),
        )
        .route(
            "/api/my-maintenance-requests/images",
            post(upload_task_image).layer(DefaultBodyLimit::max(5_000_000)),
        )
        .route("/api/tours/options", get(tours::options))
        .route("/api/guest-listings", get(guest_listings))
        .route("/api/guest-place-search", get(guest_place_search))
        .route(
            "/api/ui-revision",
            get(move || {
                let index = revision_index.clone();
                async move {
                    match tokio::fs::read(index).await {
                        Ok(html) => (
                            StatusCode::OK,
                            [(CACHE_CONTROL, "no-store")],
                            format!("{:x}", Sha256::digest(html)),
                        ),
                        Err(_) => (
                            StatusCode::SERVICE_UNAVAILABLE,
                            [(CACHE_CONTROL, "no-store")],
                            String::new(),
                        ),
                    }
                }
            }),
        )
        .route_service("/assets/main.css", ServeFile::new("../assets/main.css"))
        .route_service(
            "/assets/stay-house-modern.jpg",
            ServeFile::new("../assets/stay-house-modern.jpg"),
        )
        .route_service(
            "/assets/main-20261016-ripple.css",
            ServeFile::new("../assets/main.css"),
        )
        .route_service(
            "/assets/main-20261016-shell.css",
            ServeFile::new("../assets/main.css"),
        )
        .route_service(
            "/assets/main-20261012c.css",
            ServeFile::new("../assets/main.css"),
        )
        .route_service(
            "/assets/main-20261012d.css",
            ServeFile::new("../assets/main.css"),
        )
        .route_service(
            "/assets/main-20261012e.css",
            ServeFile::new("../assets/main.css"),
        )
        .route_service(
            "/assets/main-20261017.css",
            ServeFile::new("../assets/main.css"),
        )
        .route_service(
            "/assets/main-20261017b.css",
            ServeFile::new("../assets/main.css"),
        )
        .route_service(
            "/assets/main-20261017c.css",
            ServeFile::new("../assets/main.css"),
        )
        .route_service(
            "/assets/main-20261017d.css",
            ServeFile::new("../assets/main.css"),
        )
        .route_service(
            "/assets/lease-viewer.mjs",
            ServeFile::new("../assets/lease-viewer.mjs"),
        )
        .route_service(
            "/assets/tour-detail-swipe.mjs",
            ServeFile::new("../assets/tour-detail-swipe.mjs"),
        )
        .route_service(
            "/assets/lease-viewer-20261012c.mjs",
            ServeFile::new("../assets/lease-viewer.mjs"),
        )
        .route_service(
            "/assets/lease-viewer-20261017.mjs",
            ServeFile::new("../assets/lease-viewer.mjs"),
        )
        .route_service(
            "/assets/lease-viewer-20261017b.mjs",
            ServeFile::new("../assets/lease-viewer.mjs"),
        )
        .route_service(
            "/assets/signing-flow.mjs",
            ServeFile::new("../assets/signing-flow.mjs"),
        )
        .route_service(
            "/assets/signing-flow-20261012c.mjs",
            ServeFile::new("../assets/signing-flow.mjs"),
        )
        .route_service(
            "/assets/photo-id.mjs",
            ServeFile::new("../assets/photo-id.mjs"),
        )
        .route_service(
            "/assets/fonts/Allura-Regular.ttf",
            ServeFile::new("../assets/fonts/Allura-Regular.ttf"),
        )
        .route_service(
            "/assets/pdfjs/pdf.mjs",
            ServeFile::new("../assets/pdfjs/pdf.mjs"),
        )
        .route_service(
            "/assets/pdfjs/pdf.worker.mjs",
            ServeFile::new("../assets/pdfjs/pdf.worker.mjs"),
        )
        .route_service(
            "/assets/pdfjs/pdf.worker-compat.mjs",
            ServeFile::new("../assets/pdfjs/pdf.worker-compat.mjs"),
        )
        .route(
            "/assets/forms/ontario-standard-lease-2229e.pdf",
            get(lease_pdf::original_form),
        )
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
        .layer(auth_layer)
        .layer(middleware::from_fn(prevent_stale_pages));
    let bind = env::var("APP_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".into());
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    println!("Property catalog and API listening on http://{bind}");
    axum::serve(listener, app).await?;
    Ok(())
}
