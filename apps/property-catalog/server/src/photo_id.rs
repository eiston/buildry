use crate::{
    auth::{html_escape, Database},
    database_error, ApiResult,
};
use aes_gcm::{
    aead::{Aead, OsRng},
    Aes256Gcm, KeyInit, Nonce,
};
use axum::{
    extract::{Multipart, Path, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
};
use base64::{engine::general_purpose::STANDARD, Engine};
use image::{GenericImageView, ImageFormat, ImageReader, Limits};
use sha2::{Digest, Sha256};
use std::{env, io::Cursor};

const TERMS_VERSION: &str = "2026-10-12";
const MAX_IMAGE: usize = 5_000_000;

pub async fn migrate(db: &tokio_postgres::Client) -> Result<(), tokio_postgres::Error> {
    db.batch_execute("CREATE TABLE IF NOT EXISTS lease_photo_ids (
        lease_id BIGINT PRIMARY KEY REFERENCES leases(id) ON DELETE CASCADE,
        document_type TEXT NOT NULL CHECK (document_type IN ('drivers_licence','photo_card','passport')),
        front_type TEXT NOT NULL, front_nonce BYTEA NOT NULL, front_ciphertext BYTEA NOT NULL,
        back_type TEXT, back_nonce BYTEA, back_ciphertext BYTEA,
        terms_version TEXT NOT NULL, accepted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
        uploaded_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
        verified_at TIMESTAMPTZ, verified_by BIGINT REFERENCES users(id),
        legal_hold BOOLEAN NOT NULL DEFAULT FALSE
    );
    ALTER TABLE lease_photo_ids ADD COLUMN IF NOT EXISTS legal_hold BOOLEAN NOT NULL DEFAULT FALSE;
    ALTER TABLE lease_photo_ids ADD COLUMN IF NOT EXISTS verified_at TIMESTAMPTZ;
    ALTER TABLE lease_photo_ids ADD COLUMN IF NOT EXISTS verified_by BIGINT REFERENCES users(id);
    CREATE TABLE IF NOT EXISTS lease_photo_id_access (
        id BIGSERIAL PRIMARY KEY, lease_id BIGINT NOT NULL REFERENCES leases(id) ON DELETE CASCADE,
        user_id BIGINT NOT NULL REFERENCES users(id), side TEXT NOT NULL,
        viewed_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
    )").await
}

pub async fn delete_expired(db: &Database) -> Result<u64, tokio_postgres::Error> {
    db.lock().await.execute("DELETE FROM lease_photo_ids p USING leases l WHERE p.lease_id=l.id AND NOT p.legal_hold AND l.tenancy_ended_at + INTERVAL '1 year' <= (NOW() AT TIME ZONE 'America/Toronto')::date", &[]).await
}

fn secret_hash(secret: &str) -> String {
    format!("{:x}", Sha256::digest(secret.as_bytes()))
}

pub(crate) fn encryption_key() -> Result<[u8; 32], (StatusCode, String)> {
    let encoded = env::var("PHOTO_ID_ENCRYPTION_KEY").map_err(|_| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Secure ID storage is not configured. Please contact Buildry.".into(),
        )
    })?;
    let bytes = STANDARD.decode(encoded.trim()).map_err(|_| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Secure ID storage is not configured. Please contact Buildry.".into(),
        )
    })?;
    bytes.try_into().map_err(|_| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Secure ID storage is not configured. Please contact Buildry.".into(),
        )
    })
}

fn image_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

fn validate_image(bytes: &[u8]) -> Result<&'static str, &'static str> {
    let mime = image_type(bytes).ok_or("Upload a JPEG, PNG, or WebP image.")?;
    let format = match mime {
        "image/jpeg" => ImageFormat::Jpeg,
        "image/png" => ImageFormat::Png,
        _ => ImageFormat::WebP,
    };
    if image::guess_format(bytes).ok() != Some(format) {
        return Err("The image format is invalid.");
    }
    let (width, height) = ImageReader::with_format(Cursor::new(bytes), format)
        .into_dimensions()
        .map_err(|_| "This image is damaged or unreadable. Take a new photo.")?;
    if width.min(height) < 600
        || width.max(height) > 6_000
        || u64::from(width) * u64::from(height) > 20_000_000
    {
        return Err("Use a clear photo between 600 and 6,000 pixels per side.");
    }
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = Limits::default();
    limits.max_image_width = Some(6_000);
    limits.max_image_height = Some(6_000);
    limits.max_alloc = Some(100_000_000);
    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|_| "This image is damaged or unreadable. Take a new photo.")?;
    if image.dimensions() != (width, height) {
        return Err("The image dimensions are invalid.");
    }
    let mut sum = 0_f64;
    let mut squared = 0_f64;
    for y in 0..32 {
        for x in 0..32 {
            let pixel = image
                .get_pixel(
                    (x * width / 32).min(width - 1),
                    (y * height / 32).min(height - 1),
                )
                .0;
            let luminance = 0.2126 * f64::from(pixel[0])
                + 0.7152 * f64::from(pixel[1])
                + 0.0722 * f64::from(pixel[2]);
            sum += luminance;
            squared += luminance * luminance;
        }
    }
    let mean = sum / 1024.0;
    if (squared / 1024.0 - mean * mean).max(0.0).sqrt() < 6.0 {
        return Err("This image appears blank or has too little visible detail. Take a new photo.");
    }
    Ok(mime)
}

fn encrypt(key: &[u8; 32], bytes: &[u8]) -> Result<(Vec<u8>, Vec<u8>), (StatusCode, String)> {
    use aes_gcm::aead::AeadCore;
    let cipher = Aes256Gcm::new_from_slice(key).map_err(database_error)?;
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher.encrypt(&nonce, bytes).map_err(database_error)?;
    Ok((nonce.to_vec(), ciphertext))
}

fn protected(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert("cache-control", "private, no-store".parse().unwrap());
    response
        .headers_mut()
        .insert("referrer-policy", "no-referrer".parse().unwrap());
    response
        .headers_mut()
        .insert("x-frame-options", "DENY".parse().unwrap());
    response
        .headers_mut()
        .insert("x-content-type-options", "nosniff".parse().unwrap());
    response.headers_mut().insert(
        "content-security-policy",
        "default-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'"
            .parse()
            .unwrap(),
    );
    response
}

fn page(secret: &str, name: &str, uploaded: bool, error: &str) -> Response {
    let content = if uploaded {
        format!("<section class='signing-action'><p class='signing-eyebrow'>Step 3 · Complete</p><h1>ID received</h1><p>Your signed lease and identity document have been submitted. Buildry will review them. You may return to your <a href='/lease/{}'>signed lease</a>.</p></section>", html_escape(secret))
    } else {
        format!(
            r#"<section class='signing-action photo-id-card'><p class='signing-eyebrow'>Step 3 · Identity document</p><h1>One more step</h1><p>Your lease is signed. Upload one valid government-issued photo ID so Buildry can verify that the signer matches the tenant named on the lease.</p><p class='photo-id-notice'>Buildry retains the images for identity verification and tenancy administration. Only authorized staff can access them. Read the <a href='/photo-id-terms' target='_blank' rel='noopener noreferrer'>Photo ID terms and privacy notice</a> before uploading.</p>{error}<form method='post' action='/lease/{secret}/photo-id' enctype='multipart/form-data' class='photo-id-form'><label for='document-type'>ID type</label><select id='document-type' name='document_type' required><option value=''>Choose an ID type</option><option value='drivers_licence'>Canadian driver’s licence</option><option value='photo_card'>Canadian provincial or territorial photo ID card</option><option value='passport'>Canadian or foreign passport</option></select><label for='id-front' id='front-label'>Front of ID</label><input id='id-front' name='front' type='file' accept='image/jpeg,image/png,image/webp' required><label for='id-back' id='back-label'>Back of ID</label><input id='id-back' name='back' type='file' accept='image/jpeg,image/png,image/webp' required><p class='photo-id-hint'>Use clear, uncropped images. JPEG, PNG, or WebP; up to 5 MB per image. For a passport, upload only the photo and information page.</p><label class='sign-consent'><input type='checkbox' name='terms' value='yes' required><span>I have read the <a href='/photo-id-terms' target='_blank' rel='noopener noreferrer'>Photo ID terms and privacy notice</a> and agree to submit this ID for the stated purpose.</span></label><button type='submit'>Submit ID securely</button></form></section><script type='module' src='/assets/photo-id.mjs'></script>"#,
            secret = html_escape(secret),
            error = if error.is_empty() {
                String::new()
            } else {
                format!(
                    "<p class='photo-id-error' role='alert'>{}</p>",
                    html_escape(error)
                )
            }
        )
    };
    protected(Html(format!("<!doctype html><html lang='en'><head><meta charset='utf-8'><meta name='viewport' content='width=device-width,initial-scale=1'><meta name='referrer' content='no-referrer'><title>Identity document | Buildry</title><link rel='stylesheet' href='/assets/main-20261012d.css'></head><body class='signing-body'><main class='signing-shell'><header class='signing-header'><div class='signing-brand'>Buildry <span>Leasing</span></div><span class='signing-private'>Private signing link</span></header><div class='signing-progress'><span class='complete'>1&nbsp; Review</span><span class='complete'>2&nbsp; Sign</span><span class='active'>3&nbsp; ID upload</span></div><section class='signing-intro'><p class='signing-eyebrow'>For {}</p></section>{}<footer class='signing-footer'>Your ID is stored separately from property photos.</footer></main></body></html>",html_escape(name),content)).into_response())
}

fn upload_error(secret: &str, status: StatusCode, message: &str) -> Response {
    let mut response = page(secret, "Tenant", false, message);
    *response.status_mut() = status;
    response
}

pub async fn upload_page(Path(secret): Path<String>, State(db): State<Database>) -> Response {
    let row = db.lock().await.query_opt("SELECT l.tenant_name, EXISTS(SELECT 1 FROM lease_photo_ids p WHERE p.lease_id=l.id) FROM leases l WHERE l.token_hash=$1 AND l.token_expires_at>NOW() AND l.status IN ('signed','confirmed') AND NOT l.is_demo", &[&secret_hash(&secret)]).await;
    match row {
        Ok(Some(row)) => page(&secret, &row.get::<_, String>(0), row.get(1), ""),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            "This ID upload link is invalid or expired. Contact Buildry for a new link.",
        )
            .into_response(),
        Err(error) => database_error(error).into_response(),
    }
}

pub async fn submit(
    Path(secret): Path<String>,
    State(db): State<Database>,
    mut form: Multipart,
) -> Response {
    let mut document_type = String::new();
    let mut terms = String::new();
    let mut front = None;
    let mut back = None;
    while let Ok(Some(field)) = form.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        if !matches!(name.as_str(), "document_type" | "terms" | "front" | "back") {
            continue;
        }
        let data = match field.bytes().await {
            Ok(data) => data,
            Err(_) => {
                return upload_error(
                    &secret,
                    StatusCode::BAD_REQUEST,
                    "Could not read an ID image.",
                )
            }
        };
        if data.len() > MAX_IMAGE {
            return upload_error(
                &secret,
                StatusCode::BAD_REQUEST,
                "Each image must be smaller than 5 MB.",
            );
        }
        match name.as_str() {
            "document_type" => document_type = String::from_utf8_lossy(&data).into_owned(),
            "terms" => terms = String::from_utf8_lossy(&data).into_owned(),
            "front" => front = Some(data.to_vec()),
            "back" => {
                if !data.is_empty() {
                    back = Some(data.to_vec())
                }
            }
            _ => {}
        }
    }
    let needs_back = matches!(document_type.as_str(), "drivers_licence" | "photo_card");
    if !needs_back && document_type != "passport" {
        return upload_error(
            &secret,
            StatusCode::BAD_REQUEST,
            "Choose an accepted ID type.",
        );
    }
    if terms != "yes" {
        return upload_error(
            &secret,
            StatusCode::BAD_REQUEST,
            "Read and accept the Photo ID terms before uploading.",
        );
    }
    let Some(front) = front else {
        return upload_error(
            &secret,
            StatusCode::BAD_REQUEST,
            "Upload the front of the ID.",
        );
    };
    if front.is_empty()
        || front.len() > MAX_IMAGE
        || (needs_back && back.is_none())
        || (!needs_back && back.is_some())
    {
        return upload_error(
            &secret,
            StatusCode::BAD_REQUEST,
            "Upload the required image or images for the selected ID type.",
        );
    }
    let front_type = match validate_image(&front) {
        Ok(kind) => kind,
        Err(message) => return upload_error(&secret, StatusCode::BAD_REQUEST, message),
    };
    let back_type = match back.as_ref() {
        Some(bytes) => match validate_image(bytes) {
            Ok(kind) => Some(kind),
            Err(message) => return upload_error(&secret, StatusCode::BAD_REQUEST, message),
        },
        None => None,
    };
    let key = match encryption_key() {
        Ok(key) => key,
        Err(error) => return error.into_response(),
    };
    let (front_nonce, front_ciphertext) = match encrypt(&key, &front) {
        Ok(value) => value,
        Err(error) => return error.into_response(),
    };
    let (back_nonce, back_ciphertext) = match back.as_ref() {
        Some(bytes) => match encrypt(&key, bytes) {
            Ok((nonce, data)) => (Some(nonce), Some(data)),
            Err(error) => return error.into_response(),
        },
        None => (None, None),
    };
    let hash = secret_hash(&secret);
    let mut client = db.lock().await;
    let tx = match client.transaction().await {
        Ok(tx) => tx,
        Err(error) => return database_error(error).into_response(),
    };
    let row = match tx.query_opt("SELECT id FROM leases WHERE token_hash=$1 AND token_expires_at>NOW() AND status IN ('signed','confirmed') AND NOT is_demo FOR UPDATE", &[&hash]).await { Ok(Some(row))=>row,Ok(None)=>return (StatusCode::NOT_FOUND,"This ID upload link is invalid or expired.").into_response(),Err(error)=>return database_error(error).into_response() };
    let lease_id: i64 = row.get(0);
    let inserted = tx.execute("INSERT INTO lease_photo_ids (lease_id,document_type,front_type,front_nonce,front_ciphertext,back_type,back_nonce,back_ciphertext,terms_version) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT (lease_id) DO NOTHING", &[&lease_id,&document_type,&front_type,&front_nonce,&front_ciphertext,&back_type,&back_nonce,&back_ciphertext,&TERMS_VERSION]).await;
    match inserted { Ok(1)=>{},Ok(_)=>return (StatusCode::CONFLICT,"An ID has already been uploaded for this lease. Contact Buildry if it needs replacing.").into_response(),Err(error)=>return database_error(error).into_response() }
    if let Err(error) = tx.commit().await {
        return database_error(error).into_response();
    }
    Redirect::to(&format!("/lease/{secret}/photo-id")).into_response()
}

pub async fn admin_image(
    Path((lease_id, side)): Path<(i64, String)>,
    State(db): State<Database>,
    auth: crate::auth::AuthSession,
) -> ApiResult<(HeaderMap, Vec<u8>)> {
    let user = auth
        .user
        .ok_or((StatusCode::UNAUTHORIZED, "Sign in required.".into()))?;
    let (type_column, nonce_column, data_column) = match side.as_str() {
        "front" => ("front_type", "front_nonce", "front_ciphertext"),
        "back" => ("back_type", "back_nonce", "back_ciphertext"),
        _ => return Err((StatusCode::NOT_FOUND, "ID image not found.".into())),
    };
    let sql = format!(
        "SELECT {type_column},{nonce_column},{data_column} FROM lease_photo_ids WHERE lease_id=$1"
    );
    let row = db
        .lock()
        .await
        .query_opt(&sql, &[&lease_id])
        .await
        .map_err(database_error)?
        .ok_or((StatusCode::NOT_FOUND, "ID image not found.".into()))?;
    let content_type: Option<String> = row.get(0);
    let nonce: Option<Vec<u8>> = row.get(1);
    let ciphertext: Option<Vec<u8>> = row.get(2);
    let (Some(content_type), Some(nonce), Some(ciphertext)) = (content_type, nonce, ciphertext)
    else {
        return Err((StatusCode::NOT_FOUND, "ID image not found.".into()));
    };
    let key = encryption_key()?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(database_error)?;
    if nonce.len() != 12 {
        return Err(database_error("Invalid stored ID nonce"));
    }
    let bytes = cipher
        .decrypt(Nonce::from_slice(&nonce), ciphertext.as_ref())
        .map_err(database_error)?;
    db.lock()
        .await
        .execute(
            "INSERT INTO lease_photo_id_access (lease_id,user_id,side) VALUES ($1,$2,$3)",
            &[&lease_id, &user.id, &side],
        )
        .await
        .map_err(database_error)?;
    let mut headers = HeaderMap::new();
    headers.insert(
        "content-type",
        content_type.parse().map_err(database_error)?,
    );
    headers.insert("content-disposition", "inline".parse().unwrap());
    headers.insert("cache-control", "private, no-store".parse().unwrap());
    headers.insert("referrer-policy", "no-referrer".parse().unwrap());
    headers.insert("x-content-type-options", "nosniff".parse().unwrap());
    headers.insert(
        "content-security-policy",
        "default-src 'none'; sandbox".parse().unwrap(),
    );
    Ok((headers, bytes))
}

pub async fn approve(
    Path(lease_id): Path<i64>,
    State(db): State<Database>,
    auth: crate::auth::AuthSession,
) -> ApiResult<StatusCode> {
    let user = auth
        .user
        .ok_or((StatusCode::UNAUTHORIZED, "Sign in required.".into()))?;
    let changed=db.lock().await.execute("UPDATE lease_photo_ids p SET verified_at=NOW(),verified_by=$2 WHERE p.lease_id=$1 AND p.verified_at IS NULL AND EXISTS(SELECT 1 FROM lease_photo_id_access a WHERE a.lease_id=$1 AND a.user_id=$2 AND a.side='front' AND a.viewed_at>=p.uploaded_at) AND (p.document_type='passport' OR EXISTS(SELECT 1 FROM lease_photo_id_access a WHERE a.lease_id=$1 AND a.user_id=$2 AND a.side='back' AND a.viewed_at>=p.uploaded_at))",&[&lease_id,&user.id]).await.map_err(database_error)?;
    if changed == 0 {
        return Err((
            StatusCode::CONFLICT,
            "Open and review each required ID image before approving it.".into(),
        ));
    }
    Ok(StatusCode::NO_CONTENT)
}

pub async fn reject(
    Path(lease_id): Path<i64>,
    State(db): State<Database>,
    auth: crate::auth::AuthSession,
) -> ApiResult<StatusCode> {
    let user = auth
        .user
        .ok_or((StatusCode::UNAUTHORIZED, "Sign in required.".into()))?;
    let mut client = db.lock().await;
    let tx = client.transaction().await.map_err(database_error)?;
    let changed = tx
        .execute(
            "DELETE FROM lease_photo_ids WHERE lease_id=$1 AND verified_at IS NULL",
            &[&lease_id],
        )
        .await
        .map_err(database_error)?;
    if changed == 0 {
        return Err((
            StatusCode::CONFLICT,
            "This ID is missing or has already been approved.".into(),
        ));
    }
    tx.execute(
        "INSERT INTO lease_photo_id_access (lease_id,user_id,side) VALUES ($1,$2,'rejected')",
        &[&lease_id, &user.id],
    )
    .await
    .map_err(database_error)?;
    tx.commit().await.map_err(database_error)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn terms_page() -> Response {
    protected(Html(format!(r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><meta name="referrer" content="no-referrer"><title>Photo ID terms and privacy notice | Buildry</title><link rel="stylesheet" href="/assets/main-20261012d.css"></head><body class="signing-body"><main class="signing-shell"><header class="signing-header"><div class="signing-brand">Buildry <span>Leasing</span></div><span class="signing-private">Privacy notice</span></header><article class="signing-action photo-id-terms"><p class="signing-eyebrow">Version {TERMS_VERSION}</p><h1>Photo ID terms and privacy notice</h1><p>After you sign a lease, Buildry asks for one valid government-issued photo ID to compare the name and photograph with the person signing the lease, prevent mistaken identity, and administer the tenancy.</p><h2>What to upload</h2><p>You may upload a Canadian provincial or territorial driver’s licence or photo ID card (front and back), or a Canadian or foreign passport (photo and information page only). The document must be current and the images clear. Do not upload a health card, Social Insurance Number card, or unrelated documents.</p><h2>How the images are used</h2><p>Authorized Buildry staff review the images for identity verification and tenancy administration. Buildry does not use them for advertising or publish them in property listings. If Buildry needs to disclose them for another purpose, it will seek your consent unless disclosure is required or permitted by law.</p><h2>Storage and access</h2><p>The images are encrypted before they are stored, kept apart from public property photos, and available only through staff access controls. Uploads use the private lease link. Keep that link confidential.</p><h2>Retention and deletion</h2><p>Buildry retains the ID images during the tenancy and automatically deletes them one year after Buildry records that the tenancy has ended, unless a legal preservation requirement applies. If you move out, contact Buildry to ensure the tenancy end date is recorded. Encrypted backups may persist through their normal backup cycle. You may request access or correction.</p><h2>Questions</h2><p>For privacy questions or to request a different way to verify your identity, contact the Buildry representative who sent your lease.</p></article></main></body></html>"#)).into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_supported_image_signatures_are_accepted() {
        assert_eq!(image_type(b"\xff\xd8\xff\xe0data"), Some("image/jpeg"));
        assert_eq!(image_type(b"\x89PNG\r\n\x1a\ndata"), Some("image/png"));
        assert_eq!(image_type(b"RIFFxxxxWEBPdata"), Some("image/webp"));
        assert_eq!(
            image_type(b"<svg xmlns='http://www.w3.org/2000/svg'>"),
            None
        );
        assert_eq!(image_type(b"%PDF-1.7"), None);
    }

    #[test]
    fn stored_image_is_encrypted_and_decryptable_with_the_key() {
        let key = [17u8; 32];
        let original = b"private ID image";
        let (nonce, ciphertext) = encrypt(&key, original).unwrap();
        assert_ne!(ciphertext, original);
        let cipher = Aes256Gcm::new_from_slice(&key).unwrap();
        assert_eq!(
            cipher
                .decrypt(Nonce::from_slice(&nonce), ciphertext.as_ref())
                .unwrap(),
            original
        );
        assert!(Aes256Gcm::new_from_slice(&[18u8; 32])
            .unwrap()
            .decrypt(Nonce::from_slice(&nonce), ciphertext.as_ref())
            .is_err());
    }

    #[test]
    fn decoded_image_must_be_large_readable_and_nonblank() {
        let mut pixels = image::RgbImage::new(800, 600);
        for (x, y, pixel) in pixels.enumerate_pixels_mut() {
            let shade = if (x / 40 + y / 40) % 2 == 0 { 30 } else { 220 };
            *pixel = image::Rgb([shade, shade, shade]);
        }
        let mut encoded = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(pixels)
            .write_to(&mut encoded, ImageFormat::Png)
            .unwrap();
        let bytes = encoded.into_inner();
        assert_eq!(validate_image(&bytes), Ok("image/png"));
        assert!(validate_image(&bytes[..bytes.len() / 2]).is_err());
        let mut blank = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            800,
            600,
            image::Rgb([255, 255, 255]),
        ))
        .write_to(&mut blank, ImageFormat::Png)
        .unwrap();
        assert!(validate_image(&blank.into_inner()).is_err());
        let mut tiny = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            100,
            100,
            image::Rgb([40, 40, 40]),
        ))
        .write_to(&mut tiny, ImageFormat::Png)
        .unwrap();
        assert!(validate_image(&tiny.into_inner()).is_err());
    }
}
