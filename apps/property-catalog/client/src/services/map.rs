use gloo_net::http::Request;
use serde::Deserialize;

#[derive(Clone, PartialEq, Deserialize)]
pub struct AddressSuggestion {
    pub label: String,
    pub latitude: f64,
    pub longitude: f64,
}

pub async fn suggest_addresses(query: &str) -> Result<Vec<AddressSuggestion>, String> {
    let url = format!(
        "/api/address-suggestions?q={}",
        urlencoding::encode(query.trim())
    );
    let response = Request::get(&url).send().await.map_err(|_| {
        "Address suggestions are unavailable. You can still enter an address manually.".to_string()
    })?;
    if response.status() != 200 {
        return Err(
            "Address suggestions are unavailable. You can still enter an address manually.".into(),
        );
    }
    response.json().await.map_err(|_| {
        "Address suggestions are unavailable. You can still enter an address manually.".to_string()
    })
}
