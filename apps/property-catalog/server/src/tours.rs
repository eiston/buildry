use super::{database_error, ApiResult, Database};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::NaiveDate;
use serde::Deserialize;
use tokio_postgres::error::SqlState;
#[path = "../../src/tour_model.rs"]
#[allow(dead_code)]
mod tour_model;
use tour_model::{
    windows_overlap, Availability, GuestBooking, GuestBookingStatus, NewTour, RescheduleTour,
    TourBooking, TourHouse, TourOptions, TOUR_SLOTS,
};

fn bad_request(message: &str) -> (StatusCode, String) {
    (StatusCode::BAD_REQUEST, message.into())
}

async fn booking_window(client: &tokio_postgres::Client) -> ApiResult<(String, String, String)> {
    let row = client
        .query_one(
            "SELECT (NOW() AT TIME ZONE 'America/Toronto')::date::text, \
                    ((NOW() AT TIME ZONE 'America/Toronto')::date + 60)::text, \
                    to_char(NOW() AT TIME ZONE 'America/Toronto', 'HH24:MI')",
            &[],
        )
        .await
        .map_err(database_error)?;
    Ok((row.get(0), row.get(1), row.get(2)))
}

fn valid_date(date: &str, today: &str, last_date: &str) -> bool {
    NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .is_ok_and(|parsed| parsed.format("%Y-%m-%d").to_string() == date)
        && date >= today
        && date <= last_date
}

pub async fn options(State(db): State<Database>) -> ApiResult<Json<TourOptions>> {
    let client = db.lock().await;
    let (today, last_date, now_time) = booking_window(&client).await?;
    let rows = client
        .query(
            "SELECT p.id, p.data->>'name', p.data->>'address' FROM properties p \
                LEFT JOIN tour_house_settings s ON s.property_id = p.id \
                WHERE COALESCE(s.available, TRUE) ORDER BY p.id",
            &[],
        )
        .await
        .map_err(database_error)?;
    let houses = rows
        .into_iter()
        .map(|row| TourHouse {
            id: row.get::<_, i64>(0) as u32,
            name: row.get(1),
            address: row.get(2),
            available: true,
        })
        .collect();
    Ok(Json(TourOptions {
        houses,
        today,
        last_date,
        now_time,
    }))
}

pub async fn houses(State(db): State<Database>) -> ApiResult<Json<Vec<TourHouse>>> {
    let client = db.lock().await;
    let rows = client
        .query(
            "SELECT p.id, p.data->>'name', p.data->>'address', COALESCE(s.available, TRUE) \
         FROM properties p LEFT JOIN tour_house_settings s ON s.property_id = p.id ORDER BY p.id",
            &[],
        )
        .await
        .map_err(database_error)?;
    Ok(Json(
        rows.into_iter()
            .map(|row| TourHouse {
                id: row.get::<_, i64>(0) as u32,
                name: row.get(1),
                address: row.get(2),
                available: row.get(3),
            })
            .collect(),
    ))
}

#[derive(Deserialize)]
pub struct HouseAvailabilityUpdate {
    available: bool,
}

pub async fn set_house_availability(
    Path(id): Path<u32>,
    State(db): State<Database>,
    Json(request): Json<HouseAvailabilityUpdate>,
) -> ApiResult<Json<TourHouse>> {
    let client = db.lock().await;
    let row = client
        .query_opt(
            "SELECT data->>'name', data->>'address' FROM properties WHERE id = $1",
            &[&(id as i64)],
        )
        .await
        .map_err(database_error)?
        .ok_or((StatusCode::NOT_FOUND, "House not found.".into()))?;
    client
        .execute(
            "INSERT INTO tour_house_settings (property_id, available) VALUES ($1, $2) \
         ON CONFLICT (property_id) DO UPDATE SET available = EXCLUDED.available",
            &[&(id as i64), &request.available],
        )
        .await
        .map_err(database_error)?;
    Ok(Json(TourHouse {
        id,
        name: row.get(0),
        address: row.get(1),
        available: request.available,
    }))
}

#[derive(Deserialize)]
pub struct AvailabilityQuery {
    property_id: u32,
    date: String,
    exclude_booking_id: Option<i64>,
}

pub async fn availability(
    State(db): State<Database>,
    Query(query): Query<AvailabilityQuery>,
) -> ApiResult<Json<Availability>> {
    let client = db.lock().await;
    let (today, last_date, now_time) = booking_window(&client).await?;
    if !valid_date(&query.date, &today, &last_date) {
        return Err(bad_request("Choose a date within the next 60 days."));
    }
    let exists: bool = client
        .query_one(
            "SELECT EXISTS(SELECT 1 FROM properties p \
             LEFT JOIN tour_house_settings s ON s.property_id = p.id \
             WHERE p.id = $1 AND COALESCE(s.available, TRUE))",
            &[&(query.property_id as i64)],
        )
        .await
        .map_err(database_error)?
        .get(0);
    if !exists {
        return Err(bad_request("This house is not available for tours."));
    }
    let booked: Vec<String> = client
        .query(
            "SELECT slot_start FROM tour_bookings WHERE tour_date = $1::text::date \
             AND cancelled_at IS NULL AND id IS DISTINCT FROM $2 \
             AND (NOT is_demo OR EXISTS \
               (SELECT 1 FROM tour_bookings original WHERE original.id = $2 AND original.is_demo))",
            &[&query.date, &query.exclude_booking_id],
        )
        .await
        .map_err(database_error)?
        .into_iter()
        .map(|row| row.get(0))
        .collect();
    let available_slots = TOUR_SLOTS
        .iter()
        .filter(|(slot, _)| !booked.iter().any(|taken| windows_overlap(slot, taken)))
        .filter(|(slot, _)| query.date != today || *slot > now_time.as_str())
        .map(|(slot, _)| (*slot).to_string())
        .collect();
    Ok(Json(Availability { available_slots }))
}

pub async fn book(
    State(db): State<Database>,
    Json(mut request): Json<NewTour>,
) -> ApiResult<(StatusCode, Json<GuestBooking>)> {
    request.guest_name = request.guest_name.trim().to_string();
    request.guest_phone = request.guest_phone.trim().to_string();
    let phone_digits = request
        .guest_phone
        .chars()
        .filter(|character| character.is_ascii_digit())
        .count();
    if request.guest_name.chars().count() < 2 || request.guest_name.chars().count() > 100 {
        return Err(bad_request("Enter your name."));
    }
    if request.guest_phone.len() > 30
        || phone_digits < 7
        || !request
            .guest_phone
            .chars()
            .all(|character| character.is_ascii_digit() || " +()-".contains(character))
    {
        return Err(bad_request("Enter a valid phone number."));
    }
    if !TOUR_SLOTS
        .iter()
        .any(|(slot, _)| *slot == request.slot_start)
    {
        return Err(bad_request("Choose a start time."));
    }
    let mut client = db.lock().await;
    let (today, last_date, now_time) = booking_window(&client).await?;
    if !valid_date(&request.tour_date, &today, &last_date)
        || (request.tour_date == today && request.slot_start.as_str() <= now_time.as_str())
    {
        return Err(bad_request("Choose a future tour date and time."));
    }
    let property = client
        .query_opt(
            "SELECT p.data->>'name' FROM properties p \
             LEFT JOIN tour_house_settings s ON s.property_id = p.id \
             WHERE p.id = $1 AND COALESCE(s.available, TRUE)",
            &[&(request.property_id as i64)],
        )
        .await
        .map_err(database_error)?
        .ok_or_else(|| bad_request("This house is not available for tours."))?;
    let property_name: String = property.get(0);
    let transaction = client.transaction().await.map_err(database_error)?;
    transaction
        .query_one(
            "SELECT pg_advisory_xact_lock(hashtext('tour_bookings'), hashtext($1))",
            &[&request.tour_date],
        )
        .await
        .map_err(database_error)?;
    let booked = transaction
        .query(
            "SELECT slot_start FROM tour_bookings WHERE tour_date = $1::text::date AND NOT is_demo AND cancelled_at IS NULL",
            &[&request.tour_date],
        )
        .await
        .map_err(database_error)?;
    if booked
        .iter()
        .any(|row| windows_overlap(&request.slot_start, &row.get::<_, String>(0)))
    {
        return Err((
            StatusCode::CONFLICT,
            "You already have a tour during that two-hour window. Choose another start time."
                .into(),
        ));
    }
    let insert = transaction
        .query_one(
            "INSERT INTO tour_bookings \
             (property_id, tour_date, slot_start, guest_name, guest_phone) \
             VALUES ($1, $2::text::date, $3, $4, $5) RETURNING id, guest_token",
            &[
                &(request.property_id as i64),
                &request.tour_date,
                &request.slot_start,
                &request.guest_name,
                &request.guest_phone,
            ],
        )
        .await;
    let row = match insert {
        Ok(row) => row,
        Err(error) if error.code() == Some(&SqlState::UNIQUE_VIOLATION) => {
            return Err((
                StatusCode::CONFLICT,
                "That time was just booked. Choose another time.".into(),
            ));
        }
        Err(error) => return Err(database_error(error)),
    };
    transaction.commit().await.map_err(database_error)?;
    Ok((
        StatusCode::CREATED,
        Json(GuestBooking {
            booking: TourBooking {
                id: row.get(0),
                property_id: request.property_id,
                property_name,
                room_name: None,
                rental_start: None,
                rental_year: None,
                rental_months: None,
                quoted_monthly_rent: None,
                quote_is_estimate: true,
                tour_date: request.tour_date,
                slot_start: request.slot_start,
                guest_name: request.guest_name,
                guest_phone: request.guest_phone,
                is_demo: false,
            },
            token: row.get(1),
        }),
    ))
}

pub async fn list(State(db): State<Database>) -> ApiResult<Json<Vec<TourBooking>>> {
    let client = db.lock().await;
    let rows = client
        .query(
            "SELECT b.id, b.property_id, COALESCE(p.data->>'name', 'Property'), \
                    b.tour_date::text, b.slot_start, b.guest_name, b.guest_phone, b.is_demo, \
                    b.room_name, b.rental_start, b.rental_months, b.quoted_monthly_rent, b.quote_is_estimate, b.rental_year \
             FROM tour_bookings b JOIN properties p ON p.id = b.property_id \
             WHERE b.cancelled_at IS NULL \
             ORDER BY b.tour_date DESC, b.slot_start DESC, b.id DESC",
            &[],
        )
        .await
        .map_err(database_error)?;
    Ok(Json(
        rows.into_iter()
            .map(|row| TourBooking {
                id: row.get(0),
                property_id: row.get::<_, i64>(1) as u32,
                property_name: row.get(2),
                tour_date: row.get(3),
                slot_start: row.get(4),
                guest_name: row.get(5),
                guest_phone: row.get(6),
                is_demo: row.get(7),
                room_name: row.get(8),
                rental_start: row.get(9),
                rental_months: row.get::<_, Option<i32>>(10).map(|value| value as u8),
                quoted_monthly_rent: row.get::<_, Option<i32>>(11).map(|value| value as u32),
                quote_is_estimate: row.get(12),
                rental_year: row.get(13),
            })
            .collect(),
    ))
}

pub async fn reschedule(
    Path(id): Path<i64>,
    State(db): State<Database>,
    Json(request): Json<RescheduleTour>,
) -> ApiResult<Json<TourBooking>> {
    if !TOUR_SLOTS
        .iter()
        .any(|(slot, _)| *slot == request.slot_start)
    {
        return Err(bad_request("Choose a start time."));
    }
    let mut client = db.lock().await;
    let (today, last_date, now_time) = booking_window(&client).await?;
    if !valid_date(&request.tour_date, &today, &last_date)
        || (request.tour_date == today && request.slot_start.as_str() <= now_time.as_str())
    {
        return Err(bad_request("Choose a future tour date and time."));
    }
    let transaction = client.transaction().await.map_err(database_error)?;
    transaction
        .query_one(
            "SELECT pg_advisory_xact_lock(hashtext('tour_bookings'), hashtext($1))",
            &[&request.tour_date],
        )
        .await
        .map_err(database_error)?;
    let original = transaction
        .query_opt(
            "SELECT b.property_id, COALESCE(p.data->>'name', 'Property'), b.guest_name, \
                    b.guest_phone, b.is_demo, b.room_name, b.rental_start, b.rental_months, b.quoted_monthly_rent, b.quote_is_estimate, b.rental_year \
             FROM tour_bookings b JOIN properties p ON p.id = b.property_id \
             WHERE b.id = $1 AND b.cancelled_at IS NULL FOR UPDATE OF b",
            &[&id],
        )
        .await
        .map_err(database_error)?
        .ok_or((
            StatusCode::NOT_FOUND,
            "Tour not found or already cancelled.".into(),
        ))?;
    let is_demo: bool = original.get(4);
    let property_id: i64 = original.get(0);
    let available: bool = transaction
        .query_one(
            "SELECT COALESCE(s.available, TRUE) FROM properties p \
         LEFT JOIN tour_house_settings s ON s.property_id = p.id WHERE p.id = $1",
            &[&property_id],
        )
        .await
        .map_err(database_error)?
        .get(0);
    if !available {
        return Err(bad_request("This house is not available for tours."));
    }
    let booked = transaction
        .query(
            "SELECT slot_start FROM tour_bookings WHERE tour_date = $1::text::date \
             AND cancelled_at IS NULL AND id <> $2 AND ($3::boolean OR NOT is_demo)",
            &[&request.tour_date, &id, &is_demo],
        )
        .await
        .map_err(database_error)?;
    if booked
        .iter()
        .any(|row| windows_overlap(&request.slot_start, &row.get::<_, String>(0)))
    {
        return Err((
            StatusCode::CONFLICT,
            "You already have a tour during that two-hour window. Choose another time.".into(),
        ));
    }
    let updated = transaction
        .execute(
            "UPDATE tour_bookings SET tour_date = $2::text::date, slot_start = $3 WHERE id = $1",
            &[&id, &request.tour_date, &request.slot_start],
        )
        .await;
    match updated {
        Ok(_) => (),
        Err(error) if error.code() == Some(&SqlState::UNIQUE_VIOLATION) => {
            return Err((
                StatusCode::CONFLICT,
                "That time was just booked. Choose another time.".into(),
            ));
        }
        Err(error) => return Err(database_error(error)),
    }
    transaction.commit().await.map_err(database_error)?;
    Ok(Json(TourBooking {
        id,
        property_id: original.get::<_, i64>(0) as u32,
        property_name: original.get(1),
        tour_date: request.tour_date,
        slot_start: request.slot_start,
        guest_name: original.get(2),
        guest_phone: original.get(3),
        is_demo,
        room_name: original.get(5),
        rental_start: original.get(6),
        rental_months: original.get::<_, Option<i32>>(7).map(|value| value as u8),
        quoted_monthly_rent: original.get::<_, Option<i32>>(8).map(|value| value as u32),
        quote_is_estimate: original.get(9),
        rental_year: original.get(10),
    }))
}

pub async fn cancel(Path(id): Path<i64>, State(db): State<Database>) -> ApiResult<StatusCode> {
    let client = db.lock().await;
    let changed = client
        .execute(
            "UPDATE tour_bookings SET cancelled_at = NOW() \
             WHERE id = $1 AND cancelled_at IS NULL",
            &[&id],
        )
        .await
        .map_err(database_error)?;
    if changed == 0 {
        return Err((
            StatusCode::NOT_FOUND,
            "Tour not found or already cancelled.".into(),
        ));
    }
    Ok(StatusCode::NO_CONTENT)
}

fn valid_guest_token(token: &str) -> bool {
    token.len() == 36
        && token.bytes().enumerate().all(|(index, byte)| {
            if [8, 13, 18, 23].contains(&index) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
}

async fn guest_booking_id(db: &Database, token: &str) -> ApiResult<i64> {
    if !valid_guest_token(token) {
        return Err((StatusCode::NOT_FOUND, "Booking link not found.".into()));
    }
    let client = db.lock().await;
    client
        .query_opt(
            "SELECT id FROM tour_bookings WHERE guest_token = $1",
            &[&token],
        )
        .await
        .map_err(database_error)?
        .map(|row| row.get(0))
        .ok_or((StatusCode::NOT_FOUND, "Booking link not found.".into()))
}

pub async fn guest_lookup(
    Path(token): Path<String>,
    State(db): State<Database>,
) -> ApiResult<Json<GuestBookingStatus>> {
    if !valid_guest_token(&token) {
        return Err((StatusCode::NOT_FOUND, "Booking link not found.".into()));
    }
    let client = db.lock().await;
    let row = client
        .query_opt(
            "SELECT b.id, b.property_id, COALESCE(p.data->>'name', 'Property'), \
                    b.tour_date::text, b.slot_start, b.guest_name, b.guest_phone, \
                    b.is_demo, b.cancelled_at IS NOT NULL, b.room_name, b.rental_start, b.rental_months, b.quoted_monthly_rent, b.quote_is_estimate, b.rental_year \
             FROM tour_bookings b JOIN properties p ON p.id = b.property_id \
             WHERE b.guest_token = $1",
            &[&token],
        )
        .await
        .map_err(database_error)?
        .ok_or((StatusCode::NOT_FOUND, "Booking link not found.".into()))?;
    Ok(Json(GuestBookingStatus {
        booking: TourBooking {
            id: row.get(0),
            property_id: row.get::<_, i64>(1) as u32,
            property_name: row.get(2),
            tour_date: row.get(3),
            slot_start: row.get(4),
            guest_name: row.get(5),
            guest_phone: row.get(6),
            is_demo: row.get(7),
            room_name: row.get(9),
            rental_start: row.get(10),
            rental_months: row.get::<_, Option<i32>>(11).map(|value| value as u8),
            quoted_monthly_rent: row.get::<_, Option<i32>>(12).map(|value| value as u32),
            quote_is_estimate: row.get(13),
            rental_year: row.get(14),
        },
        cancelled: row.get(8),
    }))
}

pub async fn guest_reschedule(
    Path(token): Path<String>,
    State(db): State<Database>,
    Json(request): Json<RescheduleTour>,
) -> ApiResult<Json<GuestBookingStatus>> {
    let id = guest_booking_id(&db, &token).await?;
    let Json(booking) = reschedule(Path(id), State(db), Json(request)).await?;
    Ok(Json(GuestBookingStatus {
        booking,
        cancelled: false,
    }))
}

pub async fn guest_cancel(
    Path(token): Path<String>,
    State(db): State<Database>,
) -> ApiResult<StatusCode> {
    let id = guest_booking_id(&db, &token).await?;
    cancel(Path(id), State(db)).await
}
