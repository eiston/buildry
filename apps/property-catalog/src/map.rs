use gloo_net::http::Request;
use gloo_storage::{LocalStorage, Storage};
use serde::Deserialize;

const LAST_LOOKUP_KEY: &str = "buildry.nominatim.last_lookup.v1";

pub struct MapMatch {
    pub latitude: f64,
    pub longitude: f64,
    pub display_name: String,
}

#[derive(Deserialize)]
struct NominatimResult {
    lat: String,
    lon: String,
    display_name: String,
}

pub async fn locate(address: &str) -> Result<MapMatch, String> {
    let address = address.trim();
    if address.is_empty() {
        return Err("Add a street address before locating this property.".into());
    }

    // Public Nominatim permits at most one request per second. Keep the last
    // request time across reloads and only query after an explicit click.
    let now = js_sys::Date::now();
    let last: f64 = LocalStorage::get(LAST_LOOKUP_KEY).unwrap_or(0.0);
    if now - last < 1_100.0 {
        return Err("Please wait a moment before another map lookup.".into());
    }
    let _ = LocalStorage::set(LAST_LOOKUP_KEY, now);

    let query = if address.to_ascii_lowercase().contains("waterloo") {
        format!("{address}, Canada")
    } else {
        format!("{address}, Waterloo, Ontario, Canada")
    };
    let url = format!(
        "https://nominatim.openstreetmap.org/search?format=jsonv2&limit=1&countrycodes=ca&q={}",
        urlencoding::encode(&query)
    );
    let response = Request::get(&url)
        .send()
        .await
        .map_err(|_| "The map lookup could not connect. Try again later.".to_string())?;
    if response.status() != 200 {
        return Err(format!(
            "The map lookup returned HTTP {}.",
            response.status()
        ));
    }
    let mut results: Vec<NominatimResult> = response
        .json()
        .await
        .map_err(|_| "The map lookup returned an unreadable result.".to_string())?;
    let result = results
        .pop()
        .ok_or_else(|| "No location was found. Check the address and try again.".to_string())?;
    let latitude = result
        .lat
        .parse()
        .map_err(|_| "The map returned an invalid latitude.".to_string())?;
    let longitude = result
        .lon
        .parse()
        .map_err(|_| "The map returned an invalid longitude.".to_string())?;
    Ok(MapMatch {
        latitude,
        longitude,
        display_name: result.display_name,
    })
}
