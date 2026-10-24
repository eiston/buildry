use super::{auth::html_escape, database_error, leasing, model::Space, ApiResult, Database};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::{Datelike, NaiveDate};
use serde::Deserialize;
use tokio_postgres::error::SqlState;
#[path = "../../src/tour_model.rs"]
#[allow(dead_code)]
mod tour_model;
use tour_model::{
    windows_overlap, Availability, GuestBooking, GuestBookingStatus, NewTour, RescheduleTour,
    TourBooking, TourHouse, TourOptions, TourRoom, TOUR_SLOTS,
};

async fn send_manager_booking_notice(db: &Database, booking_id: i64) -> ApiResult<()> {
    let claimed = db.lock().await.query_opt(
        "WITH claimed AS (UPDATE tour_bookings SET manager_notice_claimed_at=NOW(), manager_notice_attempts=manager_notice_attempts+1 \
         WHERE id=$1 AND manager_notice_sent_at IS NULL AND cancelled_at IS NULL AND NOT is_demo \
         AND (manager_notice_claimed_at IS NULL OR manager_notice_claimed_at<NOW()-INTERVAL '2 minutes') RETURNING *) \
         SELECT CASE WHEN b.manager_kind='owner' THEN l.email WHEN b.manager_kind='staff' THEN u.email END, \
                CASE WHEN b.manager_kind='owner' THEN l.legal_name WHEN b.manager_kind='staff' THEN btrim(u.first_name || ' ' || u.last_name) END, \
                COALESCE(p.data->>'name','Property'), COALESCE(p.data->>'address',''), b.room_name, \
                b.tour_date::text, b.slot_start, b.guest_name, b.guest_email, b.guest_phone \
         FROM claimed b JOIN properties p ON p.id=b.property_id \
         LEFT JOIN landlords l ON b.manager_kind='owner' AND l.id=b.manager_id \
         LEFT JOIN users u ON b.manager_kind='staff' AND u.id=b.manager_id",
        &[&booking_id],
    ).await.map_err(database_error)?;
    let Some(row) = claimed else {
        return Ok(());
    };
    let to = row.get::<_, Option<String>>(0).unwrap_or_default();
    let manager = row
        .get::<_, Option<String>>(1)
        .unwrap_or_else(|| "manager".into());
    let property: String = row.get(2);
    let address: String = row.get(3);
    let room: Option<String> = row.get(4);
    let date: String = row.get(5);
    let slot: String = row.get(6);
    let guest: String = row.get(7);
    let guest_email: String = row.get(8);
    let guest_phone: String = row.get(9);
    let result = if !deliverable_manager_email(&to) {
        Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "The assigned manager needs a deliverable email address.".into(),
        ))
    } else {
        let date_label = NaiveDate::parse_from_str(&date, "%Y-%m-%d")
            .map(|date| date.format("%A, %B %-d, %Y").to_string())
            .unwrap_or(date.clone());
        let room_line = room
            .as_deref()
            .filter(|room| !room.trim().is_empty())
            .map(|room| {
                format!(
                    "<tr><th align=\"left\">Room</th><td>{}</td></tr>",
                    html_escape(room)
                )
            })
            .unwrap_or_default();
        let html = format!(
            "<p>Hello {},</p><p>A tour has been booked for you.</p><table cellpadding=\"6\" cellspacing=\"0\"><tr><th align=\"left\">Property</th><td>{}</td></tr><tr><th align=\"left\">Address</th><td>{}</td></tr>{}<tr><th align=\"left\">When</th><td>{} · {} (Toronto time)</td></tr><tr><th align=\"left\">Guest</th><td>{}</td></tr><tr><th align=\"left\">Email</th><td>{}</td></tr><tr><th align=\"left\">Phone</th><td>{}</td></tr></table><p>Booking reference: #{booking_id}</p>",
            html_escape(&manager), html_escape(&property), html_escape(&address), room_line,
            html_escape(&date_label), html_escape(tour_model::window_label(&slot)),
            html_escape(&guest), html_escape(&guest_email), html_escape(&guest_phone),
        );
        let subject_property = property.replace('\r', " ").replace('\n', " ");
        leasing::email(
            &to,
            &format!("New tour booking · {subject_property} · {date}"),
            html,
        )
        .await
    };
    let error = result.as_ref().err().map(|(_, message)| message.as_str());
    db.lock().await.execute(
        "UPDATE tour_bookings SET manager_notice_sent_at=CASE WHEN $2::text IS NULL THEN NOW() ELSE manager_notice_sent_at END, manager_notice_claimed_at=NULL, manager_notice_error=$2 WHERE id=$1",
        &[&booking_id, &error],
    ).await.map_err(database_error)?;
    result
}

pub fn start_manager_notice_worker(db: Database) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
        loop {
            interval.tick().await;
            let pending = db.lock().await.query(
                "SELECT id FROM tour_bookings WHERE manager_notice_sent_at IS NULL AND cancelled_at IS NULL AND NOT is_demo AND manager_kind IS NOT NULL AND (manager_notice_claimed_at IS NULL OR manager_notice_claimed_at<NOW()-INTERVAL '2 minutes') ORDER BY id LIMIT 30",
                &[],
            ).await;
            match pending {
                Ok(rows) => {
                    for row in rows {
                        let id: i64 = row.get(0);
                        if let Err((_, message)) = send_manager_booking_notice(&db, id).await {
                            eprintln!("Manager email for tour {id}: {message}");
                        }
                    }
                }
                Err(error) => eprintln!("Could not find pending manager emails: {error}"),
            }
        }
    });
}

fn bad_request(message: &str) -> (StatusCode, String) {
    (StatusCode::BAD_REQUEST, message.into())
}

fn quoted_monthly_rent(room: &Space, start: NaiveDate, months: i32) -> Option<i32> {
    let mut total = 0_u64;
    for offset in 0..months {
        let absolute = start.year() * 12 + start.month0() as i32 + offset;
        let year = absolute.div_euclid(12);
        let month = absolute.rem_euclid(12) as u8 + 1;
        total += room.price_for_calendar_month(year, month)? as u64;
    }
    Some(((total + months as u64 / 2) / months as u64) as i32)
}

#[cfg(test)]
mod pricing_tests {
    use super::*;
    use crate::model::MonthlyPriceOverride;

    #[test]
    fn tour_quote_includes_month_overrides() {
        let mut room = Space::new(1);
        room.current_monthly_rent = Some(1000);
        room.monthly_price_overrides.push(MonthlyPriceOverride {
            year: 2027,
            month: 1,
            price: 1400,
        });
        let start = NaiveDate::from_ymd_opt(2026, 9, 1).unwrap();
        assert_eq!(quoted_monthly_rent(&room, start, 8), Some(1050));
    }
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

fn deliverable_manager_email(email: &str) -> bool {
    let email = email.trim();
    email.contains('@')
        && !email.chars().any(char::is_whitespace)
        && !email.to_ascii_lowercase().ends_with("@example.invalid")
}

async fn manager_candidates<C: tokio_postgres::GenericClient + Sync>(
    client: &C,
    property_id: i64,
) -> ApiResult<Vec<(String, i64)>> {
    let row = client.query_opt(
        "SELECT p.landlord_id, COALESCE((p.data->>'owner_manages_tours')::boolean, TRUE), COALESCE(p.data->'manager_staff_ids', '[]'::jsonb)::text, COALESCE(l.email, '') FROM properties p LEFT JOIN landlords l ON l.id=p.landlord_id WHERE p.id=$1",
        &[&property_id],
    ).await.map_err(database_error)?.ok_or_else(|| bad_request("This house is not available for tours."))?;
    let mut managers = Vec::new();
    if row.get::<_, bool>(1) && deliverable_manager_email(&row.get::<_, String>(3)) {
        if let Some(id) = row.get::<_, Option<i64>>(0) {
            managers.push(("owner".to_string(), id));
        }
    }
    let staff_ids: Vec<i64> =
        serde_json::from_str(&row.get::<_, String>(2)).map_err(database_error)?;
    let rows = client.query(
        "SELECT id,email FROM users WHERE id=ANY($1::bigint[]) AND role='staff' AND active ORDER BY id",
        &[&staff_ids],
    ).await.map_err(database_error)?;
    managers.extend(rows.into_iter().filter_map(|row| {
        let email: String = row.get(1);
        deliverable_manager_email(&email).then(|| ("staff".to_string(), row.get(0)))
    }));
    Ok(managers)
}

fn free_manager(
    candidates: &[(String, i64)],
    bookings: &[(i64, Option<String>, Option<i64>, String)],
    property_id: i64,
    slot: &str,
) -> Option<(String, i64)> {
    if bookings.iter().any(|(booked_property, _, _, booked_slot)| {
        *booked_property == property_id && windows_overlap(slot, booked_slot)
    }) {
        return None;
    }
    candidates
        .iter()
        .find(|(kind, id)| {
            !bookings
                .iter()
                .any(|(_, booked_kind, booked_id, booked_slot)| {
                    windows_overlap(slot, booked_slot)
                        && (booked_kind.is_none()
                            || (booked_kind.as_deref() == Some(kind.as_str())
                                && *booked_id == Some(*id)))
                })
        })
        .cloned()
}

fn booking_rows(rows: Vec<tokio_postgres::Row>) -> Vec<(i64, Option<String>, Option<i64>, String)> {
    rows.into_iter()
        .map(|row| (row.get(0), row.get(1), row.get(2), row.get(3)))
        .collect()
}

#[cfg(test)]
mod manager_schedule_tests {
    use super::{deliverable_manager_email, free_manager};

    #[test]
    fn only_real_manager_addresses_can_receive_bookings() {
        assert!(deliverable_manager_email("host@buildry.ca"));
        assert!(!deliverable_manager_email(""));
        assert!(!deliverable_manager_email("helen@example.invalid"));
        assert!(!deliverable_manager_email("bad address@buildry.ca"));
    }

    #[test]
    fn another_manager_can_host_when_the_first_is_busy_elsewhere() {
        let managers = vec![("owner".into(), 5), ("staff".into(), 12)];
        let bookings = vec![(9, Some("owner".into()), Some(5), "10:00".into())];
        assert_eq!(
            free_manager(&managers, &bookings, 2, "10:00"),
            Some(("staff".into(), 12))
        );
    }

    #[test]
    fn same_house_and_legacy_unassigned_bookings_still_block() {
        let managers = vec![("staff".into(), 12)];
        let same_house = vec![(2, Some("owner".into()), Some(5), "10:00".into())];
        assert_eq!(free_manager(&managers, &same_house, 2, "11:00"), None);
        let legacy = vec![(9, None, None, "10:00".into())];
        assert_eq!(free_manager(&managers, &legacy, 2, "11:00"), None);
        assert_eq!(
            free_manager(&managers, &legacy, 2, "12:00"),
            Some(("staff".into(), 12))
        );
    }
}

pub async fn options(State(db): State<Database>) -> ApiResult<Json<TourOptions>> {
    let client = db.lock().await;
    let (today, last_date, now_time) = booking_window(&client).await?;
    let rows = client
        .query(
            "SELECT p.id, COALESCE(NULLIF(p.data->>'external_name',''), p.data->>'name'), p.data->>'address' FROM properties p \
                LEFT JOIN tour_house_settings s ON s.property_id = p.id \
                WHERE COALESCE(s.available, TRUE) ORDER BY p.id",
            &[],
        )
        .await
        .map_err(database_error)?;
    let mut houses: Vec<TourHouse> = rows
        .into_iter()
        .map(|row| TourHouse {
            id: row.get::<_, i64>(0) as u32,
            name: row.get(1),
            address: row.get(2),
            available: true,
            rooms: Vec::new(),
        })
        .collect();
    let rooms=client.query("SELECT property_id,id,COALESCE(NULLIF(data->>'external_name',''),data->>'name'),COALESCE((data->>'current_monthly_rent')::integer,(data->>'rent_jan_apr')::integer,(data->>'rent_may_aug')::integer,(data->>'rent_sep_dec')::integer) FROM property_spaces WHERE data->>'kind' IN ('Bedroom','Suite') ORDER BY id",&[]).await.map_err(database_error)?;
    for row in rooms {
        if let Some(house) = houses
            .iter_mut()
            .find(|h| h.id as i64 == row.get::<_, i64>(0))
        {
            house.rooms.push(TourRoom {
                id: row.get::<_, i64>(1) as u32,
                name: row.get(2),
                monthly_rent: row.get::<_, Option<i32>>(3).map(|v| v as u32),
            })
        }
    }
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
                rooms: Vec::new(),
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
        rooms: Vec::new(),
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
    let managers = manager_candidates(&*client, query.property_id as i64).await?;
    let booked = booking_rows(client
        .query(
            "SELECT property_id, manager_kind, manager_id, slot_start FROM tour_bookings WHERE tour_date = $1::text::date \
             AND cancelled_at IS NULL AND id IS DISTINCT FROM $2 \
             AND (NOT is_demo OR EXISTS \
               (SELECT 1 FROM tour_bookings original WHERE original.id = $2 AND original.is_demo))",
            &[&query.date, &query.exclude_booking_id],
        )
        .await
        .map_err(database_error)?
        );
    let available_slots = TOUR_SLOTS
        .iter()
        .filter(|(slot, _)| {
            free_manager(&managers, &booked, query.property_id as i64, slot).is_some()
        })
        .filter(|(slot, _)| query.date != today || *slot > now_time.as_str())
        .map(|(slot, _)| (*slot).to_string())
        .collect();
    Ok(Json(Availability { available_slots }))
}

pub async fn book(
    State(db): State<Database>,
    Json(mut request): Json<NewTour>,
) -> ApiResult<(StatusCode, Json<GuestBooking>)> {
    request.legal_first_name = request.legal_first_name.trim().to_string();
    request.legal_last_name = request.legal_last_name.trim().to_string();
    request.guest_phone = request.guest_phone.trim().to_string();
    request.guest_email = request.guest_email.trim().to_lowercase();
    let phone_digits = request
        .guest_phone
        .chars()
        .filter(|character| character.is_ascii_digit())
        .count();
    let guest_name = format!("{} {}", request.legal_first_name, request.legal_last_name);
    if request.legal_first_name.is_empty()
        || request.legal_last_name.is_empty()
        || request.legal_first_name.chars().count() > 100
        || request.legal_last_name.chars().count() > 100
        || guest_name.len() > 120
    {
        return Err(bad_request("Enter your legal first and last name."));
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
    if request.guest_email.len() > 254 || !request.guest_email.contains('@') {
        return Err(bad_request("Enter a valid email address."));
    }
    if ![4, 8, 12, 24, 36].contains(&request.rental_months) {
        return Err(bad_request("Choose a valid rental period."));
    }
    let rental_start = NaiveDate::parse_from_str(&request.rental_start, "%Y-%m-%d")
        .map_err(|_| bad_request("Choose a valid rental period."))?;
    if rental_start.day() != 1 || ![1, 5, 9].contains(&rental_start.month()) {
        return Err(bad_request("Choose a valid rental period."));
    }
    let rental_end = rental_start
        .checked_add_months(chrono::Months::new(request.rental_months as u32))
        .and_then(|date| date.pred_opt())
        .ok_or_else(|| bad_request("Choose a valid rental period."))?;
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
            "SELECT COALESCE(NULLIF(p.data->>'external_name',''),p.data->>'name') FROM properties p \
             LEFT JOIN tour_house_settings s ON s.property_id = p.id \
             WHERE p.id = $1 AND COALESCE(s.available, TRUE)",
            &[&(request.property_id as i64)],
        )
        .await
        .map_err(database_error)?
        .ok_or_else(|| bad_request("This house is not available for tours."))?;
    let property_name: String = property.get(0);
    let room=client.query_opt("SELECT data::text FROM property_spaces WHERE property_id=$1 AND id=$2 AND data->>'kind' IN ('Bedroom','Suite')",&[&(request.property_id as i64),&(request.room_id as i64)]).await.map_err(database_error)?.ok_or_else(||bad_request("Choose a room or suite."))?;
    let room: Space = serde_json::from_str(&room.get::<_, String>(0))
        .map_err(|_| bad_request("Could not read the room price."))?;
    let room_name = if room.external_name.trim().is_empty() {
        room.name.clone()
    } else {
        room.external_name.clone()
    };
    let quoted_rent = quoted_monthly_rent(&room, rental_start, request.rental_months as i32);
    let conflict = client.query_opt(
        "SELECT id FROM leases WHERE property_id=$1 AND (room_id=$2 OR room_id=0) AND status IN ('signed','confirmed') AND details->>'start_date'<=$3 AND COALESCE(NULLIF(details->>'end_date',''),'9999-12-31')>=$4 LIMIT 1",
        &[&(request.property_id as i64), &(request.room_id as i64), &rental_end.to_string(), &request.rental_start],
    ).await.map_err(database_error)?;
    if conflict.is_some() {
        return Err((
            StatusCode::CONFLICT,
            "This room is occupied during the selected rental period. Choose different dates."
                .into(),
        ));
    }
    let transaction = client.transaction().await.map_err(database_error)?;
    transaction
        .query_one(
            "SELECT pg_advisory_xact_lock(hashtext('tour_bookings'), hashtext($1))",
            &[&request.tour_date],
        )
        .await
        .map_err(database_error)?;
    let managers = manager_candidates(&transaction, request.property_id as i64).await?;
    let booked = booking_rows(transaction
        .query(
            "SELECT property_id, manager_kind, manager_id, slot_start FROM tour_bookings WHERE tour_date = $1::text::date AND NOT is_demo AND cancelled_at IS NULL",
            &[&request.tour_date],
        )
        .await
        .map_err(database_error)?);
    let (manager_kind, manager_id) = free_manager(
        &managers,
        &booked,
        request.property_id as i64,
        &request.slot_start,
    )
    .ok_or((
        StatusCode::CONFLICT,
        "No property manager is free at that time. Choose another start time.".into(),
    ))?;
    let insert = transaction
        .query_one(
            "INSERT INTO tour_bookings \
             (property_id, tour_date, slot_start, guest_name, guest_phone, guest_email, room_id, room_name, rental_start, rental_months, quoted_monthly_rent, quote_is_estimate, legal_first_name, legal_last_name, manager_kind, manager_id) \
             VALUES ($1, $2::text::date, $3, $4, $5, $6, $7, $8, $9, $10, $11, TRUE, $12, $13, $14, $15) RETURNING id, guest_token",
            &[
                &(request.property_id as i64),
                &request.tour_date,
                &request.slot_start,
                &guest_name,
                &request.guest_phone,
                &request.guest_email,&(request.room_id as i64),&room_name,&request.rental_start,&(request.rental_months as i32),&quoted_rent,&request.legal_first_name,&request.legal_last_name,&manager_kind,&manager_id,
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
    let booking_id: i64 = row.get(0);
    drop(client);
    if let Err((_, error)) = send_manager_booking_notice(&db, booking_id).await {
        eprintln!("Manager email for tour {booking_id}: {error}");
    }
    Ok((
        StatusCode::CREATED,
        Json(GuestBooking {
            booking: TourBooking {
                id: row.get(0),
                property_id: request.property_id,
                property_name,
                manager_name: None,
                manager_notice_sent: false,
                manager_notice_error: None,
                room_id: Some(request.room_id),
                room_name: Some(room_name),
                rental_start: Some(request.rental_start),
                rental_year: None,
                rental_months: Some(request.rental_months),
                quoted_monthly_rent: quoted_rent.map(|v| v as u32),
                quote_is_estimate: true,
                tour_date: request.tour_date,
                slot_start: request.slot_start,
                guest_name,
                legal_first_name: Some(request.legal_first_name),
                legal_last_name: Some(request.legal_last_name),
                guest_phone: request.guest_phone,
                guest_email: request.guest_email,
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
                    b.room_name, b.rental_start, b.rental_months, b.quoted_monthly_rent, b.quote_is_estimate, b.rental_year, b.guest_email, b.room_id, b.legal_first_name, b.legal_last_name, \
                    CASE WHEN b.manager_kind='owner' THEN l.legal_name WHEN b.manager_kind='staff' THEN btrim(u.first_name || ' ' || u.last_name) END, \
                    b.manager_notice_sent_at IS NOT NULL, b.manager_notice_error \
             FROM tour_bookings b JOIN properties p ON p.id = b.property_id \
             LEFT JOIN landlords l ON b.manager_kind='owner' AND l.id=b.manager_id \
             LEFT JOIN users u ON b.manager_kind='staff' AND u.id=b.manager_id \
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
                manager_name: row.get(18),
                manager_notice_sent: row.get(19),
                manager_notice_error: row.get(20),
                room_id: row.get::<_, Option<i64>>(15).map(|id| id as u32),
                tour_date: row.get(3),
                slot_start: row.get(4),
                guest_name: row.get(5),
                legal_first_name: row.get(16),
                legal_last_name: row.get(17),
                guest_phone: row.get(6),
                guest_email: row.get(14),
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

#[derive(Deserialize)]
pub struct LeasingInfoUpdate {
    guest_email: String,
    room_id: i64,
    rental_start: String,
    rental_months: i32,
}
pub async fn update_leasing_info(
    Path(id): Path<i64>,
    State(db): State<Database>,
    Json(input): Json<LeasingInfoUpdate>,
) -> ApiResult<StatusCode> {
    let email = input.guest_email.trim().to_lowercase();
    if email.len() > 254
        || !email.contains('@')
        || !(1..=36).contains(&input.rental_months)
        || NaiveDate::parse_from_str(&input.rental_start, "%Y-%m-%d").is_err()
    {
        return Err(bad_request(
            "Complete the guest email, move-in date, and rental duration.",
        ));
    }
    let client = db.lock().await;
    let row = client
        .query_opt(
            "SELECT property_id FROM tour_bookings WHERE id=$1 AND cancelled_at IS NULL",
            &[&id],
        )
        .await
        .map_err(database_error)?
        .ok_or((StatusCode::NOT_FOUND, "Tour not found.".into()))?;
    let property_id: i64 = row.get(0);
    let room=client.query_opt("SELECT data::text FROM property_spaces WHERE property_id=$1 AND id=$2 AND data->>'kind' IN ('Bedroom','Suite')",&[&property_id,&input.room_id]).await.map_err(database_error)?.ok_or_else(||bad_request("Choose a room in this property."))?;
    let room: Space = serde_json::from_str(&room.get::<_, String>(0))
        .map_err(|_| bad_request("Could not read the room price."))?;
    let start = NaiveDate::parse_from_str(&input.rental_start, "%Y-%m-%d")
        .map_err(|_| bad_request("Choose a valid rental period."))?;
    let quoted_rent = quoted_monthly_rent(&room, start, input.rental_months);
    let room_name = if room.external_name.trim().is_empty() {
        room.name.clone()
    } else {
        room.external_name.clone()
    };
    let changed=client.execute("UPDATE tour_bookings SET guest_email=$2,room_id=$3,room_name=$4,rental_start=$5,rental_months=$6,quoted_monthly_rent=$7 WHERE id=$1 AND NOT EXISTS (SELECT 1 FROM leases WHERE tour_id=$1)",&[&id,&email,&input.room_id,&Some(room_name),&input.rental_start,&input.rental_months,&quoted_rent]).await.map_err(database_error)?;
    if changed == 0 {
        return Err((
            StatusCode::CONFLICT,
            "A lease already exists for this tour.".into(),
        ));
    }
    Ok(StatusCode::NO_CONTENT)
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
            "SELECT b.property_id, COALESCE(NULLIF(p.data->>'external_name',''), p.data->>'name', 'Property'), b.guest_name, \
                    b.guest_phone, b.is_demo, COALESCE(NULLIF(s.data->>'external_name',''),b.room_name), b.rental_start, b.rental_months, b.quoted_monthly_rent, b.quote_is_estimate, b.rental_year, b.guest_email, b.room_id, b.legal_first_name, b.legal_last_name \
             FROM tour_bookings b JOIN properties p ON p.id = b.property_id \
             LEFT JOIN property_spaces s ON s.property_id=b.property_id AND s.id=b.room_id \
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
    let managers = manager_candidates(&transaction, property_id).await?;
    let booked = booking_rows(transaction
        .query(
            "SELECT property_id, manager_kind, manager_id, slot_start FROM tour_bookings WHERE tour_date = $1::text::date \
             AND cancelled_at IS NULL AND id <> $2 AND ($3::boolean OR NOT is_demo)",
            &[&request.tour_date, &id, &is_demo],
        )
        .await
        .map_err(database_error)?);
    let (manager_kind, manager_id) =
        free_manager(&managers, &booked, property_id, &request.slot_start).ok_or((
            StatusCode::CONFLICT,
            "No property manager is free at that time. Choose another time.".into(),
        ))?;
    let updated = transaction
        .execute(
            "UPDATE tour_bookings SET tour_date = $2::text::date, slot_start = $3, manager_kind=$4, manager_id=$5 WHERE id = $1",
            &[&id, &request.tour_date, &request.slot_start, &manager_kind, &manager_id],
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
        manager_name: None,
        manager_notice_sent: false,
        manager_notice_error: None,
        room_id: original.get::<_, Option<i64>>(12).map(|id| id as u32),
        tour_date: request.tour_date,
        slot_start: request.slot_start,
        guest_name: original.get(2),
        legal_first_name: original.get(13),
        legal_last_name: original.get(14),
        guest_phone: original.get(3),
        guest_email: original.get(11),
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
            "SELECT b.id, b.property_id, COALESCE(NULLIF(p.data->>'external_name',''), p.data->>'name', 'Property'), \
                    b.tour_date::text, b.slot_start, b.guest_name, b.guest_phone, \
                    b.is_demo, b.cancelled_at IS NOT NULL, COALESCE(NULLIF(s.data->>'external_name',''),b.room_name), b.rental_start, b.rental_months, b.quoted_monthly_rent, b.quote_is_estimate, b.rental_year, b.guest_email, b.room_id, b.legal_first_name, b.legal_last_name \
             FROM tour_bookings b JOIN properties p ON p.id = b.property_id \
             LEFT JOIN property_spaces s ON s.property_id=b.property_id AND s.id=b.room_id \
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
            manager_name: None,
            manager_notice_sent: false,
            manager_notice_error: None,
            room_id: row.get::<_, Option<i64>>(16).map(|id| id as u32),
            tour_date: row.get(3),
            slot_start: row.get(4),
            guest_name: row.get(5),
            legal_first_name: row.get(17),
            legal_last_name: row.get(18),
            guest_phone: row.get(6),
            guest_email: row.get(15),
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
