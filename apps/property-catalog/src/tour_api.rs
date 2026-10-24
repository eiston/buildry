use crate::tour_model::{
    Availability, GuestBooking, GuestBookingStatus, NewTour, RescheduleTour, TourBooking,
    TourHouse, TourOptions,
};
use gloo_net::http::Request;

async fn error_message(response: gloo_net::http::Response) -> String {
    response
        .text()
        .await
        .ok()
        .filter(|message| !message.trim().is_empty())
        .unwrap_or_else(|| "The booking service is unavailable. Please try again.".into())
}

pub async fn options() -> Result<TourOptions, String> {
    let response = Request::get("/api/tours/options")
        .send()
        .await
        .map_err(|_| "Cannot connect to the booking service.".to_string())?;
    if !response.ok() {
        return Err(error_message(response).await);
    }
    response.json().await.map_err(|error| error.to_string())
}

pub async fn houses() -> Result<Vec<TourHouse>, String> {
    let response = Request::get("/api/tours/houses")
        .send()
        .await
        .map_err(|_| "Cannot load tour houses.".to_string())?;
    if !response.ok() {
        return Err(error_message(response).await);
    }
    response.json().await.map_err(|error| error.to_string())
}

pub async fn set_house_availability(id: u32, available: bool) -> Result<TourHouse, String> {
    let response = Request::put(&format!("/api/tours/houses/{id}"))
        .json(&serde_json::json!({ "available": available }))
        .map_err(|error| error.to_string())?
        .send()
        .await
        .map_err(|_| "Cannot update tour availability.".to_string())?;
    if !response.ok() {
        return Err(error_message(response).await);
    }
    response.json().await.map_err(|error| error.to_string())
}

pub async fn availability(property_id: u32, date: &str) -> Result<Availability, String> {
    let url = format!(
        "/api/tours/availability?property_id={property_id}&date={}",
        urlencoding::encode(date)
    );
    let response = Request::get(&url)
        .send()
        .await
        .map_err(|_| "Cannot check available times.".to_string())?;
    if !response.ok() {
        return Err(error_message(response).await);
    }
    response.json().await.map_err(|error| error.to_string())
}

pub async fn availability_for_reschedule(
    property_id: u32,
    date: &str,
    booking_id: i64,
) -> Result<Availability, String> {
    let url = format!(
        "/api/tours/availability?property_id={property_id}&date={}&exclude_booking_id={booking_id}",
        urlencoding::encode(date)
    );
    let response = Request::get(&url)
        .send()
        .await
        .map_err(|_| "Cannot check available times.".to_string())?;
    if !response.ok() {
        return Err(error_message(response).await);
    }
    response.json().await.map_err(|error| error.to_string())
}

pub async fn book(request: &NewTour) -> Result<GuestBooking, String> {
    let response = Request::post("/api/tours")
        .json(request)
        .map_err(|error| error.to_string())?
        .send()
        .await
        .map_err(|_| "Cannot connect to the booking service.".to_string())?;
    if !response.ok() {
        return Err(error_message(response).await);
    }
    response.json().await.map_err(|error| error.to_string())
}

pub async fn guest_booking(token: &str) -> Result<GuestBookingStatus, String> {
    let response = Request::get(&format!("/api/guest-bookings/{token}"))
        .send()
        .await
        .map_err(|_| "Cannot load your booking.".to_string())?;
    if !response.ok() {
        return Err(error_message(response).await);
    }
    response.json().await.map_err(|error| error.to_string())
}

pub async fn guest_reschedule(
    token: &str,
    request: &RescheduleTour,
) -> Result<GuestBookingStatus, String> {
    let response = Request::put(&format!("/api/guest-bookings/{token}"))
        .json(request)
        .map_err(|error| error.to_string())?
        .send()
        .await
        .map_err(|_| "Cannot reschedule your booking.".to_string())?;
    if !response.ok() {
        return Err(error_message(response).await);
    }
    response.json().await.map_err(|error| error.to_string())
}

pub async fn guest_cancel(token: &str) -> Result<(), String> {
    let response = Request::delete(&format!("/api/guest-bookings/{token}"))
        .send()
        .await
        .map_err(|_| "Cannot cancel your booking.".to_string())?;
    if !response.ok() {
        return Err(error_message(response).await);
    }
    Ok(())
}

pub async fn list() -> Result<Vec<TourBooking>, String> {
    let response = Request::get("/api/tours")
        .send()
        .await
        .map_err(|_| "Cannot load tours.".to_string())?;
    if !response.ok() {
        return Err(error_message(response).await);
    }
    response.json().await.map_err(|error| error.to_string())
}

pub async fn reschedule(id: i64, request: &RescheduleTour) -> Result<TourBooking, String> {
    let response = Request::put(&format!("/api/tours/{id}"))
        .json(request)
        .map_err(|error| error.to_string())?
        .send()
        .await
        .map_err(|_| "Cannot reschedule the tour.".to_string())?;
    if !response.ok() {
        return Err(error_message(response).await);
    }
    response.json().await.map_err(|error| error.to_string())
}

pub async fn cancel(id: i64) -> Result<(), String> {
    let response = Request::delete(&format!("/api/tours/{id}"))
        .send()
        .await
        .map_err(|_| "Cannot cancel the tour.".to_string())?;
    if !response.ok() {
        return Err(error_message(response).await);
    }
    Ok(())
}
