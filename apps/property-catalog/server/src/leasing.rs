use crate::{
    auth::{html_escape, Database},
    collapsible_panel, database_error, ApiResult,
};
use axum::{
    body::Bytes,
    extract::{Form, Path, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::env;
use tokio_postgres::Client;
use uuid::Uuid;

pub async fn migrate(db: &Client) -> Result<(), tokio_postgres::Error> {
    db.batch_execute("CREATE TABLE IF NOT EXISTS landlord_profile (
        id BIGINT PRIMARY KEY CHECK (id = 1), legal_name TEXT NOT NULL, service_address TEXT NOT NULL,
        email TEXT NOT NULL, phone TEXT NOT NULL, signer_name TEXT NOT NULL, signer_title TEXT NOT NULL,
        signature_authorized BOOLEAN NOT NULL DEFAULT FALSE, updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
    );
    CREATE TABLE IF NOT EXISTS leases (
        id BIGSERIAL PRIMARY KEY, tenant_name TEXT NOT NULL, tenant_email TEXT NOT NULL,
        property_id BIGINT NOT NULL REFERENCES properties(id), room_id BIGINT NOT NULL,
        details JSONB NOT NULL, landlord_snapshot JSONB NOT NULL,
        status TEXT NOT NULL CHECK (status IN ('draft','sent','signed','confirmed')),
        token_hash TEXT, token_expires_at TIMESTAMPTZ, landlord_signed_at TIMESTAMPTZ,
        tenant_signature TEXT, signed_at TIMESTAMPTZ, signed_ip TEXT, signed_user_agent TEXT,
        confirmed_at TIMESTAMPTZ, confirmed_by BIGINT REFERENCES users(id), copy_sent_at TIMESTAMPTZ,
        document_sha256 TEXT, signed_pdf_sha256 TEXT,
        created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
    );
    CREATE UNIQUE INDEX IF NOT EXISTS leases_token_hash_unique ON leases(token_hash) WHERE token_hash IS NOT NULL;
    CREATE TABLE IF NOT EXISTS password_invites (
        id BIGSERIAL PRIMARY KEY, user_id BIGINT NOT NULL REFERENCES users(id), token_hash TEXT NOT NULL UNIQUE,
        expires_at TIMESTAMPTZ NOT NULL, used_at TIMESTAMPTZ, created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
    );
    CREATE TABLE IF NOT EXISTS email_verifications (
        user_id BIGINT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE, token_hash TEXT NOT NULL UNIQUE,
        expires_at TIMESTAMPTZ NOT NULL, used_at TIMESTAMPTZ,
        sent_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
    );
    ALTER TABLE email_verifications ADD COLUMN IF NOT EXISTS sent_at TIMESTAMPTZ NOT NULL DEFAULT NOW();
    ALTER TABLE leases ADD COLUMN IF NOT EXISTS landlord_signed_at TIMESTAMPTZ;
    ALTER TABLE leases ADD COLUMN IF NOT EXISTS landlord_signature TEXT;
    ALTER TABLE leases ADD COLUMN IF NOT EXISTS landlord_token_hash TEXT;
    ALTER TABLE leases ADD COLUMN IF NOT EXISTS landlord_token_expires_at TIMESTAMPTZ;
    CREATE UNIQUE INDEX IF NOT EXISTS leases_landlord_token_unique ON leases(landlord_token_hash) WHERE landlord_token_hash IS NOT NULL;
    ALTER TABLE leases ADD COLUMN IF NOT EXISTS copy_sent_at TIMESTAMPTZ;
    ALTER TABLE leases ADD COLUMN IF NOT EXISTS document_sha256 TEXT;
    ALTER TABLE leases ADD COLUMN IF NOT EXISTS signed_pdf_sha256 TEXT;
    ALTER TABLE leases ADD COLUMN IF NOT EXISTS tenancy_ended_at DATE;
    ALTER TABLE leases ADD COLUMN IF NOT EXISTS tenancy_ended_by BIGINT REFERENCES users(id);
    ALTER TABLE leases ADD COLUMN IF NOT EXISTS tenancy_end_recorded_at TIMESTAMPTZ;
    CREATE TABLE IF NOT EXISTS landlords (
        id BIGSERIAL PRIMARY KEY, legal_name TEXT NOT NULL, service_address TEXT NOT NULL DEFAULT '',
        email TEXT NOT NULL DEFAULT '', phone TEXT NOT NULL DEFAULT '', signer_name TEXT NOT NULL DEFAULT '',
        signer_title TEXT NOT NULL DEFAULT '', signature_authorized BOOLEAN NOT NULL DEFAULT FALSE,
        updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
    );
    CREATE TABLE IF NOT EXISTS landlord_defaults (id BIGINT PRIMARY KEY CHECK (id=1), landlord_id BIGINT NOT NULL REFERENCES landlords(id));
    ALTER TABLE properties ADD COLUMN IF NOT EXISTS landlord_id BIGINT REFERENCES landlords(id);
    ALTER TABLE leases ADD COLUMN IF NOT EXISTS tour_id BIGINT REFERENCES tour_bookings(id);
    ALTER TABLE leases ADD COLUMN IF NOT EXISTS is_seeded BOOLEAN NOT NULL DEFAULT FALSE;
    ALTER TABLE leases ADD COLUMN IF NOT EXISTS seed_key TEXT;
    ALTER TABLE leases ADD COLUMN IF NOT EXISTS lease_type TEXT NOT NULL DEFAULT 'room';
    CREATE TABLE IF NOT EXISTS lease_templates (
        id BIGSERIAL PRIMARY KEY, name TEXT NOT NULL, property_id BIGINT NOT NULL REFERENCES properties(id),
        room_id BIGINT NOT NULL, lease_type TEXT NOT NULL, details JSONB NOT NULL,
        landlord_snapshot JSONB NOT NULL, landlord_token_hash TEXT UNIQUE,
        landlord_token_expires_at TIMESTAMPTZ, landlord_signature TEXT,
        landlord_signed_at TIMESTAMPTZ, updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
    );
    ALTER TABLE lease_templates ADD COLUMN IF NOT EXISTS landlord_id BIGINT REFERENCES landlords(id);
    ALTER TABLE lease_templates ALTER COLUMN property_id DROP NOT NULL;
    ALTER TABLE lease_templates ADD COLUMN IF NOT EXISTS additional_pdf BYTEA;
    ALTER TABLE leases ADD COLUMN IF NOT EXISTS additional_pdf BYTEA;
    UPDATE lease_templates t SET landlord_id=p.landlord_id FROM properties p WHERE t.property_id=p.id AND t.landlord_id IS NULL;
    UPDATE lease_templates SET
        details=details - ARRAY['start_date','end_date','term_type','other_term','monthly_rent','rent_due_day','rent_deposit','rent_discount','partial_rent','unit_description','occupants'],
        landlord_snapshot=jsonb_set(jsonb_set(landlord_snapshot,'{address}',to_jsonb(''::text)),'{room}',to_jsonb(''::text)),
        room_id=0,lease_type='room'
      WHERE landlord_signed_at IS NULL AND (room_id<>0 OR COALESCE(details->>'start_date','')<>'');
    ALTER TABLE leases ADD COLUMN IF NOT EXISTS template_id BIGINT REFERENCES lease_templates(id);
    ALTER TABLE leases ADD COLUMN IF NOT EXISTS is_demo BOOLEAN NOT NULL DEFAULT FALSE;
    UPDATE leases SET is_demo=TRUE
      WHERE is_seeded AND seed_key LIKE 'occupancy-%'
        AND lower(tenant_email) LIKE '%@example.invalid' AND NOT is_demo;
    ALTER TABLE leases DROP CONSTRAINT IF EXISTS leases_lease_type_check;
    ALTER TABLE leases ADD CONSTRAINT leases_lease_type_check CHECK (lease_type IN ('room','entire_house'));
    CREATE UNIQUE INDEX IF NOT EXISTS leases_seed_key_unique ON leases(seed_key) WHERE seed_key IS NOT NULL;
    UPDATE leases SET lease_type='entire_house',room_id=0,
        details=jsonb_set(details,'{unit_description}',to_jsonb('Entire house'::text)),
        landlord_snapshot=jsonb_set(landlord_snapshot,'{room}',to_jsonb('Entire house'::text))
      WHERE seed_key='occupancy-priya-confirmed' AND is_seeded AND lease_type='room';
    UPDATE leases SET details=jsonb_set(
        jsonb_set(details,'{start_date}',to_jsonb('2026-10-01'::text)),
        '{end_date}',to_jsonb('2027-09-30'::text))
      WHERE seed_key='occupancy-priya-confirmed' AND is_seeded
        AND details->>'start_date'='2026-12-01' AND details->>'end_date'='2027-11-30';
    INSERT INTO landlords (legal_name,service_address,email,phone,signer_name,signer_title,signature_authorized)
    SELECT legal_name,service_address,email,phone,signer_name,signer_title,signature_authorized
    FROM landlord_profile WHERE id=1 AND NOT EXISTS (SELECT 1 FROM landlords);
    INSERT INTO landlords (legal_name) SELECT 'Helen Bai' WHERE NOT EXISTS (SELECT 1 FROM landlord_defaults) AND NOT EXISTS (SELECT 1 FROM landlords WHERE lower(legal_name)='helen bai');
    INSERT INTO landlord_defaults (id,landlord_id) SELECT 1,id FROM landlords WHERE lower(legal_name)='helen bai' ORDER BY id LIMIT 1 ON CONFLICT (id) DO NOTHING;
    UPDATE properties SET landlord_id=(SELECT landlord_id FROM landlord_defaults WHERE id=1) WHERE landlord_id IS NULL;
    DROP INDEX IF EXISTS leases_tour_unique;
    CREATE INDEX IF NOT EXISTS leases_tour_idx ON leases(tour_id) WHERE tour_id IS NOT NULL;
    DELETE FROM leases WHERE is_demo OR is_seeded OR seed_key IS NOT NULL OR lower(tenant_email) LIKE '%@example.invalid';").await
}

fn token() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}
fn digest(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
fn base_url() -> ApiResult<String> {
    let url = env::var("APP_PUBLIC_URL").map_err(|_| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "APP_PUBLIC_URL must be configured before sending email.".into(),
        )
    })?;
    let url = reqwest::Url::parse(url.trim()).map_err(|_| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "APP_PUBLIC_URL must be a public HTTPS origin before sending signing links.".into(),
        )
    })?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "APP_PUBLIC_URL must be a public HTTPS origin before sending signing links.".into(),
        ));
    }
    Ok(url.as_str().trim_end_matches('/').to_string())
}
pub(crate) async fn email(to: &str, subject: &str, html: String) -> ApiResult<()> {
    email_payload(to, subject, html, None).await
}
pub(crate) async fn email_smoke_test(to: &str) -> ApiResult<()> {
    if !to.contains('@') || to.chars().any(char::is_whitespace) {
        return Err((
            StatusCode::BAD_REQUEST,
            "Enter one valid test recipient email address.".into(),
        ));
    }
    email_payload(
        to,
        "Buildry leasing email test",
        "<p>This is a Buildry leasing email test. The attached file checks that lease attachments can be sent.</p>".into(),
        Some(("buildry-email-test.txt", b"Buildry leasing email attachment test.\n".to_vec())),
    ).await
}
#[derive(Serialize)]
pub struct EmailStatus {
    configured: bool,
}
pub async fn email_status() -> Json<EmailStatus> {
    Json(EmailStatus {
        configured: env::var("RESEND_API_KEY").is_ok_and(|value| !value.trim().is_empty())
            && env::var("RESEND_FROM_EMAIL").is_ok_and(|value| !value.trim().is_empty())
            && env::var("APP_PUBLIC_URL").is_ok_and(|value| !value.trim().is_empty()),
    })
}
async fn email_payload(
    to: &str,
    subject: &str,
    html: String,
    attachment: Option<(&str, Vec<u8>)>,
) -> ApiResult<()> {
    let key = env::var("RESEND_API_KEY").map_err(|_| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Email is not configured.".into(),
        )
    })?;
    let from = env::var("RESEND_FROM_EMAIL").map_err(|_| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "RESEND_FROM_EMAIL must be configured.".into(),
        )
    })?;
    let html = format!("{html}<p style=\"color:#667; font-size:12px\">This is an automated message. Replies to this address are not monitored.</p>");
    send_email_request(
        "https://api.resend.com/emails",
        &key,
        &from,
        to,
        subject,
        html,
        attachment,
    )
    .await
}
async fn send_email_request(
    url: &str,
    key: &str,
    from: &str,
    to: &str,
    subject: &str,
    html: String,
    attachment: Option<(&str, Vec<u8>)>,
) -> ApiResult<()> {
    let mut body = serde_json::json!({"from":from,"to":[to],"subject":subject,"html":html});
    if let Some((filename, bytes)) = attachment {
        use base64::Engine;
        body["attachments"] = serde_json::json!([{"filename":filename,"content":base64::engine::general_purpose::STANDARD.encode(bytes)}]);
    }
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(database_error)?;
    let result = http
        .post(url)
        .bearer_auth(key)
        .json(&body)
        .send()
        .await
        .map_err(|error| {
            eprintln!("email transport: {error}");
            (StatusCode::BAD_GATEWAY, "Could not send email.".into())
        })?;
    if !result.status().is_success() {
        eprintln!("Resend rejected email with HTTP {}", result.status());
        return Err((
            StatusCode::BAD_GATEWAY,
            "Email provider rejected the message. Check sender verification and configuration."
                .into(),
        ));
    }
    let receipt: serde_json::Value = result.json().await.map_err(|_| {
        (
            StatusCode::BAD_GATEWAY,
            "Email provider returned an invalid receipt.".into(),
        )
    })?;
    if receipt["id"].as_str().unwrap_or("").is_empty() {
        return Err((
            StatusCode::BAD_GATEWAY,
            "Email provider did not confirm the message.".into(),
        ));
    }
    Ok(())
}

pub async fn send_verification(db: &Database, user_id: i64, to: &str, name: &str) -> ApiResult<()> {
    let secret = token();
    let hash = digest(&secret);
    let link = format!("{}/verify-email/{secret}", base_url()?);
    db.lock().await.execute("INSERT INTO email_verifications (user_id,token_hash,expires_at) VALUES ($1,$2,NOW()+INTERVAL '24 hours') ON CONFLICT (user_id) DO UPDATE SET token_hash=$2,expires_at=NOW()+INTERVAL '24 hours',used_at=NULL,sent_at=NOW()", &[&user_id,&hash]).await.map_err(database_error)?;
    email(to,"Verify your Buildry email",format!("<p>Hello {},</p><p><a href=\"{}\">Verify your email address</a> to finish creating your Buildry account. This link expires in 24 hours.</p>",html_escape(name),html_escape(&link))).await
}
pub async fn send_password_reset(db: &Database, to: &str) -> ApiResult<()> {
    let secret = token();
    let hash = digest(&secret);
    let link = format!("{}/set-password/{secret}", base_url()?);
    let row = db.lock().await.query_opt(
        "INSERT INTO password_invites (user_id,token_hash,expires_at) SELECT u.id,$2,NOW()+INTERVAL '1 hour' FROM users u WHERE lower(u.email)=lower($1) AND u.active AND NOT EXISTS (SELECT 1 FROM password_invites p WHERE p.user_id=u.id AND p.created_at>NOW()-INTERVAL '2 minutes') RETURNING user_id",
        &[&to, &hash],
    ).await.map_err(database_error)?;
    if row.is_none() {
        return Ok(());
    }
    if let Err(error) = email(to, "Reset your Buildry password", format!("<p>We received a request to reset your Buildry password.</p><p><a href=\"{}\">Set a new password</a>. This link expires in one hour.</p><p>If you did not request this, you can ignore this email.</p>", html_escape(&link))).await {
        let _ = db.lock().await.execute("DELETE FROM password_invites WHERE token_hash=$1", &[&hash]).await;
        return Err(error);
    }
    Ok(())
}
pub async fn verify_email_page(Path(secret): Path<String>, State(db): State<Database>) -> Response {
    let valid = db.lock().await.query_opt(
        "SELECT user_id FROM email_verifications WHERE token_hash=$1 AND used_at IS NULL AND expires_at>NOW()",
        &[&digest(&secret)],
    ).await.ok().flatten().is_some();
    if !valid {
        return (StatusCode::NOT_FOUND, Html("<!doctype html><html lang=\"en\"><head><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>Verification link expired | Buildry</title><link rel=\"stylesheet\" href=\"/assets/main.css\"></head><body class=\"auth-body\"><main class=\"auth-card\"><h1>This link is invalid or expired</h1><p>Request a new verification email to finish creating your account.</p><a href=\"/resend-verification\">Resend verification email</a></main></body></html>".to_string())).into_response();
    }
    Html(format!("<!doctype html><html lang=\"en\"><head><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>Verify email | Buildry</title><link rel=\"stylesheet\" href=\"/assets/main.css\"></head><body class=\"auth-body\"><main class=\"auth-card\"><h1>Verify your email</h1><p>Confirm your email address to activate your Buildry account.</p><form method=\"post\" action=\"/verify-email/{}\"><button type=\"submit\">Verify email</button></form></main></body></html>", html_escape(&secret))).into_response()
}
pub async fn verify_email(Path(secret): Path<String>, State(db): State<Database>) -> Response {
    let hash = digest(&secret);
    let mut client = db.lock().await;
    let tx = match client.transaction().await {
        Ok(v) => v,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let row=match tx.query_opt("UPDATE email_verifications SET used_at=NOW() WHERE token_hash=$1 AND used_at IS NULL AND expires_at>NOW() RETURNING user_id", &[&hash]).await{Ok(Some(v))=>v,_=>return Redirect::to("/resend-verification").into_response()};
    let id: i64 = row.get(0);
    if tx
        .execute(
            "UPDATE users SET active=TRUE WHERE id=$1 AND role='guest'",
            &[&id],
        )
        .await
        .is_err()
        || tx.commit().await.is_err()
    {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    Redirect::to("/login?verified=1").into_response()
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Landlord {
    id: i64,
    legal_name: String,
    service_address: String,
    email: String,
    phone: String,
    signer_name: String,
    signer_title: String,
    signature_authorized: bool,
    #[serde(default)]
    property_ids: Vec<i64>,
    #[serde(default)]
    photo_url: String,
}
fn landlord_row(row: tokio_postgres::Row) -> Landlord {
    Landlord {
        id: row.get(0),
        legal_name: row.get(1),
        service_address: row.get(2),
        email: row.get(3),
        phone: row.get(4),
        signer_name: row.get(5),
        signer_title: row.get(6),
        signature_authorized: row.get(7),
        property_ids: row.get(8),
        photo_url: row.get(9),
    }
}
const LANDLORD_SELECT: &str = "SELECT l.id,l.legal_name,l.service_address,l.email,l.phone,l.signer_name,l.signer_title,l.signature_authorized,COALESCE(array_agg(p.id ORDER BY p.id) FILTER (WHERE p.id IS NOT NULL),'{}'),COALESCE((SELECT u.photo_url FROM users u WHERE u.landlord_id=l.id AND u.role='landlord' ORDER BY u.active DESC,u.id LIMIT 1),'') FROM landlords l LEFT JOIN properties p ON p.landlord_id=l.id GROUP BY l.id ORDER BY l.id";
pub async fn list_landlords(State(db): State<Database>, auth: crate::auth::AuthSession) -> ApiResult<Json<Vec<Landlord>>> {
    let user = crate::scope::user(&auth)?;
    let client = db.lock().await;
    let ids = crate::scope::property_ids(&client, user.id).await?;
    let own_landlord_id: Option<i64> = client.query_one("SELECT landlord_id FROM users WHERE id=$1", &[&user.id]).await.map_err(database_error)?.get(0);
    let rows = client
        .query(LANDLORD_SELECT, &[])
        .await
        .map_err(database_error)?;
    Ok(Json(rows.into_iter().map(landlord_row).filter_map(|mut landlord| {
        if user.role == "admin" { return Some(landlord); }
        landlord.property_ids.retain(|id| ids.contains(id));
        (user.role == "landlord" && Some(landlord.id) == own_landlord_id || !landlord.property_ids.is_empty()).then_some(landlord)
    }).collect()))
}
fn validate_landlord(input: &Landlord) -> ApiResult<()> {
    if input.legal_name.trim().is_empty()
        || [
            input.legal_name.len(),
            input.service_address.len(),
            input.email.len(),
            input.phone.len(),
            input.signer_name.len(),
            input.signer_title.len(),
        ]
        .iter()
        .any(|n| *n > 500)
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "Enter a landlord name (maximum 500 characters per field).".into(),
        ));
    }
    if input.signature_authorized
        && (input.service_address.trim().is_empty()
            || !input.email.contains('@')
            || input.signer_name.trim().is_empty()
            || input.signer_title.trim().is_empty())
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "Complete legal contact and signer details before authorizing signatures.".into(),
        ));
    }
    Ok(())
}
async fn assign_properties(client: &mut Client, id: i64, ids: &[i64]) -> ApiResult<()> {
    let tx = client.transaction().await.map_err(database_error)?;
    if !ids.is_empty() {
        let count: i64 = tx
            .query_one("SELECT count(*) FROM properties WHERE id=ANY($1)", &[&ids])
            .await
            .map_err(database_error)?
            .get(0);
        if count != ids.len() as i64 {
            return Err((
                StatusCode::BAD_REQUEST,
                "One or more properties do not exist.".into(),
            ));
        }
    }
    tx.execute(
        "UPDATE properties SET landlord_id=(SELECT landlord_id FROM landlord_defaults WHERE id=1) WHERE landlord_id=$1 AND id <> ALL($2::bigint[])",
        &[&id, &ids],
    )
    .await
    .map_err(database_error)?;
    tx.execute(
        "UPDATE properties SET landlord_id=$1 WHERE id=ANY($2)",
        &[&id, &ids],
    )
    .await
    .map_err(database_error)?;
    tx.execute("UPDATE leases AS draft SET landlord_snapshot=jsonb_set(draft.landlord_snapshot,'{landlord}',to_jsonb(l)) FROM properties p JOIN landlords l ON l.id=p.landlord_id WHERE draft.property_id=p.id AND draft.status='draft'",&[]).await.map_err(database_error)?;
    tx.commit().await.map_err(database_error)?;
    Ok(())
}
pub async fn create_landlord(
    State(db): State<Database>,
    Json(input): Json<Landlord>,
) -> ApiResult<Json<Landlord>> {
    validate_landlord(&input)?;
    let mut client = db.lock().await;
    let id:i64=client.query_one("INSERT INTO landlords (legal_name,service_address,email,phone,signer_name,signer_title,signature_authorized) VALUES ($1,$2,$3,$4,$5,$6,$7) RETURNING id",&[&input.legal_name.trim(),&input.service_address.trim(),&input.email.trim(),&input.phone.trim(),&input.signer_name.trim(),&input.signer_title.trim(),&input.signature_authorized]).await.map_err(database_error)?.get(0);
    assign_properties(&mut client, id, &input.property_ids).await?;
    Ok(Json(Landlord { id, ..input }))
}
pub async fn save_landlord(
    Path(id): Path<i64>,
    State(db): State<Database>,
    Json(input): Json<Landlord>,
) -> ApiResult<Json<Landlord>> {
    validate_landlord(&input)?;
    let mut client = db.lock().await;
    let changed=client.execute("UPDATE landlords SET legal_name=$2,service_address=$3,email=$4,phone=$5,signer_name=$6,signer_title=$7,signature_authorized=$8,updated_at=NOW() WHERE id=$1", &[&id,&input.legal_name.trim(),&input.service_address.trim(),&input.email.trim(),&input.phone.trim(),&input.signer_name.trim(),&input.signer_title.trim(),&input.signature_authorized]).await.map_err(database_error)?;
    if changed == 0 {
        return Err((StatusCode::NOT_FOUND, "Landlord not found.".into()));
    }
    assign_properties(&mut client, id, &input.property_ids).await?;
    Ok(Json(Landlord { id, ..input }))
}

#[derive(Serialize)]
pub struct AccessUser {
    id: i64,
    first_name: String,
    last_name: String,
    email: String,
    phone: String,
    role: String,
    active: bool,
    password_set: bool,
    google_linked: bool,
    landlord_id: Option<i64>,
    photo_url: String,
}
pub async fn users(State(db): State<Database>) -> ApiResult<Json<Vec<AccessUser>>> {
    let rows = db.lock().await.query("SELECT id,first_name,last_name,email,phone,role,active,password_hash <> '' AND password_hash NOT LIKE 'google-only:%',landlord_id,photo_url,google_sub IS NOT NULL FROM users ORDER BY id", &[]).await.map_err(database_error)?;
    Ok(Json(
        rows.into_iter()
            .map(|r| AccessUser {
                id: r.get(0),
                first_name: r.get(1),
                last_name: r.get(2),
                email: r.get(3),
                phone: r.get(4),
                role: r.get(5),
                active: r.get(6),
                password_set: r.get(7),
                landlord_id: r.get(8),
                photo_url: r.get(9),
                google_linked: r.get(10),
            })
            .collect(),
    ))
}
#[derive(Deserialize)]
pub struct AccessUpdate {
    first_name: String,
    last_name: String,
    email: String,
    #[serde(default)]
    phone: String,
    role: String,
    active: bool,
    landlord_id: Option<i64>,
    #[serde(default)]
    photo_url: Option<String>,
}
fn validate_access(input: &AccessUpdate) -> ApiResult<()> {
    if input.first_name.trim().is_empty()
        || input.last_name.trim().is_empty()
        || input.first_name.len() > 60
        || input.last_name.len() > 60
        || !input.email.contains('@')
        || input.email.len() > 254
        || input.phone.len() > 40
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "Enter a first and last name, valid email, and optional phone number.".into(),
        ));
    }
    if !["guest", "tenant", "landlord", "staff", "admin"].contains(&input.role.as_str()) {
        return Err((StatusCode::BAD_REQUEST, "Choose a valid role.".into()));
    }
    if input.role == "landlord" && input.landlord_id.is_none() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Choose a landlord profile for this account.".into(),
        ));
    }
    if input.role != "landlord" && input.landlord_id.is_some() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Only landlord accounts can link to a landlord profile.".into(),
        ));
    }
    if input
        .photo_url
        .as_deref()
        .is_some_and(|url| !url.is_empty() && (!url.starts_with("/images/") || url.len() > 200))
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "Choose an uploaded profile image.".into(),
        ));
    }
    Ok(())
}
pub async fn create_user(
    State(db): State<Database>,
    Json(input): Json<AccessUpdate>,
) -> ApiResult<Json<AccessUser>> {
    validate_access(&input)?;
    let client = db.lock().await;
    if let Some(landlord_id) = input.landlord_id {
        if client
            .query_opt("SELECT id FROM landlords WHERE id=$1", &[&landlord_id])
            .await
            .map_err(database_error)?
            .is_none()
        {
            return Err((
                StatusCode::BAD_REQUEST,
                "Landlord profile not found.".into(),
            ));
        }
    }
    if client
        .query_opt(
            "SELECT id FROM users WHERE lower(email)=lower($1)",
            &[&input.email.trim()],
        )
        .await
        .map_err(database_error)?
        .is_some()
    {
        return Err((
            StatusCode::CONFLICT,
            "An account already uses this email.".into(),
        ));
    }
    let row=client.query_one("INSERT INTO users (first_name,last_name,email,phone,role,active,password_hash,landlord_id,photo_url) VALUES ($1,$2,lower($3),$4,$5,$6,'',$7,$8) RETURNING id,first_name,last_name,email,phone,role,active,password_hash <> '',landlord_id,photo_url",&[&input.first_name.trim(),&input.last_name.trim(),&input.email.trim(),&input.phone.trim(),&input.role,&input.active,&input.landlord_id,&input.photo_url.clone().unwrap_or_default()]).await.map_err(database_error)?;
    let user_id: i64 = row.get(0);
    if ["admin", "staff", "landlord"].contains(&input.role.as_str()) {
        crate::avatar::ensure_user_avatar(&client, user_id)
            .await
            .map_err(database_error)?;
    }
    Ok(Json(AccessUser {
        id: row.get(0),
        first_name: row.get(1),
        last_name: row.get(2),
        email: row.get(3),
        phone: row.get(4),
        role: row.get(5),
        active: row.get(6),
        password_set: row.get(7),
        google_linked: false,
        landlord_id: row.get(8),
        photo_url: if input.photo_url.as_deref().unwrap_or_default().is_empty()
            && ["admin", "staff", "landlord"].contains(&input.role.as_str())
        {
            client
                .query_one("SELECT photo_url FROM users WHERE id=$1", &[&user_id])
                .await
                .map_err(database_error)?
                .get(0)
        } else {
            row.get(9)
        },
    }))
}
pub async fn update_user(
    Path(id): Path<i64>,
    State(db): State<Database>,
    Json(input): Json<AccessUpdate>,
) -> ApiResult<StatusCode> {
    validate_access(&input)?;
    let mut client = db.lock().await;
    let tx = client.transaction().await.map_err(database_error)?;
    let target = tx
        .query_opt("SELECT role,email,active FROM users WHERE id=$1 FOR UPDATE", &[&id])
        .await
        .map_err(database_error)?
        .ok_or((StatusCode::NOT_FOUND, "User not found.".into()))?;
    let old_role: String = target.get(0);
    let old_email: String = target.get(1);
    let old_active: bool = target.get(2);
    let revoke_sessions = !old_email.eq_ignore_ascii_case(input.email.trim())
        || old_role != input.role
        || old_active != input.active;
    if old_role == "admin" && (input.role != "admin" || !input.active) {
        let count: i64 = tx
            .query_one(
                "SELECT count(*) FROM users WHERE role='admin' AND active AND password_hash<>''",
                &[],
            )
            .await
            .map_err(database_error)?
            .get(0);
        if count <= 1 {
            return Err((
                StatusCode::CONFLICT,
                "Keep at least one active admin.".into(),
            ));
        }
    }
    if let Some(landlord_id) = input.landlord_id {
        if tx
            .query_opt("SELECT id FROM landlords WHERE id=$1", &[&landlord_id])
            .await
            .map_err(database_error)?
            .is_none()
        {
            return Err((
                StatusCode::BAD_REQUEST,
                "Landlord profile not found.".into(),
            ));
        }
    }
    if tx
        .query_opt(
            "SELECT id FROM users WHERE lower(email)=lower($1) AND id<>$2",
            &[&input.email.trim(), &id],
        )
        .await
        .map_err(database_error)?
        .is_some()
    {
        return Err((
            StatusCode::CONFLICT,
            "An account already uses this email.".into(),
        ));
    }
    if !old_email.eq_ignore_ascii_case(input.email.trim()) || !input.active {
        tx.execute("UPDATE password_invites SET used_at=NOW() WHERE user_id=$1 AND used_at IS NULL", &[&id]).await.map_err(database_error)?;
        tx.execute("DELETE FROM email_verifications WHERE user_id=$1", &[&id]).await.map_err(database_error)?;
    }
    tx.execute("UPDATE users SET first_name=$2,last_name=$3,email=lower($4),phone=$5,role=$6,active=$7,landlord_id=$8,photo_url=COALESCE($9,photo_url),google_sub=CASE WHEN lower(email)=lower($4) THEN google_sub ELSE NULL END,auth_version=auth_version+CASE WHEN $10 THEN 1 ELSE 0 END,is_seeded=CASE WHEN lower($4) LIKE '%@example.invalid' THEN is_seeded ELSE FALSE END WHERE id=$1", &[&id,&input.first_name.trim(),&input.last_name.trim(),&input.email.trim(),&input.phone.trim(),&input.role,&input.active,&input.landlord_id,&input.photo_url,&revoke_sessions]).await.map_err(database_error)?;
    tx.commit().await.map_err(database_error)?;
    if ["admin", "staff", "landlord"].contains(&input.role.as_str()) {
        crate::avatar::ensure_user_avatar(&client, id)
            .await
            .map_err(database_error)?;
    }
    Ok(StatusCode::NO_CONTENT)
}
pub async fn invite_user(Path(id): Path<i64>, State(db): State<Database>) -> ApiResult<StatusCode> {
    let url = base_url()?;
    let row = db
        .lock()
        .await
        .query_opt(
            "SELECT first_name,email,active,password_hash FROM users WHERE id=$1",
            &[&id],
        )
        .await
        .map_err(database_error)?
        .ok_or((StatusCode::NOT_FOUND, "Account not found.".into()))?;
    let name: String = row.get(0);
    let to: String = row.get(1);
    let active: bool = row.get(2);
    let pass: String = row.get(3);
    if !active {
        return Err((
            StatusCode::CONFLICT,
            "Activate this account before sending a password email.".into(),
        ));
    }
    if to.to_lowercase().ends_with("@example.invalid") {
        return Err((
            StatusCode::BAD_REQUEST,
            "Confirm the account email before sending setup instructions.".into(),
        ));
    }
    let secret = token();
    let hash = digest(&secret);
    let password_set = !pass.is_empty() && !pass.starts_with("google-only:");
    let (subject, action, expiry) = if password_set {
        ("Reset your Buildry password", "Reset your password", "1 hour")
    } else {
        ("Set up your Buildry account", "Set your password", "7 days")
    };
    let interval = if password_set { "1 hour" } else { "7 days" };
    let inserted = db.lock().await.query_opt("INSERT INTO password_invites (user_id,token_hash,expires_at) SELECT $1,$2,NOW()+$3::interval WHERE NOT EXISTS (SELECT 1 FROM password_invites WHERE user_id=$1 AND created_at>NOW()-INTERVAL '2 minutes') RETURNING id", &[&id,&hash,&interval]).await.map_err(database_error)?;
    if inserted.is_none() {
        return Err((StatusCode::TOO_MANY_REQUESTS, "A password email was sent recently. Try again in two minutes.".into()));
    }
    if let Err(error) = email(&to,subject,format!("<p>Hello {},</p><p><a href=\"{}/set-password/{}\">{}</a> to access Buildry. This link expires in {}.</p><p>If you did not expect this email, you can ignore it.</p>",html_escape(&name),html_escape(&url),secret,action,expiry)).await {
        let _ = db.lock().await.execute("DELETE FROM password_invites WHERE token_hash=$1", &[&hash]).await;
        return Err(error);
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct LeaseDetails {
    pub(crate) start_date: String,
    pub(crate) end_date: String,
    pub(crate) term_type: String,
    pub(crate) monthly_rent: String,
    pub(crate) rent_due_day: String,
    pub(crate) rent_payment_method: String,
    pub(crate) parking: String,
    pub(crate) condo: String,
    pub(crate) services: String,
    pub(crate) utilities: String,
    pub(crate) rent_deposit: String,
    pub(crate) key_deposit: String,
    pub(crate) smoking: String,
    pub(crate) insurance: String,
    pub(crate) additional_terms: String,
    pub(crate) unit_description: String,
    pub(crate) occupants: String,
    pub(crate) notes: String,
    pub(crate) email_notices: bool,
    pub(crate) landlord_notice_email: String,
    pub(crate) landlord_contact_details: String,
    pub(crate) gas_included: bool,
    pub(crate) ac_included: bool,
    pub(crate) storage_included: bool,
    pub(crate) laundry_included: bool,
    pub(crate) guest_parking_included: bool,
    pub(crate) laundry_fee: String,
    pub(crate) electricity_payer: String,
    pub(crate) heat_payer: String,
    pub(crate) water_payer: String,
    pub(crate) rent_discount: String,
    pub(crate) partial_rent: String,
    pub(crate) nsf_fee: String,
    pub(crate) key_description: String,
    pub(crate) other_term: String,
}
#[derive(Serialize, Deserialize)]
pub struct LeaseTemplate {
    id: i64,
    name: String,
    property_id: Option<i64>,
    room_id: i64,
    lease_type: String,
    landlord_id: Option<i64>,
    details: LeaseDetails,
    landlord_signed_at: Option<String>,
    landlord_signature: Option<String>,
    has_additional_pdf: bool,
}
#[derive(Deserialize)]
pub struct TemplateInput {
    name: String,
    landlord_id: i64,
    details: LeaseDetails,
}
fn template_row(r: tokio_postgres::Row) -> ApiResult<LeaseTemplate> {
    let raw: String = r.get(5);
    Ok(LeaseTemplate {
        id: r.get(0),
        name: r.get(1),
        property_id: r.get(2),
        room_id: r.get(3),
        lease_type: r.get(4),
        details: serde_json::from_str(&raw).map_err(database_error)?,
        landlord_signed_at: r.get(6),
        landlord_signature: r.get(7),
        landlord_id: r.get(8),
        has_additional_pdf: r.get(9),
    })
}
const TEMPLATE_COLUMNS:&str="id,name,property_id,room_id,lease_type,details::text,landlord_signed_at::text,landlord_signature,landlord_id,additional_pdf IS NOT NULL";
pub async fn list_templates(State(db): State<Database>, auth: crate::auth::AuthSession) -> ApiResult<Json<Vec<LeaseTemplate>>> {
    let user = crate::scope::user(&auth)?;
    let client = db.lock().await;
    let ids = crate::scope::property_ids(&client, user.id).await?;
    let landlord_ids: Vec<i64> = client.query("SELECT DISTINCT landlord_id FROM properties WHERE id=ANY($1::bigint[]) AND landlord_id IS NOT NULL", &[&ids]).await.map_err(database_error)?.into_iter().map(|row| row.get(0)).collect();
    let own_landlord_id: Option<i64> = client.query_one("SELECT landlord_id FROM users WHERE id=$1", &[&user.id]).await.map_err(database_error)?.get(0);
    let rows = client
        .query(
            &format!(
                "SELECT {TEMPLATE_COLUMNS} FROM lease_templates ORDER BY updated_at DESC,id DESC"
            ),
            &[],
        )
        .await
        .map_err(database_error)?;
    Ok(Json(
        rows.into_iter().filter(|row| {
            let landlord_id: Option<i64> = row.get(8);
            user.role == "admin"
                || landlord_id.is_some_and(|id| landlord_ids.contains(&id) || (user.role == "landlord" && Some(id) == own_landlord_id))
                || (landlord_id.is_none() && row.get::<_, Option<i64>>(2).is_some_and(|id| ids.contains(&id)))
        })
            .map(template_row)
            .collect::<ApiResult<Vec<_>>>()?,
    ))
}
fn validate_template(input: &TemplateInput) -> ApiResult<()> {
    if input.name.trim().is_empty() || input.name.len() > 120 || input.landlord_id <= 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Enter a template name and select a landlord.".into(),
        ));
    }
    let d = &input.details;
    if d.email_notices
        && (d.landlord_notice_email.len() > 90
            || (!d.landlord_notice_email.trim().is_empty()
                && (!d.landlord_notice_email.contains('@')
                    || d.landlord_notice_email.chars().any(char::is_whitespace)))
            || d.landlord_contact_details.len() > 110)
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "Enter a valid landlord notice email and contact details.".into(),
        ));
    }
    if d.rent_payment_method.trim().is_empty()
        || d.parking.trim().is_empty()
        || !["Yes", "No"].contains(&d.condo.as_str())
        || [&d.electricity_payer, &d.heat_payer, &d.water_payer]
            .iter()
            .any(|payer| payer.as_str() != "landlord" && payer.as_str() != "tenant")
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "Complete payment, parking, condominium and utility responsibilities.".into(),
        ));
    }
    Ok(())
}
async fn template_snapshot(client: &Client, input: &TemplateInput) -> ApiResult<String> {
    let row=client.query_opt("SELECT id,legal_name,service_address,email,phone,signer_name,signer_title,signature_authorized FROM landlords WHERE id=$1",&[&input.landlord_id]).await.map_err(database_error)?.ok_or((StatusCode::BAD_REQUEST,"Choose an existing landlord.".into()))?;
    let landlord = Landlord {
        id: row.get(0),
        legal_name: row.get(1),
        service_address: row.get(2),
        email: row.get(3),
        phone: row.get(4),
        signer_name: row.get(5),
        signer_title: row.get(6),
        signature_authorized: row.get(7),
        property_ids: vec![],
        photo_url: String::new(),
    };
    serde_json::to_string(&serde_json::json!({"landlord":landlord,"address":"","room":""}))
        .map_err(database_error)
}
pub async fn save_template(
    Path(id): Path<i64>,
    State(db): State<Database>,
    auth: crate::auth::AuthSession,
    Json(input): Json<TemplateInput>,
) -> ApiResult<Json<LeaseTemplate>> {
    let user = crate::scope::user(&auth)?;
    validate_template(&input)?;
    let client = db.lock().await;
    if user.role == "landlord" {
        let own_landlord_id: Option<i64> = client.query_one("SELECT landlord_id FROM users WHERE id=$1", &[&user.id]).await.map_err(database_error)?.get(0);
        if own_landlord_id != Some(input.landlord_id) {
            return Err((StatusCode::FORBIDDEN, "Choose your own landlord profile.".into()));
        }
    }
    let snapshot = template_snapshot(&client, &input).await?;
    let mut terms = input.details.clone();
    clear_lease_specific_terms(&mut terms);
    let details = serde_json::to_string(&terms).map_err(database_error)?;
    if details.len() > 20_000 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Template details are too long.".into(),
        ));
    }
    let anchor: Option<i64> = client
        .query_opt(
            &format!("SELECT p.id FROM properties p WHERE p.landlord_id=$1 AND {} ORDER BY p.id LIMIT 1", crate::scope::property_sql("p", 2)),
            &[&input.landlord_id, &user.id],
        )
        .await
        .map_err(database_error)?
        .map(|row| row.get(0));
    if anchor.is_none() && user.role != "admin" && user.role != "landlord" {
        return Err((StatusCode::FORBIDDEN, "Choose a landlord for a property you manage.".into()));
    }
    let row = if id == 0 {
        client.query_one(&format!("INSERT INTO lease_templates (name,property_id,room_id,lease_type,details,landlord_snapshot,landlord_id) VALUES ($1,$2,0,'room',$3::text::jsonb,$4::text::jsonb,$5) RETURNING {TEMPLATE_COLUMNS}"),&[&input.name.trim(),&anchor,&details,&snapshot,&input.landlord_id]).await.map_err(database_error)?
    } else {
        client.query_opt(&format!("UPDATE lease_templates SET name=$2,property_id=$3,room_id=0,lease_type='room',details=$4::text::jsonb,landlord_snapshot=$5::text::jsonb,landlord_id=$6,landlord_signature=NULL,landlord_signed_at=NULL,landlord_token_hash=NULL,landlord_token_expires_at=NULL,updated_at=NOW() WHERE id=$1 RETURNING {TEMPLATE_COLUMNS}"),&[&id,&input.name.trim(),&anchor,&details,&snapshot,&input.landlord_id]).await.map_err(database_error)?.ok_or((StatusCode::NOT_FOUND,"Template not found.".into()))?
    };
    Ok(Json(template_row(row)?))
}
pub async fn upload_template_pdf(
    Path(id): Path<i64>,
    State(db): State<Database>,
    bytes: Bytes,
) -> ApiResult<StatusCode> {
    if bytes.is_empty() || bytes.len() > 5_000_000 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Choose a PDF smaller than 5 MB.".into(),
        ));
    }
    crate::lease_pdf::validate_supplement(&bytes)
        .map_err(|message| (StatusCode::BAD_REQUEST, message))?;
    let updated = db
        .lock()
        .await
        .execute(
            "UPDATE lease_templates SET additional_pdf=$2,updated_at=NOW() WHERE id=$1",
            &[&id, &bytes.to_vec()],
        )
        .await
        .map_err(database_error)?;
    if updated == 0 {
        return Err((StatusCode::NOT_FOUND, "Template not found.".into()));
    }
    Ok(StatusCode::NO_CONTENT)
}
pub async fn remove_template_pdf(
    Path(id): Path<i64>,
    State(db): State<Database>,
) -> ApiResult<StatusCode> {
    let updated = db
        .lock()
        .await
        .execute(
            "UPDATE lease_templates SET additional_pdf=NULL,updated_at=NOW() WHERE id=$1",
            &[&id],
        )
        .await
        .map_err(database_error)?;
    if updated == 0 {
        return Err((StatusCode::NOT_FOUND, "Template not found.".into()));
    }
    Ok(StatusCode::NO_CONTENT)
}
pub async fn email_template_pdf(
    Path(id): Path<i64>,
    State(db): State<Database>,
) -> ApiResult<StatusCode> {
    let row = db
        .lock()
        .await
        .query_opt(
            "SELECT name,landlord_snapshot::text FROM lease_templates WHERE id=$1",
            &[&id],
        )
        .await
        .map_err(database_error)?
        .ok_or((StatusCode::NOT_FOUND, "Template not found.".into()))?;
    let name: String = row.get(0);
    let snapshot: serde_json::Value =
        serde_json::from_str(&row.get::<_, String>(1)).map_err(database_error)?;
    let to = snapshot["landlord"]["email"].as_str().unwrap_or("");
    if !to.contains('@') || to.to_ascii_lowercase().ends_with("@example.invalid") {
        return Err((
            StatusCode::CONFLICT,
            "Set a deliverable landlord email before sending the template.".into(),
        ));
    }
    let pdf = crate::lease_pdf::bytes_for_template_id(&db, id).await?;
    email_payload(
        to,
        &format!("Updated Buildry lease template · {}", name.replace(['\r','\n'], " ")),
        format!("<p>The updated lease template <strong>{}</strong> is attached for your review.</p><p>This is a template preview; tenant and stay details are added to each completed lease.</p>",html_escape(&name)),
        Some(("updated-lease-template.pdf",pdf)),
    ).await?;
    Ok(StatusCode::NO_CONTENT)
}
fn clear_lease_specific_terms(d: &mut LeaseDetails) {
    d.start_date.clear();
    d.end_date.clear();
    d.term_type.clear();
    d.other_term.clear();
    d.monthly_rent.clear();
    d.rent_due_day.clear();
    d.rent_deposit.clear();
    d.rent_discount.clear();
    d.partial_rent.clear();
    d.unit_description.clear();
    d.occupants.clear();
}
fn apply_reusable_terms(value: &mut LeaseDetails, terms: &LeaseDetails) {
    value.rent_payment_method = terms.rent_payment_method.clone();
    value.parking = terms.parking.clone();
    value.condo = terms.condo.clone();
    value.services = terms.services.clone();
    value.utilities = terms.utilities.clone();
    value.key_deposit = terms.key_deposit.clone();
    value.smoking = terms.smoking.clone();
    value.insurance = terms.insurance.clone();
    value.additional_terms = terms.additional_terms.clone();
    value.notes = terms.notes.clone();
    value.email_notices = terms.email_notices;
    value.landlord_notice_email = terms.landlord_notice_email.clone();
    value.landlord_contact_details = terms.landlord_contact_details.clone();
    value.gas_included = terms.gas_included;
    value.ac_included = terms.ac_included;
    value.storage_included = terms.storage_included;
    value.laundry_included = terms.laundry_included;
    value.guest_parking_included = terms.guest_parking_included;
    value.laundry_fee = terms.laundry_fee.clone();
    value.electricity_payer = terms.electricity_payer.clone();
    value.heat_payer = terms.heat_payer.clone();
    value.water_payer = terms.water_payer.clone();
    value.nsf_fee = terms.nsf_fee.clone();
    value.key_description = terms.key_description.clone();
}
pub async fn send_template_to_landlord(
    Path(id): Path<i64>,
    State(db): State<Database>,
) -> ApiResult<Json<SentLink>> {
    let url = base_url()?;
    let secret = token();
    let hash = digest(&secret);
    let client = db.lock().await;
    let row = client
        .query_opt(
            "SELECT name,landlord_snapshot::text FROM lease_templates WHERE id=$1",
            &[&id],
        )
        .await
        .map_err(database_error)?
        .ok_or((StatusCode::NOT_FOUND, "Template not found.".into()))?;
    let name: String = row.get(0);
    let snapshot: serde_json::Value =
        serde_json::from_str(&row.get::<_, String>(1)).map_err(database_error)?;
    let landlord = &snapshot["landlord"];
    if landlord["signature_authorized"] != true
        || landlord["signer_name"].as_str().unwrap_or("").is_empty()
    {
        return Err((
            StatusCode::CONFLICT,
            "Complete landlord signing authority first.".into(),
        ));
    }
    let to = landlord["email"].as_str().unwrap_or("");
    if !to.contains('@') || to.ends_with("@example.invalid") {
        return Err((
            StatusCode::CONFLICT,
            "Set a deliverable landlord email before requesting a signature.".into(),
        ));
    }
    let link = format!("{url}/lease-template/{secret}");
    client.execute("UPDATE lease_templates SET landlord_token_hash=$2,landlord_token_expires_at=NOW()+INTERVAL '14 days' WHERE id=$1",&[&id,&hash]).await.map_err(database_error)?;
    drop(client);
    email_payload(to,"Review and sign a Buildry lease template",format!("<p>The lease template <strong>{}</strong> is ready for your review. Open the private link to inspect its terms and adopt your electronic signature. The link expires in 14 days.</p><p><a href=\"{}\">Review and sign template</a></p>",html_escape(&name),html_escape(&link)),None).await?;
    Ok(Json(SentLink { url: link }))
}
pub async fn template_signing_page(
    Path(secret): Path<String>,
    State(db): State<Database>,
) -> Response {
    let hash = digest(&secret);
    let row=match db.lock().await.query_opt("SELECT name,details::text,landlord_snapshot::text,landlord_signed_at IS NOT NULL FROM lease_templates WHERE landlord_token_hash=$1 AND landlord_token_expires_at>NOW()",&[&hash]).await{Ok(Some(row))=>row,_=>return (StatusCode::NOT_FOUND,"This template signing link is invalid or expired.").into_response()};
    let name: String = row.get(0);
    let details: String = row.get(1);
    let snapshot: String = row.get(2);
    let signed: bool = row.get(3);
    let d: LeaseDetails = match serde_json::from_str(&details) {
        Ok(v) => v,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let s: serde_json::Value = match serde_json::from_str(&snapshot) {
        Ok(v) => v,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let signer = s["landlord"]["signer_name"].as_str().unwrap_or("");
    let details_view = lease_details_view(&d, &s, None);
    let action = if signed {
        "<div class='sign-complete' role='status'><span aria-hidden='true'>✓</span><div><strong>Template signed</strong><p>Your signature appears on page 7 of the PDF.</p></div></div>".to_string()
    } else {
        format!("<form class='sign-form' method='post' data-signature-form data-kind='landlord'><label for='landlord-signature'>Your legal name</label><input id='landlord-signature' name='signature' required maxlength='120' autocomplete='name' value='{}'><label class='sign-consent'><input type='checkbox' name='consent' value='yes' required><span>I reviewed the template and agree to sign it electronically. The guest, home, dates and rent are set on each lease.</span></label><button type='button' data-preview-signature>Preview signature</button><div class='sign-preview' data-sign-preview hidden><span>Signature preview</span><strong></strong></div><button type='submit' data-submit-signature hidden>Submit signature</button></form>",html_escape(signer))
    };
    let document = html_escape(&format!("/lease-template/{secret}/document?v=signature-2"));
    let pdf_viewer = pdf_viewer(&document, "landlord", signed);
    let html = format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><meta name="referrer" content="no-referrer"><title>Review lease template | Buildry</title><link rel="stylesheet" href="/assets/main-20261017d.css"><script type="module" src="/assets/lease-viewer-20261017b.mjs"></script><script type="module" src="/assets/signing-flow-20261012c.mjs"></script></head><body class="signing-body"><main class="signing-shell"><header class="signing-header"><div class="signing-brand">Buildry <span>Leasing</span></div><span class="signing-private">Private landlord link</span></header><section class="signing-intro"><p class="signing-eyebrow">Landlord review</p><h1>{name}</h1></section><section class="signing-document"><div class="signing-section-head"><div><h2>Ontario lease template</h2><p>Guest and stay details are added to each lease.</p></div><a href="{document}" target="_blank" rel="noopener noreferrer">Open PDF ↗</a></div>{details_view}{pdf_viewer}</section><section class="signing-action"><p class="signing-eyebrow">Landlord signature</p><h2>{action_heading}</h2>{action}</section><footer class="signing-footer">Private link for {recipient}.</footer></main></body></html>"#,
        name = html_escape(&name),
        action_heading = if signed {
            "Complete"
        } else {
            "Sign this template"
        },
        recipient = html_escape(s["landlord"]["email"].as_str().unwrap_or(""))
    );
    let mut response = Html(html).into_response();
    response
        .headers_mut()
        .insert("cache-control", "private, no-store".parse().unwrap());
    response
        .headers_mut()
        .insert("referrer-policy", "no-referrer".parse().unwrap());
    response
        .headers_mut()
        .insert("x-frame-options", "DENY".parse().unwrap());
    response.headers_mut().insert(
        "content-security-policy",
        "frame-ancestors 'none'".parse().unwrap(),
    );
    response
}
pub async fn sign_template(
    Path(secret): Path<String>,
    State(db): State<Database>,
    Form(input): Form<SignatureInput>,
) -> Response {
    if input.consent != "yes" || input.signature.trim().len() < 2 {
        return (
            StatusCode::BAD_REQUEST,
            "Enter your name and consent to sign.",
        )
            .into_response();
    }
    let hash = digest(&secret);
    let client = db.lock().await;
    let row=match client.query_opt("SELECT landlord_snapshot::text FROM lease_templates WHERE landlord_token_hash=$1 AND landlord_token_expires_at>NOW()",&[&hash]).await{Ok(Some(row))=>row,_=>return (StatusCode::NOT_FOUND,"Link expired.").into_response()};
    let snapshot: serde_json::Value = match serde_json::from_str(&row.get::<_, String>(0)) {
        Ok(v) => v,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    if !snapshot["landlord"]["signer_name"]
        .as_str()
        .unwrap_or("")
        .eq_ignore_ascii_case(input.signature.trim())
    {
        return (
            StatusCode::CONFLICT,
            "Use the authorized signer's full legal name.",
        )
            .into_response();
    }
    if client.execute("UPDATE lease_templates SET landlord_signature=$2,landlord_signed_at=NOW() WHERE landlord_token_hash=$1 AND landlord_token_expires_at>NOW() AND landlord_signed_at IS NULL",&[&hash,&input.signature.trim()]).await.is_err(){return StatusCode::INTERNAL_SERVER_ERROR.into_response()}
    Redirect::to(&format!("/lease-template/{secret}")).into_response()
}
fn room_lease_type() -> String {
    "room".into()
}
fn valid_lease_type(value: &str, room_id: i64) -> bool {
    matches!(
        (value, room_id),
        ("room", 1..=i64::MAX) | ("entire_house", 0)
    )
}
async fn ensure_no_signed_lease_overlap(
    client: &Client,
    property_id: i64,
    room_id: i64,
    details: &LeaseDetails,
    exclude_id: Option<i64>,
) -> ApiResult<()> {
    let start =
        chrono::NaiveDate::parse_from_str(&details.start_date, "%Y-%m-%d").map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                "Enter a valid tenancy start date.".into(),
            )
        })?;
    let end = if details.end_date.trim().is_empty() {
        "9999-12-31".to_string()
    } else {
        let end =
            chrono::NaiveDate::parse_from_str(&details.end_date, "%Y-%m-%d").map_err(|_| {
                (
                    StatusCode::BAD_REQUEST,
                    "Enter a valid tenancy end date.".into(),
                )
            })?;
        if end < start {
            return Err((
                StatusCode::BAD_REQUEST,
                "End date must follow start date.".into(),
            ));
        }
        end.to_string()
    };
    let occupied=client.query_opt("SELECT id FROM leases WHERE ($1::bigint IS NULL OR id<>$1) AND property_id=$2 AND (room_id=$3 OR room_id=0 OR $3=0) AND status IN ('signed','confirmed') AND NOT is_demo AND details->>'start_date'<=$4 AND COALESCE(NULLIF(details->>'end_date',''),'9999-12-31')>=$5 LIMIT 1",&[&exclude_id,&property_id,&room_id,&end,&details.start_date]).await.map_err(database_error)?;
    if occupied.is_some() {
        return Err((StatusCode::CONFLICT,"This rental unit already has a signed lease for these dates. Choose another unit or dates.".into()));
    }
    Ok(())
}
#[derive(Deserialize)]
pub struct LeaseInput {
    tenant_name: String,
    tenant_email: String,
    property_id: i64,
    room_id: i64,
    #[serde(default = "room_lease_type")]
    lease_type: String,
    details: LeaseDetails,
    template_id: Option<i64>,
}
#[derive(Serialize)]
pub struct Lease {
    id: i64,
    tenant_name: String,
    tenant_email: String,
    property_id: i64,
    room_id: i64,
    lease_type: String,
    tour_id: Option<i64>,
    template_id: Option<i64>,
    is_demo: bool,
    details: LeaseDetails,
    status: String,
    signed_at: Option<String>,
    landlord_signed_at: Option<String>,
    confirmed_at: Option<String>,
    copy_sent_at: Option<String>,
    photo_id_uploaded: bool,
    photo_id_verified: bool,
    photo_id_type: Option<String>,
    tenancy_ended_at: Option<String>,
}
fn lease_row(r: tokio_postgres::Row) -> ApiResult<Lease> {
    let raw: String = r.get(5);
    Ok(Lease {
        id: r.get(0),
        tenant_name: r.get(1),
        tenant_email: r.get(2),
        property_id: r.get(3),
        room_id: r.get(4),
        details: serde_json::from_str(&raw).map_err(database_error)?,
        status: r.get(6),
        signed_at: r.get(7),
        landlord_signed_at: r.get(14),
        confirmed_at: r.get(8),
        copy_sent_at: r.get(9),
        tour_id: r.get(10),
        lease_type: r.get(11),
        template_id: r.get(12),
        is_demo: r.get(13),
        photo_id_uploaded: if r.len() > 15 { r.get(15) } else { false },
        photo_id_verified: if r.len() > 16 { r.get(16) } else { false },
        photo_id_type: if r.len() > 17 { r.get(17) } else { None },
        tenancy_ended_at: if r.len() > 18 { r.get(18) } else { None },
    })
}
const LEASE_COLUMNS:&str="id,tenant_name,tenant_email,property_id,room_id,details::text,status,signed_at::text,confirmed_at::text,copy_sent_at::text,tour_id,lease_type,template_id,is_demo,landlord_signed_at::text";
pub async fn list_leases(State(db): State<Database>, auth: crate::auth::AuthSession) -> ApiResult<Json<Vec<Lease>>> {
    let user = crate::scope::user(&auth)?;
    let client = db.lock().await;
    let ids = crate::scope::property_ids(&client, user.id).await?;
    let rows=client.query(&format!("SELECT {LEASE_COLUMNS}, EXISTS(SELECT 1 FROM lease_photo_ids p WHERE p.lease_id=leases.id), EXISTS(SELECT 1 FROM lease_photo_ids p WHERE p.lease_id=leases.id AND p.verified_at IS NOT NULL), (SELECT p.document_type FROM lease_photo_ids p WHERE p.lease_id=leases.id),tenancy_ended_at::text FROM leases ORDER BY id DESC"), &[]).await.map_err(database_error)?;
    Ok(Json(
        rows.into_iter().filter(|row| ids.contains(&row.get::<_, i64>(3)))
            .map(lease_row)
            .collect::<ApiResult<Vec<_>>>()?,
    ))
}
pub async fn create_lease(
    State(db): State<Database>,
    auth: crate::auth::AuthSession,
    Json(input): Json<LeaseInput>,
) -> ApiResult<Json<Lease>> {
    let user = crate::scope::user(&auth)?;
    crate::scope::require_property_write(&*db.lock().await, user.id, input.property_id).await?;
    if let Some(template_id) = input.template_id {
        crate::scope::require_entity(&db, user.id, "lease_templates", template_id).await?;
    }
    if input.tenant_name.trim().is_empty()
        || input.tenant_name.len() > 120
        || !input.tenant_email.contains('@')
        || input.tenant_email.len() > 254
        || input.details.start_date.is_empty()
        || input
            .details
            .monthly_rent
            .parse::<u32>()
            .ok()
            .filter(|v| *v > 0)
            .is_none()
        || input
            .details
            .rent_due_day
            .parse::<u8>()
            .ok()
            .filter(|v| (1..=31).contains(v))
            .is_none()
        || !valid_lease_type(&input.lease_type, input.room_id)
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "Enter a tenant name, email, start date, monthly rent, and rent due day.".into(),
        ));
    }
    let client = db.lock().await;
    let landlord=client.query_opt("SELECT l.id,l.legal_name,l.service_address,l.email,l.phone,l.signer_name,l.signer_title,l.signature_authorized,ARRAY[p.id] FROM properties p JOIN landlords l ON l.id=p.landlord_id WHERE p.id=$1",&[&input.property_id]).await.map_err(database_error)?.map(landlord_row).ok_or((StatusCode::CONFLICT,"Assign this property to a landlord first.".into()))?;
    let property = client
        .query_opt(
            "SELECT data->>'address' FROM properties WHERE id=$1",
            &[&input.property_id],
        )
        .await
        .map_err(database_error)?
        .ok_or((StatusCode::BAD_REQUEST, "Property not found.".into()))?;
    let address: Option<String> = property.get(0);
    if address.as_deref().unwrap_or("").trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Enter the property's address first.".into(),
        ));
    }
    let room_name = if input.lease_type == "entire_house" {
        "Entire house".to_string()
    } else {
        client.query_opt("SELECT data->>'name' FROM property_spaces WHERE property_id=$1 AND id=$2 AND data->>'kind' IN ('Bedroom','Suite')", &[&input.property_id,&input.room_id]).await.map_err(database_error)?
            .ok_or((StatusCode::BAD_REQUEST,"Select an existing bedroom or suite.".into()))?.get::<_,Option<String>>(0).unwrap_or_default()
    };
    let mut lease_details = input.details.clone();
    if input.lease_type == "entire_house" || lease_details.unit_description == "Entire house" {
        lease_details.unit_description = room_name.clone();
    }
    ensure_no_signed_lease_overlap(
        &client,
        input.property_id,
        input.room_id,
        &lease_details,
        None,
    )
    .await?;
    let snapshot = serde_json::to_string(
        &serde_json::json!({"landlord":landlord,"address":address,"room":room_name}),
    )
    .map_err(database_error)?;
    let mut additional_pdf: Option<Vec<u8>> = None;
    if let Some(template_id) = input.template_id {
        let template = client
            .query_opt(
                "SELECT landlord_id,details::text,additional_pdf FROM lease_templates WHERE id=$1",
                &[&template_id],
            )
            .await
            .map_err(database_error)?
            .ok_or((StatusCode::BAD_REQUEST, "Template not found.".into()))?;
        if template.get::<_, Option<i64>>(0) != Some(landlord.id) {
            return Err((
                StatusCode::CONFLICT,
                "Choose a template for this landlord.".into(),
            ));
        }
        let terms: LeaseDetails =
            serde_json::from_str(&template.get::<_, String>(1)).map_err(database_error)?;
        apply_reusable_terms(&mut lease_details, &terms);
        additional_pdf = template.get(2);
    }
    let details = serde_json::to_string(&lease_details).map_err(database_error)?;
    if details.len() > 20_000 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Lease details are too long.".into(),
        ));
    }
    let row=client.query_one(&format!("INSERT INTO leases (tenant_name,tenant_email,property_id,room_id,lease_type,details,landlord_snapshot,status,template_id,additional_pdf) VALUES ($1,lower($2),$3,$4,$5,$6::text::jsonb,$7::text::jsonb,'draft',$8,$9) RETURNING {LEASE_COLUMNS}"), &[&input.tenant_name.trim(),&input.tenant_email.trim(),&input.property_id,&input.room_id,&input.lease_type,&details,&snapshot,&input.template_id,&additional_pdf]).await.map_err(database_error)?;
    let lease = lease_row(row)?;
    drop(client);
    if let Err(error) = send_lease(Path(lease.id), State(db.clone())).await {
        db.lock()
            .await
            .execute(
                "DELETE FROM leases WHERE id=$1 AND status='draft'",
                &[&lease.id],
            )
            .await
            .map_err(database_error)?;
        return Err(error);
    }
    let row = db
        .lock()
        .await
        .query_one(
            &format!("SELECT {LEASE_COLUMNS} FROM leases WHERE id=$1"),
            &[&lease.id],
        )
        .await
        .map_err(database_error)?;
    Ok(Json(lease_row(row)?))
}
pub async fn update_lease(
    Path(id): Path<i64>,
    State(db): State<Database>,
    auth: crate::auth::AuthSession,
    Json(input): Json<LeaseInput>,
) -> ApiResult<Json<Lease>> {
    let user = crate::scope::user(&auth)?;
    crate::scope::require_property_write(&*db.lock().await, user.id, input.property_id).await?;
    if input.tenant_name.trim().is_empty()
        || input.tenant_name.len() > 120
        || !input.tenant_email.contains('@')
        || input.tenant_email.len() > 254
        || chrono::NaiveDate::parse_from_str(&input.details.start_date, "%Y-%m-%d").is_err()
        || input
            .details
            .monthly_rent
            .parse::<u32>()
            .ok()
            .filter(|v| *v > 0)
            .is_none()
        || !valid_lease_type(&input.lease_type, input.room_id)
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "Enter valid tenant, rental unit, start date, and rent details.".into(),
        ));
    }
    let client = db.lock().await;
    let room_name = if input.lease_type == "entire_house" {
        "Entire house".to_string()
    } else {
        client.query_opt("SELECT data->>'name' FROM property_spaces WHERE property_id=$1 AND id=$2 AND data->>'kind' IN ('Bedroom','Suite')", &[&input.property_id,&input.room_id]).await.map_err(database_error)?
            .ok_or((StatusCode::BAD_REQUEST,"Select an existing bedroom or suite.".into()))?.get::<_,Option<String>>(0).unwrap_or_default()
    };
    let mut lease_details = input.details.clone();
    if input.lease_type == "entire_house" || lease_details.unit_description == "Entire house" {
        lease_details.unit_description = room_name.clone();
    }
    ensure_no_signed_lease_overlap(
        &client,
        input.property_id,
        input.room_id,
        &lease_details,
        Some(id),
    )
    .await?;
    let details = serde_json::to_string(&lease_details).map_err(database_error)?;
    if details.len() > 20_000 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Lease details are too long.".into(),
        ));
    }
    let row=client.query_opt(&format!("UPDATE leases SET tenant_name=$2,tenant_email=lower($3),details=$4::text::jsonb,room_id=$6,lease_type=$7,landlord_snapshot=jsonb_set(landlord_snapshot,'{{room}}',to_jsonb($8::text)),template_id=NULL,additional_pdf=NULL,landlord_signed_at=NULL WHERE id=$1 AND status='draft' AND property_id=$5 RETURNING {LEASE_COLUMNS}"), &[&id,&input.tenant_name.trim(),&input.tenant_email.trim(),&details,&input.property_id,&input.room_id,&input.lease_type,&room_name]).await.map_err(database_error)?.ok_or((StatusCode::CONFLICT,"Only an unsent draft in this property can be edited.".into()))?;
    Ok(Json(lease_row(row)?))
}
#[derive(Deserialize)]
pub struct ApplyTemplateInput {
    template_id: i64,
}
pub async fn apply_template(
    Path(id): Path<i64>,
    State(db): State<Database>,
    auth: crate::auth::AuthSession,
    Json(input): Json<ApplyTemplateInput>,
) -> ApiResult<Json<Lease>> {
    let user = crate::scope::user(&auth)?;
    crate::scope::require_entity(&db, user.id, "lease_templates", input.template_id).await?;
    let client = db.lock().await;
    let lease = client
        .query_opt(
            "SELECT property_id,room_id,lease_type,details::text,status FROM leases WHERE id=$1",
            &[&id],
        )
        .await
        .map_err(database_error)?
        .ok_or((StatusCode::NOT_FOUND, "Lease not found.".into()))?;
    if lease.get::<_, String>(4) != "draft" {
        return Err((
            StatusCode::CONFLICT,
            "Only a draft can use a template.".into(),
        ));
    }
    let template = client
        .query_opt(
            "SELECT landlord_id,details::text,additional_pdf FROM lease_templates WHERE id=$1",
            &[&input.template_id],
        )
        .await
        .map_err(database_error)?
        .ok_or((StatusCode::NOT_FOUND, "Template not found.".into()))?;
    let lease_landlord: Option<i64> = client
        .query_one(
            "SELECT landlord_id FROM properties WHERE id=$1",
            &[&lease.get::<_, i64>(0)],
        )
        .await
        .map_err(database_error)?
        .get(0);
    if template.get::<_, Option<i64>>(0) != lease_landlord {
        return Err((
            StatusCode::CONFLICT,
            "Choose a template for this property's landlord.".into(),
        ));
    }
    let current: LeaseDetails =
        serde_json::from_str(&lease.get::<_, String>(3)).map_err(database_error)?;
    let terms: LeaseDetails =
        serde_json::from_str(&template.get::<_, String>(1)).map_err(database_error)?;
    let mut value = current;
    apply_reusable_terms(&mut value, &terms);
    let details = serde_json::to_string(&value).map_err(database_error)?;
    let additional_pdf: Option<Vec<u8>> = template.get(2);
    let row=client.query_one(&format!("UPDATE leases SET details=$2::text::jsonb,template_id=$3,additional_pdf=$4 WHERE id=$1 RETURNING {LEASE_COLUMNS}"),&[&id,&details,&input.template_id,&additional_pdf]).await.map_err(database_error)?;
    Ok(Json(lease_row(row)?))
}
#[derive(Deserialize)]
pub struct TourLeaseInput {
    monthly_rent: u32,
    property_id: Option<i64>,
    room_id: Option<i64>,
    #[serde(default = "room_lease_type")]
    lease_type: String,
    start_date: Option<String>,
    months: Option<i32>,
    tenant_email: Option<String>,
    template_id: i64,
}
pub async fn create_from_tour(
    Path(tour_id): Path<i64>,
    State(db): State<Database>,
    auth: crate::auth::AuthSession,
    Json(input): Json<TourLeaseInput>,
) -> ApiResult<Json<Lease>> {
    let user = crate::scope::user(&auth)?;
    if let Some(property_id) = input.property_id {
        crate::scope::require_property_write(&*db.lock().await, user.id, property_id).await?;
    }
    crate::scope::require_entity(&db, user.id, "lease_templates", input.template_id).await?;
    if input.monthly_rent == 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Enter a monthly rent greater than zero.".into(),
        ));
    }
    let client = db.lock().await;
    let tour=client.query_opt("SELECT property_id,room_id,guest_name,guest_email,rental_start,rental_months,cancelled_at IS NOT NULL,legal_first_name,legal_last_name FROM tour_bookings WHERE id=$1",&[&tour_id]).await.map_err(database_error)?.ok_or((StatusCode::NOT_FOUND,"Prospect not found.".into()))?;
    if tour.get::<_, bool>(6) {
        return Err((
            StatusCode::CONFLICT,
            "A cancelled tour cannot start a lease.".into(),
        ));
    }
    let property_id = input.property_id.unwrap_or_else(|| tour.get(0));
    let room_id = if input.lease_type == "entire_house" {
        0
    } else {
        input
            .room_id
            .or_else(|| tour.get(1))
            .ok_or((StatusCode::BAD_REQUEST, "Choose a room or suite.".into()))?
    };
    if !valid_lease_type(&input.lease_type, room_id) {
        return Err((
            StatusCode::BAD_REQUEST,
            "Choose a room, suite, or entire house.".into(),
        ));
    }
    let email = input.tenant_email.unwrap_or_else(|| tour.get(3));
    if email.len() > 254 || !email.contains('@') {
        return Err((
            StatusCode::BAD_REQUEST,
            "Enter the prospect's email address.".into(),
        ));
    }
    let start = input
        .start_date
        .or_else(|| tour.get(4))
        .ok_or((StatusCode::BAD_REQUEST, "Choose a move-in date.".into()))?;
    let months = input
        .months
        .or_else(|| tour.get(5))
        .ok_or((StatusCode::BAD_REQUEST, "Choose a rental duration.".into()))?;
    if !(1..=120).contains(&months) {
        return Err((
            StatusCode::BAD_REQUEST,
            "Choose a duration from 1 to 120 months.".into(),
        ));
    }
    let start_date = chrono::NaiveDate::parse_from_str(&start, "%Y-%m-%d").map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            "Invalid rental start date on tour.".into(),
        )
    })?;
    let end_date = start_date
        .checked_add_months(chrono::Months::new(months as u32))
        .and_then(|date| date.pred_opt())
        .ok_or((
            StatusCode::BAD_REQUEST,
            "Invalid rental duration on tour.".into(),
        ))?;
    let property=client.query_opt("SELECT p.data->>'address',l.id,l.legal_name,l.service_address,l.email,l.phone,l.signer_name,l.signer_title,l.signature_authorized FROM properties p LEFT JOIN landlords l ON l.id=p.landlord_id WHERE p.id=$1",&[&property_id]).await.map_err(database_error)?.ok_or((StatusCode::BAD_REQUEST,"Choose an existing property.".into()))?;
    let landlord_id: Option<i64> = property.get(1);
    if landlord_id.is_none() {
        return Err((
            StatusCode::CONFLICT,
            "Assign the property to a landlord first.".into(),
        ));
    }
    let landlord = Landlord {
        id: landlord_id.unwrap(),
        legal_name: property.get(2),
        service_address: property.get(3),
        email: property.get(4),
        phone: property.get(5),
        signer_name: property.get(6),
        signer_title: property.get(7),
        signature_authorized: property.get(8),
        property_ids: vec![property_id],
        photo_url: String::new(),
    };
    let room_name = if input.lease_type == "entire_house" {
        "Entire house".to_string()
    } else {
        client.query_opt("SELECT data->>'name' FROM property_spaces WHERE property_id=$1 AND id=$2 AND data->>'kind' IN ('Bedroom','Suite')",&[&property_id,&room_id]).await.map_err(database_error)?
            .ok_or((StatusCode::BAD_REQUEST,"Choose a room or suite in this property.".into()))?.get::<_,Option<String>>(0).unwrap_or_default()
    };
    let details = LeaseDetails {
        start_date: start,
        end_date: end_date.to_string(),
        term_type: "fixed".into(),
        monthly_rent: input.monthly_rent.to_string(),
        rent_due_day: "1".into(),
        rent_payment_method: "e-transfer".into(),
        parking: "None".into(),
        condo: "No".into(),
        rent_deposit: input.monthly_rent.to_string(),
        key_deposit: "0".into(),
        unit_description: room_name.clone(),
        electricity_payer: "landlord".into(),
        heat_payer: "landlord".into(),
        water_payer: "landlord".into(),
        laundry_included: true,
        ..Default::default()
    };
    let template = client
        .query_opt(
            "SELECT landlord_id,details::text,additional_pdf FROM lease_templates WHERE id=$1",
            &[&input.template_id],
        )
        .await
        .map_err(database_error)?
        .ok_or((StatusCode::BAD_REQUEST, "Choose a lease template.".into()))?;
    if template.get::<_, Option<i64>>(0) != landlord_id {
        return Err((
            StatusCode::CONFLICT,
            "Choose a template for this property's landlord.".into(),
        ));
    }
    let terms: LeaseDetails =
        serde_json::from_str(&template.get::<_, String>(1)).map_err(database_error)?;
    let mut details = details;
    apply_reusable_terms(&mut details, &terms);
    ensure_no_signed_lease_overlap(&client, property_id, room_id, &details, None).await?;
    let details = serde_json::to_string(&details).map_err(database_error)?;
    let address: Option<String> = property.get(0);
    if address.as_deref().unwrap_or("").trim().is_empty() {
        return Err((
            StatusCode::CONFLICT,
            "Add the property's address before preparing a lease.".into(),
        ));
    }
    let snapshot = serde_json::to_string(
        &serde_json::json!({"landlord":landlord,"address":address,"room":room_name}),
    )
    .map_err(database_error)?;
    let tenant_name = match (
        tour.get::<_, Option<String>>(7),
        tour.get::<_, Option<String>>(8),
    ) {
        (Some(first), Some(last)) if !first.trim().is_empty() && !last.trim().is_empty() => {
            format!("{} {}", first.trim(), last.trim())
        }
        _ => tour.get(2),
    };
    let additional_pdf: Option<Vec<u8>> = template.get(2);
    let row=client.query_one(&format!("INSERT INTO leases (tenant_name,tenant_email,property_id,room_id,lease_type,details,landlord_snapshot,status,tour_id,template_id,additional_pdf) VALUES ($1,lower($2),$3,$4,$5,$6::text::jsonb,$7::text::jsonb,'draft',$8,$9,$10) RETURNING {LEASE_COLUMNS}"),&[&tenant_name,&email,&property_id,&room_id,&input.lease_type,&details,&snapshot,&tour_id,&input.template_id,&additional_pdf]).await.map_err(database_error)?;
    let lease = lease_row(row)?;
    drop(client);
    if let Err(error) = send_lease(Path(lease.id), State(db.clone())).await {
        db.lock()
            .await
            .execute(
                "DELETE FROM leases WHERE id=$1 AND status='draft'",
                &[&lease.id],
            )
            .await
            .map_err(database_error)?;
        return Err(error);
    }
    let row = db
        .lock()
        .await
        .query_one(
            &format!("SELECT {LEASE_COLUMNS} FROM leases WHERE id=$1"),
            &[&lease.id],
        )
        .await
        .map_err(database_error)?;
    Ok(Json(lease_row(row)?))
}
#[derive(Serialize)]
pub struct SentLink {
    pub(crate) url: String,
}
async fn restore_send_state(
    db: &Database,
    id: i64,
    status: &str,
    token_hash: &Option<String>,
    expires_at: &Option<String>,
    signed_at: &Option<String>,
    document_hash: &Option<String>,
) {
    if let Err(error)=db.lock().await.execute("UPDATE leases SET status=$2,token_hash=$3,token_expires_at=$4::text::timestamptz,landlord_signed_at=$5::text::timestamptz,document_sha256=$6 WHERE id=$1",&[&id,&status,&token_hash,&expires_at,&signed_at,&document_hash]).await {eprintln!("restore lease send state failed: {error}")}
}
pub async fn send_lease(
    Path(id): Path<i64>,
    State(db): State<Database>,
) -> ApiResult<Json<SentLink>> {
    let url = base_url()?;
    let secret = token();
    let hash = digest(&secret);
    let client = db.lock().await;
    let row=client.query_opt("SELECT tenant_name,tenant_email,status,details::text,landlord_snapshot::text,token_hash,token_expires_at::text,landlord_signed_at::text,document_sha256,is_demo,property_id,room_id FROM leases WHERE id=$1", &[&id]).await.map_err(database_error)?.ok_or((StatusCode::NOT_FOUND,"Lease not found.".into()))?;
    let status: String = row.get(2);
    let old_token: Option<String> = row.get(5);
    let old_expiry: Option<String> = row.get(6);
    let old_signature: Option<String> = row.get(7);
    let old_document_hash: Option<String> = row.get(8);
    if status != "draft" && status != "sent" {
        return Err((
            StatusCode::CONFLICT,
            "Only draft or sent leases can be sent.".into(),
        ));
    }
    let snapshot: serde_json::Value =
        serde_json::from_str(&row.get::<_, String>(4)).map_err(database_error)?;
    let landlord = &snapshot["landlord"];
    let current_landlord:Option<i64>=client.query_one("SELECT p.landlord_id FROM leases lease JOIN properties p ON p.id=lease.property_id WHERE lease.id=$1",&[&id]).await.map_err(database_error)?.get(0);
    if current_landlord.is_none() || landlord["id"].as_i64() != current_landlord {
        return Err((
            StatusCode::CONFLICT,
            "Assign this property to a landlord before sending the lease.".into(),
        ));
    }
    if landlord["signature_authorized"] != true
        || ["service_address", "email", "signer_name", "signer_title"]
            .iter()
            .any(|field| landlord[*field].as_str().unwrap_or("").trim().is_empty())
    {
        return Err((StatusCode::CONFLICT,"Complete and authorize this lease's landlord details before sending. If the landlord was updated after this draft was created, prepare a new draft.".into()));
    }
    let details: LeaseDetails =
        serde_json::from_str(&row.get::<_, String>(3)).map_err(database_error)?;
    if !row.get::<_, bool>(9) {
        ensure_no_signed_lease_overlap(&client, row.get(10), row.get(11), &details, Some(id))
            .await?;
    }
    if !["fixed", "month-to-month", "other"].contains(&details.term_type.as_str())
        || (details.term_type == "fixed" && details.end_date.is_empty())
        || (details.term_type == "other" && details.other_term.trim().is_empty())
        || details.rent_payment_method.trim().is_empty()
        || details.parking.trim().is_empty()
        || !["Yes", "No"].contains(&details.condo.as_str())
        || !["landlord", "tenant"].contains(&details.electricity_payer.as_str())
        || !["landlord", "tenant"].contains(&details.heat_payer.as_str())
        || !["landlord", "tenant"].contains(&details.water_payer.as_str())
        || (details.key_deposit.trim() != ""
            && details.key_deposit.trim() != "0"
            && details.key_description.trim().is_empty())
    {
        return Err((StatusCode::BAD_REQUEST,"Complete the term, payment method, condominium choice, utility payers, and any deposit details before sending.".into()));
    }
    let start =
        chrono::NaiveDate::parse_from_str(&details.start_date, "%Y-%m-%d").map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                "Enter a valid tenancy start date.".into(),
            )
        })?;
    if details.term_type == "fixed" {
        let end =
            chrono::NaiveDate::parse_from_str(&details.end_date, "%Y-%m-%d").map_err(|_| {
                (
                    StatusCode::BAD_REQUEST,
                    "Enter a valid fixed-term end date.".into(),
                )
            })?;
        if end <= start {
            return Err((
                StatusCode::BAD_REQUEST,
                "End date must follow start date.".into(),
            ));
        }
    }
    let rent = details.monthly_rent.parse::<u32>().map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            "Enter a valid monthly rent.".into(),
        )
    })?;
    for (label, amount) in [
        ("Rent deposit", &details.rent_deposit),
        ("Key deposit", &details.key_deposit),
    ] {
        if !amount.trim().is_empty() && amount.parse::<u32>().is_err() {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("{label} must be a non-negative whole-dollar amount."),
            ));
        }
    }
    if details.rent_deposit.parse::<u32>().unwrap_or(0) > rent {
        return Err((
            StatusCode::BAD_REQUEST,
            "Last month's rent deposit cannot exceed one month's rent.".into(),
        ));
    }
    if !details.nsf_fee.trim().is_empty()
        && details
            .nsf_fee
            .parse::<u8>()
            .ok()
            .filter(|v| *v <= 20)
            .is_none()
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "NSF administration charge cannot exceed $20.".into(),
        ));
    }
    let name: String = row.get(0);
    let to: String = row.get(1);
    let is_demo: bool = row.get(9);
    if !is_demo {
        let _ = crate::photo_id::encryption_key()?;
    }
    if to.to_ascii_lowercase().ends_with("@example.invalid") {
        return Err((
            StatusCode::BAD_REQUEST,
            "Confirm the guest email before sending this lease.".into(),
        ));
    }
    let link = format!("{url}/lease/{secret}");
    client.execute("UPDATE leases SET token_hash=$2,token_expires_at=NOW()+INTERVAL '14 days',landlord_signed_at=NULL,landlord_signature=NULL,status='sent' WHERE id=$1", &[&id,&hash]).await.map_err(database_error)?;
    drop(client);
    let bytes = match crate::lease_pdf::bytes_for_id(&db, id).await {
        Ok(bytes) => bytes,
        Err(error) => {
            restore_send_state(
                &db,
                id,
                &status,
                &old_token,
                &old_expiry,
                &old_signature,
                &old_document_hash,
            )
            .await;
            return Err(error);
        }
    };
    let pdf_hash = format!("{:x}", Sha256::digest(&bytes));
    if let Err(error) = db
        .lock()
        .await
        .execute(
            "UPDATE leases SET document_sha256=$2 WHERE id=$1",
            &[&id, &pdf_hash],
        )
        .await
    {
        restore_send_state(
            &db,
            id,
            &status,
            &old_token,
            &old_expiry,
            &old_signature,
            &old_document_hash,
        )
        .await;
        return Err(database_error(error));
    }
    let subject = if is_demo {
        "Buildry demo lease - signing walkthrough"
    } else {
        "Review and sign your Buildry lease"
    };
    let introduction = if is_demo {
        "<strong>DEMONSTRATION ONLY - This is test data, not a rental agreement.</strong> You can review the PDF and try the electronic signing step."
    } else {
        "Your lease is ready to review and sign online. Open your private signing link to read the completed Ontario Standard Form of Lease and select Sign lease when you are ready."
    };
    let result=email_payload(&to,subject,format!("<p>Hello {},</p><p>{}</p><p><a href=\"{}\">Review and sign lease</a></p><p>This link expires in 14 days. Contact Buildry through your usual contact channel if any details need correction.</p>",html_escape(&name),introduction,html_escape(&link)),None).await;
    if let Err(error) = result {
        restore_send_state(
            &db,
            id,
            &status,
            &old_token,
            &old_expiry,
            &old_signature,
            &old_document_hash,
        )
        .await;
        return Err(error);
    }
    Ok(Json(SentLink { url: link }))
}

fn pdf_viewer(document: &str, signature_kind: &str, signed: bool) -> String {
    format!(
        r#"<div class="lease-pdf-viewer lease-pdf-scroll-viewer" data-document="{document}" data-signature-kind="{signature_kind}" data-signed="{signed}"></div>"#
    )
}
fn readable_date(value: &str) -> String {
    chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map(|date| date.format("%B %-d, %Y").to_string())
        .unwrap_or_else(|_| value.to_string())
}
fn shown(value: &str) -> String {
    html_escape(if value.trim().is_empty() {
        "Not specified"
    } else {
        value
    })
}
fn amount(value: &str) -> String {
    if value.trim().is_empty() || value.trim() == "0" {
        "None".into()
    } else {
        format!("C${}", html_escape(value))
    }
}
fn detail_row(label: &str, value: &str) -> String {
    format!(
        "<div class='lease-story-row'><span>{}</span><strong>{}</strong></div>",
        html_escape(label),
        shown(value)
    )
}
fn optional_row(label: &str, value: &str) -> String {
    if value.trim().is_empty() {
        String::new()
    } else {
        detail_row(label, value)
    }
}
fn amenity(label: &str, included: bool) -> String {
    let (symbol, status, class) = if included {
        ("✓", "Included", "included")
    } else {
        ("—", "Not included", "excluded")
    };
    format!("<li class='lease-amenity {class}'><span class='lease-amenity-mark' aria-hidden='true'>{symbol}</span><span>{}</span><small>{status}</small></li>",html_escape(label))
}
fn lease_details_view(
    d: &LeaseDetails,
    s: &serde_json::Value,
    guest: Option<(&str, &str)>,
) -> String {
    if guest.is_none() && d.start_date.is_empty() {
        let landlord = &s["landlord"];
        let mut payments = detail_row("Payment method", &d.rent_payment_method);
        if !d.nsf_fee.trim().is_empty() {
            payments.push_str(&detail_row("NSF charge", &amount(&d.nsf_fee)));
        }
        if !d.key_deposit.trim().is_empty() {
            payments.push_str(&detail_row("Key deposit", &amount(&d.key_deposit)));
        }
        payments.push_str(&optional_row("Keys and devices", &d.key_description));
        let mut inclusions = String::new();
        for (label, included) in [
            ("Gas", d.gas_included),
            ("Air conditioning", d.ac_included),
            ("Storage", d.storage_included),
            ("Laundry", d.laundry_included),
            ("Guest parking", d.guest_parking_included),
        ] {
            inclusions.push_str(&amenity(label, included));
        }
        let mut utility_rows = String::new();
        for (label, payer) in [
            ("Electricity", &d.electricity_payer),
            ("Heat", &d.heat_payer),
            ("Water", &d.water_payer),
        ] {
            utility_rows.push_str(&detail_row(label, &format!("Paid by {payer}")));
        }
        utility_rows.push_str(&optional_row("Laundry charge", &d.laundry_fee));
        let mut rules = detail_row("Parking", &d.parking);
        rules.push_str(&detail_row("Condominium", &d.condo));
        rules.push_str(&optional_row("Smoking", &d.smoking));
        rules.push_str(&optional_row("Tenant insurance", &d.insurance));
        rules.push_str(&detail_row(
            "Email notices",
            if d.email_notices {
                "Agreed"
            } else {
                "Not agreed"
            },
        ));
        let content=format!("<section class='lease-story-section'><div class='lease-story-section-head'><span>01</span><div><h3>Payments</h3></div></div><div class='lease-story-list'>{payments}</div></section><section class='lease-story-section'><div class='lease-story-section-head'><span>02</span><div><h3>Services & utilities</h3></div></div><ul class='lease-amenity-list'>{inclusions}</ul><div class='lease-story-list'>{utility_rows}</div>{}{}</section><section class='lease-story-section'><div class='lease-story-section-head'><span>03</span><div><h3>House terms</h3></div></div><div class='lease-story-list'>{rules}</div>{}{}</section><section class='lease-story-section'><div class='lease-story-section-head'><span>04</span><div><h3>Landlord</h3></div></div><div class='lease-story-list'>{}{}</div></section>",if d.services.trim().is_empty(){String::new()}else{format!("<div class='lease-story-note'><strong>Services</strong><p>{}</p></div>",html_escape(&d.services))},if d.utilities.trim().is_empty(){String::new()}else{format!("<div class='lease-story-note'><strong>Utility details</strong><p>{}</p></div>",html_escape(&d.utilities))},if d.additional_terms.trim().is_empty(){String::new()}else{format!("<div class='lease-story-note'><strong>Additional terms</strong><p>{}</p></div>",html_escape(&d.additional_terms))},if d.notes.trim().is_empty(){String::new()}else{format!("<div class='lease-story-note'><strong>Notes</strong><p>{}</p></div>",html_escape(&d.notes))},detail_row("Legal name",landlord["legal_name"].as_str().unwrap_or("")),detail_row("Notice address",landlord["service_address"].as_str().unwrap_or("")));
        return collapsible_panel::render(
            "Lease details",
            "Payments, services and terms",
            "lease-details",
            false,
            &content,
        );
    }
    let landlord = &s["landlord"];
    let address = shown(s["address"].as_str().unwrap_or(""));
    let room = shown(s["room"].as_str().unwrap_or(""));
    let start = html_escape(&readable_date(&d.start_date));
    let end = if d.term_type == "month-to-month" {
        "Continues monthly".into()
    } else {
        html_escape(&readable_date(&d.end_date))
    };
    let term = match d.term_type.as_str() {
        "fixed" => "Fixed term",
        "month-to-month" => "Month to month",
        _ => "Other term",
    };
    let rent = amount(&d.monthly_rent);
    let unit_description = if d.unit_description.trim().is_empty()
        || d.unit_description
            .trim()
            .eq_ignore_ascii_case(s["room"].as_str().unwrap_or("").trim())
    {
        String::new()
    } else {
        format!(
            "<p class='lease-story-description'>{}</p>",
            html_escape(&d.unit_description)
        )
    };
    let home = format!(
        r#"<div class="lease-story-hero"><div><p class="lease-story-kicker">The home</p><h3>{room}</h3><p class="lease-story-address">{address}</p>{unit_description}</div><div class="lease-story-price"><span>Monthly rent</span><strong>{rent}</strong><small>per month</small></div></div>"#
    );
    let timeline = format!(
        r#"<div class="lease-story-timeline"><div class="tour-detail-schedule"><span class="tour-detail-label">Move in</span><strong class="tour-detail-time">{start}</strong></div><div class="tour-detail-schedule"><span class="tour-detail-label">{}</span><strong class="tour-detail-time">{end}</strong></div><span class="lease-story-term">{term}</span></div>"#,
        if d.term_type == "month-to-month" {
            "Term"
        } else {
            "Term ends"
        }
    );
    let due = match d.rent_due_day.parse::<u8>() {
        Ok(day) if (1..=31).contains(&day) => {
            let suffix = if (11..=13).contains(&day) {
                "th"
            } else {
                match day % 10 {
                    1 => "st",
                    2 => "nd",
                    3 => "rd",
                    _ => "th",
                }
            };
            format!("{day}{suffix} of each month")
        }
        _ => "Not specified".into(),
    };
    let mut payments = detail_row("Rent due", &due);
    payments.push_str(&detail_row("Payment method", &d.rent_payment_method));
    payments.push_str(&detail_row("Rent deposit", &amount(&d.rent_deposit)));
    payments.push_str(&detail_row("Key deposit", &amount(&d.key_deposit)));
    payments.push_str(&optional_row("Keys and devices", &d.key_description));
    payments.push_str(&optional_row("Partial rent", &d.partial_rent));
    payments.push_str(&optional_row("Rent discount", &d.rent_discount));
    if !d.nsf_fee.trim().is_empty() {
        payments.push_str(&detail_row("NSF charge", &amount(&d.nsf_fee)));
    }
    payments.push_str(&optional_row("Other term", &d.other_term));
    let payment_section=format!("<section class='lease-story-section'><div class='lease-story-section-head'><span>01</span><div><h3>Rent & payments</h3></div></div><div class='lease-story-list'>{payments}</div></section>");
    let mut features = String::new();
    for (label, included) in [
        ("Gas", d.gas_included),
        ("Air conditioning", d.ac_included),
        ("Storage", d.storage_included),
        ("Laundry", d.laundry_included),
        ("Guest parking", d.guest_parking_included),
    ] {
        features.push_str(&amenity(label, included));
    }
    let mut utility_rows = String::new();
    for (label, payer) in [
        ("Electricity", &d.electricity_payer),
        ("Heat", &d.heat_payer),
        ("Water", &d.water_payer),
    ] {
        utility_rows.push_str(&detail_row(
            label,
            &format!(
                "Paid by {}",
                if payer == "landlord" {
                    "landlord"
                } else if payer == "tenant" {
                    "tenant"
                } else {
                    "not specified"
                }
            ),
        ));
    }
    utility_rows.push_str(&optional_row("Laundry fee", &d.laundry_fee));
    let service_note = if d.services.trim().is_empty() {
        String::new()
    } else {
        format!(
            "<div class='lease-story-note'><strong>Services</strong><p>{}</p></div>",
            html_escape(&d.services)
        )
    };
    let utility_note = if d.utilities.trim().is_empty() {
        String::new()
    } else {
        format!(
            "<div class='lease-story-note'><strong>Utility details</strong><p>{}</p></div>",
            html_escape(&d.utilities)
        )
    };
    let inclusion_section=format!("<section class='lease-story-section'><div class='lease-story-section-head'><span>02</span><div><h3>What the home includes</h3></div></div>{service_note}<ul class='lease-amenity-list'>{features}</ul><h4 class='lease-story-subhead'>Utilities</h4><div class='lease-story-list'>{utility_rows}</div>{utility_note}</section>");
    let mut rules = detail_row("Parking", &d.parking);
    rules.push_str(&detail_row("Condominium", &d.condo));
    rules.push_str(&detail_row("Smoking rules", &d.smoking));
    rules.push_str(&detail_row("Tenant insurance", &d.insurance));
    rules.push_str(&detail_row(
        "Email notices",
        if d.email_notices {
            "Agreed"
        } else {
            "Not agreed"
        },
    ));
    rules.push_str(&optional_row("Other occupants", &d.occupants));
    let additional = if d.additional_terms.trim().is_empty() {
        String::new()
    } else {
        format!(
            "<div class='lease-story-note'><strong>Additional terms</strong><p>{}</p></div>",
            html_escape(&d.additional_terms)
        )
    };
    let notes = if d.notes.trim().is_empty() {
        String::new()
    } else {
        format!(
            "<div class='lease-story-note'><strong>Notes</strong><p>{}</p></div>",
            html_escape(&d.notes)
        )
    };
    let rules_section=format!("<section class='lease-story-section'><div class='lease-story-section-head'><span>03</span><div><h3>House rules & terms</h3></div></div><div class='lease-story-list'>{rules}</div>{additional}{notes}</section>");
    let landlord_name = shown(landlord["legal_name"].as_str().unwrap_or(""));
    let guest_card=guest.map(|(name,email)|format!("<div class='lease-person'><span class='lease-person-avatar' aria-hidden='true'>T</span><div><small>Tenant</small><strong>{}</strong><p>{}</p></div></div>",shown(name),shown(email))).unwrap_or_default();
    let people_section=format!("<section class='lease-story-section'><div class='lease-story-section-head'><span>04</span><div><h3>People & contact</h3></div></div><div class='lease-people'><div class='lease-person'><span class='lease-person-avatar' aria-hidden='true'>L</span><div><small>Landlord</small><strong>{landlord_name}</strong><p>{}</p></div></div>{guest_card}</div></section>",shown(landlord["service_address"].as_str().unwrap_or("")));
    collapsible_panel::render(
        "Lease details",
        "Home, dates, payments and terms",
        "lease-details",
        false,
        &format!(
            "{home}{timeline}{payment_section}{inclusion_section}{rules_section}{people_section}"
        ),
    )
}
pub async fn lease_page(Path(secret): Path<String>, State(db): State<Database>) -> Response {
    let hash = digest(&secret);
    let row=match db.lock().await.query_opt("SELECT tenant_name,tenant_email,details::text,landlord_snapshot::text,status,signed_at,is_demo FROM leases WHERE token_hash=$1 AND token_expires_at>NOW()", &[&hash]).await { Ok(Some(r))=>r,_=>return (StatusCode::NOT_FOUND,"This lease link is invalid or expired. Ask Buildry for a new one.").into_response() };
    let name: String = row.get(0);
    let email: String = row.get(1);
    let details: String = row.get(2);
    let snapshot: String = row.get(3);
    let status: String = row.get(4);
    let d: LeaseDetails = match serde_json::from_str(&details) {
        Ok(v) => v,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let s: serde_json::Value = match serde_json::from_str(&snapshot) {
        Ok(v) => v,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let details_view = lease_details_view(&d, &s, Some((&name, &email)));
    let signed = status == "signed" || status == "confirmed";
    let is_demo: bool = row.get(6);
    let action = if signed {
        format!("<div class='sign-complete' role='status'><span aria-hidden='true'>✓</span><div><strong>Your signature is complete</strong><p>Your signature appears on page 7. The landlord signs this lease next.</p></div></div>{}",if is_demo {String::new()}else{format!("<form class='sign-form identity-document-form' method='get' action='/lease/{}/photo-id'><button type='submit'><svg aria-hidden='true' width='20' height='20' viewBox='0 0 24 24' fill='none' stroke='currentColor' stroke-width='1.8' stroke-linecap='round' stroke-linejoin='round'><rect x='3' y='5' width='18' height='14' rx='2'/><circle cx='8.5' cy='10' r='2'/><path d='M5.5 16c.5-1.6 1.5-2.4 3-2.4s2.5.8 3 2.4M14 9h4M14 13h4'/></svg><span>Continue to identity document</span></button></form>",html_escape(&secret))})
    } else {
        format!("<form class='sign-form' method='post' action='/lease/{}' data-signature-form data-kind='tenant'><label for='legal-signature'>Your legal name</label><input id='legal-signature' name='signature' required maxlength='120' autocomplete='name' placeholder='{}'><label class='sign-consent'><input type='checkbox' name='consent' value='yes' required><span>I reviewed this lease and agree to sign it electronically.</span></label><button type='button' data-preview-signature>Preview signature</button><div class='sign-preview' data-sign-preview hidden><span>Signature preview</span><strong></strong></div><button type='submit' data-submit-signature hidden>Submit signature</button></form>",html_escape(&secret),html_escape(&name))
    };
    let document = html_escape(&format!("/lease/{secret}/document?v=signature-2"));
    let pdf_viewer = pdf_viewer(&document, "tenant", signed);
    let demo_notice = if is_demo {
        "<div class='signing-demo-notice' role='status'><strong>DEMONSTRATION ONLY</strong> This is test data and does not create a tenancy or rental agreement.</div>"
    } else {
        ""
    };
    let html = format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><meta name="referrer" content="no-referrer"><title>Review and sign lease | Buildry</title><link rel="stylesheet" href="/assets/main-20261017d.css?v=20261022c"><script type="module" src="/assets/lease-viewer-20261017b.mjs?v=20261022b"></script><script type="module" src="/assets/signing-flow-20261012c.mjs"></script></head><body class="signing-body tenant-signing-body"><main class="signing-shell"><header class="signing-header"><div class="signing-brand">Buildry <span>Leasing</span></div><span class="signing-private">Private signing link</span></header>{demo_notice}<section class="signing-document">{details_view}{pdf_viewer}</section><section class="signing-action">{action_heading_html}{action}</section><footer class="signing-footer">Private link for {email}.</footer></main></body></html>"#,
        action_heading_html = if signed {
            "<h2>Signing complete</h2>"
        } else {
            ""
        },
        email = html_escape(&email)
    );
    let mut response = Html(html).into_response();
    response
        .headers_mut()
        .insert("referrer-policy", "no-referrer".parse().unwrap());
    response
        .headers_mut()
        .insert("cache-control", "private, no-store".parse().unwrap());
    response
        .headers_mut()
        .insert("x-frame-options", "DENY".parse().unwrap());
    response.headers_mut().insert(
        "content-security-policy",
        "frame-ancestors 'none'".parse().unwrap(),
    );
    response
}

async fn issue_landlord_link(db: &Database, id: i64) -> ApiResult<()> {
    let origin = base_url()?;
    let secret = token();
    let hash = digest(&secret);
    let row = db.lock().await.query_opt(
        "SELECT landlord_snapshot::text,tenant_name FROM leases WHERE id=$1 AND status='signed' AND signed_at IS NOT NULL AND landlord_signed_at IS NULL AND NOT is_demo",
        &[&id],
    ).await.map_err(database_error)?.ok_or((StatusCode::CONFLICT, "The tenant must sign before the landlord can sign.".into()))?;
    let snapshot: serde_json::Value =
        serde_json::from_str(&row.get::<_, String>(0)).map_err(database_error)?;
    let recipient = snapshot["landlord"]["email"].as_str().unwrap_or("").trim();
    let signer = snapshot["landlord"]["signer_name"]
        .as_str()
        .unwrap_or("")
        .trim();
    if !recipient.contains('@')
        || signer.is_empty()
        || snapshot["landlord"]["signature_authorized"] != true
    {
        return Err((
            StatusCode::CONFLICT,
            "Complete the landlord's signing details before sending a link.".into(),
        ));
    }
    db.lock().await.execute(
        "UPDATE leases SET landlord_token_hash=$2,landlord_token_expires_at=NOW()+INTERVAL '14 days' WHERE id=$1 AND status='signed' AND landlord_signed_at IS NULL",
        &[&id,&hash],
    ).await.map_err(database_error)?;
    let link = format!("{origin}/lease-landlord/{secret}");
    email(recipient, "Sign the tenant-signed Buildry lease", format!(
        "<p>Hello {},</p><p>The tenant has signed the lease for {}. Please review the completed lease and add your signature:</p><p><a href='{}'>Review and sign lease</a></p><p>This private link expires in 14 days.</p>",
        html_escape(signer), html_escape(&row.get::<_, String>(1)), html_escape(&link)
    )).await?;
    Ok(())
}

pub async fn resend_landlord_signing_link(
    Path(id): Path<i64>,
    State(db): State<Database>,
) -> ApiResult<StatusCode> {
    issue_landlord_link(&db, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn landlord_lease_page(
    Path(secret): Path<String>,
    State(db): State<Database>,
) -> Response {
    let hash = digest(&secret);
    let row = match db.lock().await.query_opt(
        "SELECT tenant_name,tenant_email,details::text,landlord_snapshot::text,landlord_signed_at IS NOT NULL FROM leases WHERE landlord_token_hash=$1 AND landlord_token_expires_at>NOW() AND status='signed' AND signed_at IS NOT NULL",
        &[&hash],
    ).await { Ok(Some(row)) => row, _ => return (StatusCode::NOT_FOUND, "This landlord signing link is invalid or expired.").into_response() };
    let name: String = row.get(0);
    let details: LeaseDetails = match serde_json::from_str(&row.get::<_, String>(2)) {
        Ok(value) => value,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let snapshot: serde_json::Value = match serde_json::from_str(&row.get::<_, String>(3)) {
        Ok(value) => value,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let signer = snapshot["landlord"]["signer_name"].as_str().unwrap_or("");
    let signed: bool = row.get(4);
    let action = if signed {
        "<div class='sign-complete' role='status'><span aria-hidden='true'>✓</span><div><strong>Lease signed</strong><p>Both signatures now appear on page 7 of the PDF.</p></div></div>".to_string()
    } else {
        format!("<form class='sign-form' method='post' action='/lease-landlord/{}' data-signature-form data-kind='landlord'><label for='landlord-signature'>Your legal name</label><input id='landlord-signature' name='signature' required maxlength='120' autocomplete='name' placeholder='{}'><label class='sign-consent'><input type='checkbox' name='consent' value='yes' required><span>I reviewed the tenant-signed lease and agree to sign it electronically.</span></label><button type='button' data-preview-signature>Preview signature</button><div class='sign-preview' data-sign-preview hidden><span>Signature preview</span><strong></strong></div><button type='submit' data-submit-signature hidden>Sign lease</button></form>", html_escape(&secret), html_escape(signer))
    };
    let document = html_escape(&format!("/lease-landlord/{secret}/document?v=20261022"));
    let viewer = pdf_viewer(&document, "landlord", signed);
    let details_view =
        lease_details_view(&details, &snapshot, Some((&name, &row.get::<_, String>(1))));
    let html = format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><meta name="referrer" content="no-referrer"><title>Landlord lease signature | Buildry</title><link rel="stylesheet" href="/assets/main-20261017d.css?v=20261022c"><script type="module" src="/assets/lease-viewer-20261017b.mjs?v=20261022b"></script><script type="module" src="/assets/signing-flow-20261012c.mjs"></script></head><body class="signing-body tenant-signing-body"><main class="signing-shell"><header class="signing-header"><div class="signing-brand">Buildry <span>Leasing</span></div><span class="signing-private">Private landlord link</span></header><section class="signing-intro"><p class="signing-eyebrow">Tenant signed · landlord review</p><h1>Sign the lease</h1><p>Review the tenant-signed agreement before adding your signature.</p></section><section class="signing-document">{details_view}{viewer}</section><section class="signing-action"><h2>{}</h2>{action}</section><footer class="signing-footer">Private link for {}.</footer></main></body></html>"#,
        if signed {
            "Signing complete"
        } else {
            "Landlord signature"
        },
        html_escape(snapshot["landlord"]["email"].as_str().unwrap_or(""))
    );
    let mut response = Html(html).into_response();
    response
        .headers_mut()
        .insert("cache-control", "private, no-store".parse().unwrap());
    response
        .headers_mut()
        .insert("referrer-policy", "no-referrer".parse().unwrap());
    response
        .headers_mut()
        .insert("x-frame-options", "DENY".parse().unwrap());
    response.headers_mut().insert(
        "content-security-policy",
        "frame-ancestors 'none'".parse().unwrap(),
    );
    response
}

pub async fn sign_landlord_lease(
    Path(secret): Path<String>,
    State(db): State<Database>,
    Form(input): Form<SignatureInput>,
) -> Response {
    if input.signature.trim().len() < 2 || input.signature.len() > 120 || input.consent != "yes" {
        return (
            StatusCode::BAD_REQUEST,
            "Enter your full legal name and accept the signature statement.",
        )
            .into_response();
    }
    let hash = digest(&secret);
    let row = match db.lock().await.query_opt(
        "SELECT id,landlord_snapshot::text FROM leases WHERE landlord_token_hash=$1 AND landlord_token_expires_at>NOW() AND status='signed' AND signed_at IS NOT NULL AND landlord_signed_at IS NULL",
        &[&hash],
    ).await { Ok(Some(row)) => row, _ => return (StatusCode::CONFLICT, "This lease is already signed or the link has expired.").into_response() };
    let snapshot: serde_json::Value = match serde_json::from_str(&row.get::<_, String>(1)) {
        Ok(value) => value,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    if !snapshot["landlord"]["signer_name"]
        .as_str()
        .unwrap_or("")
        .trim()
        .eq_ignore_ascii_case(input.signature.trim())
    {
        return (
            StatusCode::CONFLICT,
            "Use the landlord signer's full legal name shown in the lease.",
        )
            .into_response();
    }
    let id: i64 = row.get(0);
    let changed = db.lock().await.execute(
        "UPDATE leases SET landlord_signature=$2,landlord_signed_at=NOW(),landlord_snapshot=jsonb_set(landlord_snapshot,'{landlord_signature}',to_jsonb($2::text)) WHERE id=$1 AND landlord_token_hash=$3 AND landlord_token_expires_at>NOW() AND status='signed' AND signed_at IS NOT NULL AND landlord_signed_at IS NULL",
        &[&id,&input.signature.trim(),&hash],
    ).await;
    match changed {
        Ok(1) => {
            if let Ok(bytes) = crate::lease_pdf::bytes_for_id(&db, id).await {
                let digest = format!("{:x}", Sha256::digest(&bytes));
                let _ = db
                    .lock()
                    .await
                    .execute(
                        "UPDATE leases SET signed_pdf_sha256=$2 WHERE id=$1",
                        &[&id, &digest],
                    )
                    .await;
            }
            Redirect::to(&format!("/lease-landlord/{secret}")).into_response()
        }
        Ok(_) => (StatusCode::CONFLICT, "This lease was already signed.").into_response(),
        Err(error) => {
            eprintln!("landlord lease signature failed: {error}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
#[derive(Deserialize)]
pub struct SignatureInput {
    signature: String,
    consent: String,
}
fn signing_error(secret: &str, status: StatusCode, title: &str, detail: &str) -> Response {
    let html=format!("<!doctype html><html lang='en'><head><meta charset='utf-8'><meta name='viewport' content='width=device-width, initial-scale=1'><meta name='referrer' content='no-referrer'><title>{} | Buildry</title><link rel='stylesheet' href='/assets/main-20261012c.css'></head><body class='signing-body'><main class='signing-shell'><header class='signing-header'><div class='signing-brand'>Buildry <span>Leasing</span></div><span class='signing-private'>Private signing link</span></header><section class='signing-action signing-error'><p class='signing-eyebrow'>Signature not recorded</p><h1>{}</h1><p>{}</p><a href='/lease/{}'>Return to the lease</a></section></main></body></html>",html_escape(title),html_escape(title),html_escape(detail),html_escape(secret));
    let mut response = (status, Html(html)).into_response();
    response
        .headers_mut()
        .insert("cache-control", "private, no-store".parse().unwrap());
    response
        .headers_mut()
        .insert("referrer-policy", "no-referrer".parse().unwrap());
    response
        .headers_mut()
        .insert("x-frame-options", "DENY".parse().unwrap());
    response.headers_mut().insert(
        "content-security-policy",
        "frame-ancestors 'none'".parse().unwrap(),
    );
    response
}
pub async fn sign_lease(
    Path(secret): Path<String>,
    State(db): State<Database>,
    headers: HeaderMap,
    Form(input): Form<SignatureInput>,
) -> Response {
    if input.signature.trim().len() < 2 || input.signature.len() > 120 || input.consent != "yes" {
        return signing_error(&secret,StatusCode::BAD_REQUEST,"Check your signature","Enter your full legal name and accept the electronic-signature statement before signing.");
    }
    let hash = digest(&secret);
    let ip = headers
        .get("cf-connecting-ip")
        .or_else(|| headers.get("x-forwarded-for"))
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .chars()
        .take(120)
        .collect::<String>();
    let agent = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .chars()
        .take(500)
        .collect::<String>();
    let mut client = db.lock().await;
    let tx = match client.transaction().await {
        Ok(tx) => tx,
        Err(error) => {
            eprintln!("sign lease transaction: {error}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let row=match tx.query_opt("SELECT id,property_id,room_id,tenant_name,details::text,is_demo FROM leases WHERE token_hash=$1 AND token_expires_at>NOW() AND status='sent' FOR UPDATE",&[&hash]).await {
        Ok(Some(row))=>row,
        Ok(None)=>return (StatusCode::CONFLICT,"The link may have been used or expired.").into_response(),
        Err(error)=>{eprintln!("sign lease lookup: {error}");return StatusCode::INTERNAL_SERVER_ERROR.into_response()}
    };
    let name: String = row.get(3);
    if !name.trim().eq_ignore_ascii_case(input.signature.trim()) {
        return signing_error(
            &secret,
            StatusCode::CONFLICT,
            "Name does not match",
            "Use the tenant's full legal name exactly as it appears on the lease.",
        );
    }
    let property_id: i64 = row.get(1);
    let room_id: i64 = row.get(2);
    if tx
        .query_one(
            "SELECT pg_advisory_xact_lock(hashtext($1),hashtext('leases'))",
            &[&property_id.to_string()],
        )
        .await
        .is_err()
    {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    let details: LeaseDetails = match serde_json::from_str(&row.get::<_, String>(4)) {
        Ok(details) => details,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let end = if details.end_date.is_empty() {
        "9999-12-31"
    } else {
        details.end_date.as_str()
    };
    let conflict=match tx.query_opt("SELECT id FROM leases WHERE id<>$1 AND property_id=$2 AND (room_id=$3 OR room_id=0 OR $3=0) AND status IN ('signed','confirmed') AND NOT is_demo AND details->>'start_date'<=$4 AND COALESCE(NULLIF(details->>'end_date',''),'9999-12-31')>=$5 LIMIT 1",&[&row.get::<_,i64>(0),&property_id,&room_id,&end,&details.start_date]).await {
        Ok(value)=>value.is_some(),Err(error)=>{eprintln!("sign lease occupancy check: {error}");return StatusCode::INTERNAL_SERVER_ERROR.into_response()}
    };
    if conflict && !row.get::<_, bool>(5) {
        return signing_error(&secret,StatusCode::CONFLICT,"Lease needs review","This rental space is already covered by another signed lease for the same dates. Contact Buildry to resolve it.");
    }
    match tx.execute("UPDATE leases SET tenant_signature=$2,signed_at=NOW(),signed_ip=$3,signed_user_agent=$4,status='signed',token_expires_at=GREATEST(token_expires_at,NOW()+INTERVAL '7 days') WHERE token_hash=$1", &[&hash,&input.signature.trim(),&ip,&agent]).await {
        Ok(1)=>{
            if let Err(error)=tx.commit().await{eprintln!("sign lease commit: {error}");return StatusCode::INTERNAL_SERVER_ERROR.into_response()}
            drop(client);
            if let Ok(bytes)=crate::lease_pdf::bytes_for_token(&db,&hash).await {
                let pdf_hash=format!("{:x}",Sha256::digest(&bytes));
                let _=db.lock().await.execute("UPDATE leases SET signed_pdf_sha256=$2 WHERE token_hash=$1", &[&hash,&pdf_hash]).await;
            }
            if !row.get::<_,bool>(5) {
                if let Err(error) = issue_landlord_link(&db, row.get(0)).await {
                    eprintln!("landlord signing link after tenant signature: {}", error.1);
                }
            }
            if row.get::<_,bool>(5) { Redirect::to(&format!("/lease/{secret}")).into_response() }
            else { Redirect::to(&format!("/lease/{secret}/photo-id")).into_response() }
        },
        Ok(_)=>(StatusCode::CONFLICT,"The lease could not be signed. Ask Buildry for a new link.").into_response(),
        Err(error)=>{eprintln!("sign lease failed: {error}");StatusCode::INTERNAL_SERVER_ERROR.into_response()}
    }
}
pub async fn confirm_lease(
    Path(id): Path<i64>,
    State(db): State<Database>,
    auth: crate::auth::AuthSession,
) -> ApiResult<StatusCode> {
    let admin = auth
        .user
        .ok_or((StatusCode::UNAUTHORIZED, "Sign in required.".into()))?;
    let signed_pdf = crate::lease_pdf::bytes_for_id(&db, id).await?;
    let signed_pdf_hash = format!("{:x}", Sha256::digest(&signed_pdf));
    let mut client = db.lock().await;
    let tx = client.transaction().await.map_err(database_error)?;
    let row=tx.query_opt("SELECT tenant_name,tenant_email,status,is_seeded,is_demo,landlord_signed_at IS NOT NULL FROM leases WHERE id=$1 FOR UPDATE", &[&id]).await.map_err(database_error)?.ok_or((StatusCode::NOT_FOUND,"Lease not found.".into()))?;
    if row.get::<_, bool>(4) {
        return Err((
            StatusCode::CONFLICT,
            "Demo leases cannot be confirmed as tenancies.".into(),
        ));
    }
    let status: String = row.get(2);
    if status != "signed" {
        return Err((
            StatusCode::CONFLICT,
            "Only a signed lease can be confirmed.".into(),
        ));
    }
    if !row.get::<_, bool>(5) {
        return Err((
            StatusCode::CONFLICT,
            "The landlord must sign the tenant-signed lease before confirmation.".into(),
        ));
    }
    if tx
        .query_opt(
            "SELECT 1 FROM lease_photo_ids WHERE lease_id=$1 AND verified_at IS NOT NULL",
            &[&id],
        )
        .await
        .map_err(database_error)?
        .is_none()
    {
        return Err((
            StatusCode::CONFLICT,
            "Review and approve the tenant's photo ID before confirming the tenancy.".into(),
        ));
    }
    let name: String = row.get(0);
    let (first_name, last_name) = crate::account_name_parts(&name);
    let email_addr: String = row.get(1);
    let seeded: bool = row.get(3);
    let existing=tx.query_opt("SELECT id,role,password_hash,active FROM users WHERE lower(email)=lower($1) FOR UPDATE", &[&email_addr]).await.map_err(database_error)?;
    let (user_id, needs_password) = if let Some(r) = existing {
        let role: String = r.get(1);
        if role != "guest" && role != "tenant" {
            return Err((StatusCode::CONFLICT,"A privileged account uses this email. Resolve the account in Access before confirming the lease.".into()));
        }
        let password: String = r.get(2);
        let active: bool = r.get(3);
        let id: i64 = r.get(0);
        let reset = password.is_empty() || (role == "guest" && !active);
        tx.execute("UPDATE users SET role='tenant',first_name=$2,last_name=$3,active=TRUE,password_hash=CASE WHEN $4 THEN '' ELSE password_hash END,auth_version=auth_version+1 WHERE id=$1", &[&id,&first_name,&last_name,&reset]).await.map_err(database_error)?;
        (id, reset)
    } else {
        let id:i64=tx.query_one("INSERT INTO users (first_name,last_name,email,role,password_hash) VALUES ($1,$2,$3,'tenant','') RETURNING id", &[&first_name,&last_name,&email_addr]).await.map_err(database_error)?.get(0);
        (id, true)
    };
    tx.execute("UPDATE leases SET status='confirmed',confirmed_at=NOW(),confirmed_by=$2,signed_pdf_sha256=$3,copy_sent_at=CASE WHEN $4 THEN NOW() ELSE copy_sent_at END WHERE id=$1", &[&id,&admin.id,&signed_pdf_hash,&seeded]).await.map_err(database_error)?;
    let invite = if needs_password && !seeded {
        let secret = token();
        let hash = digest(&secret);
        tx.execute("INSERT INTO password_invites (user_id,token_hash,expires_at) VALUES ($1,$2,NOW()+INTERVAL '7 days')", &[&user_id,&hash]).await.map_err(database_error)?;
        Some(secret)
    } else {
        None
    };
    tx.commit().await.map_err(database_error)?;
    drop(client);
    if seeded {
        return Ok(StatusCode::NO_CONTENT);
    }
    send_confirmation(&db, id, &name, &email_addr, invite.as_deref()).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn resend_photo_id_link(
    Path(id): Path<i64>,
    State(db): State<Database>,
) -> ApiResult<StatusCode> {
    let _ = crate::photo_id::encryption_key()?;
    let origin = base_url()?;
    let secret = token();
    let hash = digest(&secret);
    let client = db.lock().await;
    let row=client.query_opt("SELECT tenant_name,tenant_email,token_hash,token_expires_at::text FROM leases WHERE id=$1 AND status='signed' AND NOT is_demo AND NOT EXISTS(SELECT 1 FROM lease_photo_ids WHERE lease_id=$1)",&[&id]).await.map_err(database_error)?.ok_or((StatusCode::CONFLICT,"This lease is not waiting for a photo ID.".into()))?;
    let name: String = row.get(0);
    let to: String = row.get(1);
    let old_hash: Option<String> = row.get(2);
    let old_expiry: Option<String> = row.get(3);
    client
        .execute(
            "UPDATE leases SET token_hash=$2,token_expires_at=NOW()+INTERVAL '7 days' WHERE id=$1",
            &[&id, &hash],
        )
        .await
        .map_err(database_error)?;
    drop(client);
    let link = format!("{origin}/lease/{secret}/photo-id");
    if let Err(error)=email(&to,"Complete your Buildry identity document upload",format!("<p>Hello {},</p><p>Your signed lease is waiting for one photo ID. Use this private link to upload it:</p><p><a href='{}'>Upload your photo ID</a></p><p>The link expires in seven days. Keep it private.</p>",html_escape(&name),html_escape(&link))).await {
        if let Err(restore_error)=db.lock().await.execute("UPDATE leases SET token_hash=$2,token_expires_at=$3::text::timestamptz WHERE id=$1 AND token_hash=$4",&[&id,&old_hash,&old_expiry,&hash]).await {eprintln!("restore photo ID link failed: {restore_error}")}
        return Err(error)
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct TenancyEndInput {
    end_date: String,
}

pub async fn record_tenancy_end(
    Path(id): Path<i64>,
    State(db): State<Database>,
    auth: crate::auth::AuthSession,
    Form(input): Form<TenancyEndInput>,
) -> Response {
    let Some(user) = auth.user else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let date = if input.end_date.trim().is_empty() {
        None
    } else {
        let parsed = match chrono::NaiveDate::parse_from_str(input.end_date.trim(), "%Y-%m-%d") {
            Ok(date) => date,
            Err(_) => {
                return (StatusCode::BAD_REQUEST, "Enter a valid tenancy end date.").into_response()
            }
        };
        Some(parsed.to_string())
    };
    let result=db.lock().await.execute("UPDATE leases SET tenancy_ended_at=$2::text::date,tenancy_ended_by=$3,tenancy_end_recorded_at=NOW() WHERE id=$1 AND status='confirmed' AND NOT is_demo AND ($2::text IS NULL OR ($2::text>=details->>'start_date' AND $2::text::date<=(NOW() AT TIME ZONE 'America/Toronto')::date))", &[&id,&date,&user.id]).await;
    match result {
        Ok(1)=>{if let Err(error)=crate::photo_id::delete_expired(&db).await { eprintln!("Photo ID retention cleanup failed: {error}"); } Redirect::to(&format!("/leases?lease={id}")).into_response()},
        Ok(_)=>(StatusCode::CONFLICT,"Only a confirmed tenancy can be ended, and the end date must be between its start date and today.").into_response(),
        Err(error)=>database_error(error).into_response(),
    }
}
async fn send_confirmation(
    db: &Database,
    id: i64,
    name: &str,
    to: &str,
    invite: Option<&str>,
) -> ApiResult<()> {
    let setup = if let Some(secret) = invite {
        let link = format!("{}/set-password/{secret}", base_url()?);
        format!("<p><a href=\"{}\">Set your password</a> to access your tenant account. This link expires in 7 days.</p>",html_escape(&link))
    } else {
        String::new()
    };
    let pdf = crate::lease_pdf::bytes_for_id(db, id).await?;
    email_payload(to,"Your confirmed Buildry lease",format!("<p>Hello {},</p><p>Your signed Ontario Standard Form of Lease is attached for your records.</p>{setup}",html_escape(name)),Some(("signed-ontario-lease.pdf",pdf))).await?;
    db.lock()
        .await
        .execute("UPDATE leases SET copy_sent_at=NOW() WHERE id=$1", &[&id])
        .await
        .map_err(database_error)?;
    Ok(())
}
pub async fn resend_confirmation(
    Path(id): Path<i64>,
    State(db): State<Database>,
) -> ApiResult<StatusCode> {
    let mut client = db.lock().await;
    let tx = client.transaction().await.map_err(database_error)?;
    let row=tx.query_opt("SELECT l.tenant_name,l.tenant_email,l.status,u.id,u.password_hash FROM leases l JOIN users u ON lower(u.email)=lower(l.tenant_email) WHERE l.id=$1", &[&id]).await.map_err(database_error)?.ok_or((StatusCode::NOT_FOUND,"Lease or tenant not found.".into()))?;
    let status: String = row.get(2);
    if status != "confirmed" {
        return Err((
            StatusCode::CONFLICT,
            "Confirm the signed lease first.".into(),
        ));
    }
    let name: String = row.get(0);
    let to: String = row.get(1);
    let user_id: i64 = row.get(3);
    let pass: String = row.get(4);
    let invite = if pass.is_empty() {
        let secret = token();
        let hash = digest(&secret);
        tx.execute("INSERT INTO password_invites (user_id,token_hash,expires_at) VALUES ($1,$2,NOW()+INTERVAL '7 days')", &[&user_id,&hash]).await.map_err(database_error)?;
        Some(secret)
    } else {
        None
    };
    tx.commit().await.map_err(database_error)?;
    drop(client);
    send_confirmation(&db, id, &name, &to, invite.as_deref()).await?;
    Ok(StatusCode::NO_CONTENT)
}
pub async fn set_password_page(Path(secret): Path<String>, State(db): State<Database>) -> Response {
    let hash = digest(&secret);
    let valid=db.lock().await.query_opt("SELECT p.id FROM password_invites p JOIN users u ON u.id=p.user_id WHERE p.token_hash=$1 AND p.used_at IS NULL AND p.expires_at>NOW() AND u.active", &[&hash]).await.ok().flatten().is_some();
    if !valid {
        return (StatusCode::NOT_FOUND, Html("<!doctype html><html lang=\"en\"><head><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>Password link expired | Buildry</title><link rel=\"stylesheet\" href=\"/assets/main.css\"></head><body class=\"auth-body\"><main class=\"auth-card\"><h1>This password link is invalid or expired</h1><p>Request a new link from Buildry or sign in with Google.</p><a href=\"/login\">Back to sign in</a></main></body></html>".to_string())).into_response();
    }
    set_password_html(&secret, None).into_response()
}

fn set_password_html(secret: &str, error: Option<&str>) -> Html<String> {
    let error = error
        .map(|message| format!("<p role=\"alert\">{}</p>", html_escape(message)))
        .unwrap_or_default();
    Html(format!("<!doctype html><html lang=\"en\"><head><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>Set password | Buildry</title><link rel=\"stylesheet\" href=\"/assets/main.css\"></head><body class=\"auth-body\"><main class=\"auth-card\"><h1>Set your password</h1>{error}<form method=\"post\" action=\"/set-password/{}\"><label>Password (12+ characters)<input type=\"password\" name=\"password\" minlength=\"12\" required autocomplete=\"new-password\"></label><label>Confirm password<input type=\"password\" name=\"confirm_password\" minlength=\"12\" required autocomplete=\"new-password\"></label><button type=\"submit\">Set password</button></form></main></body></html>", html_escape(secret)))
}
#[derive(Deserialize)]
pub struct PasswordInput {
    password: String,
    confirm_password: String,
}
pub async fn set_password(
    Path(secret): Path<String>,
    State(db): State<Database>,
    Form(input): Form<PasswordInput>,
) -> Response {
    if input.password.chars().count() < 12 {
        return (
            StatusCode::BAD_REQUEST,
            set_password_html(&secret, Some("Password must have at least 12 characters.")),
        )
            .into_response();
    }
    if input.password != input.confirm_password {
        return (
            StatusCode::BAD_REQUEST,
            set_password_html(&secret, Some("Passwords do not match.")),
        )
            .into_response();
    }
    let hash = digest(&secret);
    let pass =
        match tokio::task::spawn_blocking(move || password_auth::generate_hash(input.password))
            .await
        {
            Ok(v) => v,
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        };
    let mut client = db.lock().await;
    let tx = match client.transaction().await {
        Ok(v) => v,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let row=match tx.query_opt("UPDATE password_invites p SET used_at=NOW() FROM users u WHERE p.token_hash=$1 AND p.used_at IS NULL AND p.expires_at>NOW() AND u.id=p.user_id AND u.active RETURNING p.user_id", &[&hash]).await {Ok(Some(v))=>v,_=>return Redirect::to("/login").into_response()};
    let id: i64 = row.get(0);
    if tx
        .execute(
            "UPDATE users SET password_hash=$2 WHERE id=$1",
            &[&id, &pass],
        )
        .await
        .is_err()
        || tx
            .execute(
                "UPDATE password_invites SET used_at=NOW() WHERE user_id=$1 AND used_at IS NULL",
                &[&id],
            )
            .await
            .is_err()
        || tx.commit().await.is_err()
    {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    Redirect::to("/login?password_set=1").into_response()
}

#[cfg(test)]
mod email_tests {
    use super::*;
    use axum::{routing::post, Router};

    #[test]
    fn lease_type_uses_a_room_only_for_room_leases() {
        assert!(valid_lease_type("room", 1));
        assert!(valid_lease_type("entire_house", 0));
        assert!(!valid_lease_type("room", 0));
        assert!(!valid_lease_type("entire_house", 1));
    }

    #[test]
    fn reusable_template_keeps_each_lease_stay_and_price() {
        let mut value = LeaseDetails {
            start_date: "2027-01-01".into(),
            end_date: "2027-04-30".into(),
            term_type: "fixed".into(),
            monthly_rent: "838".into(),
            rent_due_day: "1".into(),
            rent_deposit: "838".into(),
            unit_description: "Basement ensuite bedroom 1".into(),
            occupants: "Guest".into(),
            ..Default::default()
        };
        let mut terms = LeaseDetails {
            start_date: "2026-01-01".into(),
            monthly_rent: "999".into(),
            rent_payment_method: "e-transfer".into(),
            parking: "None".into(),
            condo: "No".into(),
            ..Default::default()
        };
        clear_lease_specific_terms(&mut terms);
        apply_reusable_terms(&mut value, &terms);
        assert_eq!(value.start_date, "2027-01-01");
        assert_eq!(value.end_date, "2027-04-30");
        assert_eq!(value.monthly_rent, "838");
        assert_eq!(value.rent_deposit, "838");
        assert_eq!(value.unit_description, "Basement ensuite bedroom 1");
        assert_eq!(value.occupants, "Guest");
        assert_eq!(value.rent_payment_method, "e-transfer");
    }

    #[test]
    fn access_roles_require_a_landlord_profile_only_for_landlords() {
        let mut input = AccessUpdate {
            first_name: "Helen".into(),
            last_name: "Bai".into(),
            email: "helen@example.com".into(),
            phone: String::new(),
            role: "landlord".into(),
            active: true,
            landlord_id: None,
            photo_url: None,
        };
        assert!(validate_access(&input).is_err());
        input.landlord_id = Some(1);
        assert!(validate_access(&input).is_ok());
        input.role = "staff".into();
        assert!(validate_access(&input).is_err());
        input.landlord_id = None;
        assert!(validate_access(&input).is_ok());
        input.last_name.clear();
        assert!(validate_access(&input).is_err());
    }

    #[tokio::test]
    async fn email_transport_sends_payload_and_checks_provider_receipt() {
        async fn accept(
            headers: HeaderMap,
            Json(body): Json<serde_json::Value>,
        ) -> Json<serde_json::Value> {
            assert_eq!(
                headers
                    .get("authorization")
                    .and_then(|value| value.to_str().ok()),
                Some("Bearer test-key")
            );
            assert_eq!(body["from"], "Buildry <test@buildry.ca>");
            assert_eq!(body["to"][0], "delivered@resend.dev");
            assert_eq!(body["subject"], "Buildry email check");
            assert_eq!(
                body["attachments"][0]["filename"],
                "signed-ontario-lease.pdf"
            );
            Json(serde_json::json!({"id":"provider-message-1"}))
        }
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/emails", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            axum::serve(listener, Router::new().route("/emails", post(accept)))
                .await
                .unwrap()
        });
        let sent = send_email_request(
            &url,
            "test-key",
            "Buildry <test@buildry.ca>",
            "delivered@resend.dev",
            "Buildry email check",
            "<p>ok</p>".into(),
            Some(("signed-ontario-lease.pdf", vec![1, 2, 3])),
        )
        .await;
        server.abort();
        assert!(sent.is_ok(), "{sent:?}");
    }
}
