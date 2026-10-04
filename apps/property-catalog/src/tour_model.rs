use serde::{Deserialize, Serialize};

pub const TOUR_SLOTS: [(&str, &str); 7] = [
    ("10:00", "10:00 AM"),
    ("11:00", "11:00 AM"),
    ("12:00", "12:00 PM"),
    ("13:00", "1:00 PM"),
    ("14:00", "2:00 PM"),
    ("15:00", "3:00 PM"),
    ("16:00", "4:00 PM"),
];

pub fn slot_label(key: &str) -> &'static str {
    TOUR_SLOTS
        .iter()
        .find(|(slot, _)| *slot == key)
        .map(|(_, label)| *label)
        .unwrap_or("Unknown time")
}

pub fn window_label(key: &str) -> &'static str {
    match key {
        "10:00" => "10:00 AM–12:00 PM",
        "11:00" => "11:00 AM–1:00 PM",
        "12:00" => "12:00–2:00 PM",
        "13:00" => "1:00–3:00 PM",
        "14:00" => "2:00–4:00 PM",
        "15:00" => "3:00–5:00 PM",
        "16:00" => "4:00–6:00 PM",
        _ => "Unknown time",
    }
}

#[allow(dead_code)]
pub fn windows_overlap(first: &str, second: &str) -> bool {
    fn minutes(value: &str) -> Option<i32> {
        let (hour, minute) = value.split_once(':')?;
        Some(hour.parse::<i32>().ok()? * 60 + minute.parse::<i32>().ok()?)
    }
    match (minutes(first), minutes(second)) {
        (Some(first), Some(second)) => (first - second).abs() < 120,
        _ => false,
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TourHouse {
    pub id: u32,
    pub name: String,
    #[serde(default)]
    pub address: String,
    #[serde(default = "default_tour_available")]
    pub available: bool,
}

fn default_tour_available() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TourOptions {
    pub houses: Vec<TourHouse>,
    pub today: String,
    pub last_date: String,
    pub now_time: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Availability {
    pub available_slots: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NewTour {
    pub property_id: u32,
    pub tour_date: String,
    pub slot_start: String,
    pub guest_name: String,
    pub guest_phone: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RescheduleTour {
    pub tour_date: String,
    pub slot_start: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TourBooking {
    pub id: i64,
    pub property_id: u32,
    pub property_name: String,
    #[serde(default)]
    pub room_name: Option<String>,
    #[serde(default)]
    pub rental_start: Option<String>,
    #[serde(default)]
    pub rental_year: Option<i32>,
    #[serde(default)]
    pub rental_months: Option<u8>,
    #[serde(default)]
    pub quoted_monthly_rent: Option<u32>,
    #[serde(default)]
    pub quote_is_estimate: bool,
    pub tour_date: String,
    pub slot_start: String,
    pub guest_name: String,
    pub guest_phone: String,
    #[serde(default)]
    pub is_demo: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GuestBooking {
    pub booking: TourBooking,
    pub token: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GuestBookingStatus {
    pub booking: TourBooking,
    pub cancelled: bool,
}
