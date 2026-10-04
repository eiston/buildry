use crate::Property;
use gloo_net::http::Request;

const API: &str = "/api/properties";

pub async fn list() -> Result<Vec<Property>, String> {
    let response = Request::get(API)
        .send()
        .await
        .map_err(|_| "Cannot connect to the property API.".to_string())?;
    if !response.ok() {
        return Err(format!(
            "Could not load properties (HTTP {}).",
            response.status()
        ));
    }
    response
        .json()
        .await
        .map_err(|_| "The property API returned unreadable data.".to_string())
}

pub async fn bootstrap(initial: &[Property]) -> Result<Vec<Property>, String> {
    let response = Request::post(&format!("{API}/bootstrap"))
        .json(initial)
        .map_err(|error| error.to_string())?
        .send()
        .await
        .map_err(|_| "Cannot connect to the property API.".to_string())?;
    if !response.ok() {
        return Err(format!("Property API returned HTTP {}.", response.status()));
    }
    response
        .json()
        .await
        .map_err(|_| "The property API returned unreadable data.".to_string())
}

pub async fn save(property: &Property) -> Result<Property, String> {
    let response = Request::put(&format!("{API}/{}", property.id))
        .json(property)
        .map_err(|error| error.to_string())?
        .send()
        .await
        .map_err(|_| "Cannot connect to the property API.".to_string())?;
    if !response.ok() {
        let status = response.status();
        let detail = response.text().await.unwrap_or_default();
        return Err(if detail.is_empty() {
            format!("Could not save property (HTTP {status}).")
        } else {
            detail
        });
    }
    response
        .json()
        .await
        .map_err(|_| "The property API returned unreadable data.".to_string())
}

pub async fn upload_image(file: dioxus::html::FileData) -> Result<String, String> {
    let content_type = file.content_type().unwrap_or_default();
    if !["image/jpeg", "image/png", "image/webp"].contains(&content_type.as_str()) {
        return Err("Choose a JPEG, PNG, or WebP image.".into());
    }
    if file.size() > 5_000_000 {
        return Err("Image must be smaller than 5 MB.".into());
    }
    let bytes = file
        .read_bytes()
        .await
        .map_err(|_| "Could not read the image.".to_string())?;
    let response = Request::post("/api/images")
        .header("Content-Type", &content_type)
        .body(bytes.to_vec())
        .map_err(|_| "Could not prepare the image upload.".to_string())?
        .send()
        .await
        .map_err(|_| "Could not upload the image.".to_string())?;
    if !response.ok() {
        let status = response.status();
        let detail = response.text().await.unwrap_or_default();
        return Err(if detail.is_empty() {
            format!("Image upload failed (HTTP {status}).")
        } else {
            detail
        });
    }
    response
        .json()
        .await
        .map_err(|_| "Image upload returned unreadable data.".to_string())
}
