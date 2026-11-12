//! Stripe collects the document through Stripe.js. Store only session state here.
use crate::{
    auth::{html_escape, AuthSession, Database},
    billing::Stripe,
    database_error, ApiResult,
};
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    Json,
};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub fn enabled() -> bool {
    std::env::var("STRIPE_IDENTITY_ENABLED").as_deref() == Ok("1")
}
fn unavailable() -> (StatusCode, String) {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        "Stripe Identity is unavailable. Try again later or contact Buildry.".into(),
    )
}
fn configured() -> ApiResult<Stripe> {
    if !enabled() {
        return Err(unavailable());
    }
    Stripe::configured()
        .filter(|s| s.publishable_key.is_some())
        .ok_or_else(unavailable)
}
fn headers() -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert("cache-control", "private, no-store".parse().unwrap());
    h.insert("referrer-policy", "no-referrer".parse().unwrap());
    h.insert("x-content-type-options", "nosniff".parse().unwrap());
    h
}

pub async fn migrate(db: &tokio_postgres::Client) -> Result<(), tokio_postgres::Error> {
    db.batch_execute("CREATE TABLE IF NOT EXISTS lease_identity_sessions (
        lease_id BIGINT PRIMARY KEY REFERENCES leases(id) ON DELETE CASCADE,
        request_key TEXT NOT NULL, session_id TEXT UNIQUE, test_mode BOOLEAN NOT NULL,
        status TEXT NOT NULL DEFAULT 'not_started', consent_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
        terms_version TEXT NOT NULL DEFAULT '2026-11-11', checked_at TIMESTAMPTZ,
        approved_at TIMESTAMPTZ, approved_by BIGINT REFERENCES users(id), redacted_at TIMESTAMPTZ, legal_hold BOOLEAN NOT NULL DEFAULT FALSE
    ); ALTER TABLE lease_identity_sessions ADD COLUMN IF NOT EXISTS legal_hold BOOLEAN NOT NULL DEFAULT FALSE").await
}

#[derive(Serialize)]
pub struct SessionState {
    status: String,
    approved: bool,
    verified_name: Option<String>,
}
fn session_status(value: &Value, id: &str, lease_id: i64, test_mode: bool) -> ApiResult<String> {
    if value["type"].as_str() != Some("document")
        || value["id"].as_str() != Some(id)
        || value["livemode"].as_bool() != Some(!test_mode)
        || value["metadata"]["lease_id"].as_str() != Some(lease_id.to_string().as_str())
    {
        return Err(unavailable());
    }
    if !value["redaction"].is_null() {
        return Ok("redacted".into());
    }
    let status = value["status"]
        .as_str()
        .filter(|s| ["requires_input", "processing", "verified", "canceled"].contains(s))
        .ok_or_else(unavailable)?;
    Ok(status.into())
}
async fn refresh(db: &Database, lease_id: i64, stripe: &Stripe) -> ApiResult<Option<Value>> {
    let row = db.lock().await.query_opt("SELECT session_id,test_mode,redacted_at IS NOT NULL FROM lease_identity_sessions WHERE lease_id=$1", &[&lease_id]).await.map_err(database_error)?;
    let Some(row) = row else { return Ok(None) };
    if row.get::<_, bool>(1) != stripe.test_mode() || row.get::<_, bool>(2) {
        return Err(unavailable());
    }
    let Some(id) = row.get::<_, Option<String>>(0) else {
        return Ok(None);
    };
    let value = stripe
        .request(
            Method::GET,
            &format!("identity/verification_sessions/{id}"),
            &[],
            None,
        )
        .await
        .map_err(|_| unavailable())?;
    let status = session_status(&value, &id, lease_id, stripe.test_mode())?;
    db.lock().await.execute("UPDATE lease_identity_sessions SET status=$2,checked_at=NOW(),approved_at=CASE WHEN $2='verified' THEN approved_at ELSE NULL END,approved_by=CASE WHEN $2='verified' THEN approved_by ELSE NULL END WHERE lease_id=$1 AND session_id=$3", &[&lease_id, &status, &id]).await.map_err(database_error)?;
    Ok(Some(value))
}
async fn tenant_lease(db: &Database, secret: &str, auth: &AuthSession) -> ApiResult<(i64, String)> {
    let user = crate::scope::user(auth)?;
    let hash = format!("{:x}", Sha256::digest(secret.as_bytes()));
    let row = db.lock().await.query_opt("SELECT id,tenant_name FROM leases WHERE token_hash=$1 AND token_expires_at>NOW() AND tenant_user_id=$2 AND status='signed' AND NOT is_demo", &[&hash, &user.id]).await.map_err(database_error)?.ok_or((StatusCode::NOT_FOUND, "Identity verification is not available for this lease.".into()))?;
    Ok((row.get(0), row.get(1)))
}

pub async fn page(secret: &str, name: &str) -> Response {
    // The SDK is loaded only after explicit consent and the verification button.
    let html = format!(
        r#"<!doctype html><html lang='en'><head><meta charset='utf-8'><meta name='viewport' content='width=device-width,initial-scale=1'><meta name='referrer' content='no-referrer'><title>Verify identity | Buildry</title><link rel='stylesheet' href='/assets/main-20261012d.css'></head><body class='signing-body'><main class='signing-shell'><header class='signing-header'><div class='signing-brand'>Buildry <span>Leasing</span></div></header><section class='signing-action photo-id-card shared-form-card' data-stripe-identity data-endpoint='/lease/{secret}/identity'><h1>Verify identity</h1><p>Stripe securely collects and checks your photo ID. Buildry receives the verification result and reviews it before confirming your tenancy.</p><form class='app-form sign-form' data-identity-form><label class='sign-consent'><input class='shared-form-checkbox' type='checkbox' name='consent' required><span>I agree to verify my ID with Stripe and have read the <a href='/photo-id-terms' target='_blank' rel='noopener noreferrer'>Photo ID terms and privacy notice</a>.</span></label>{verify_button}</form><p role='status' data-identity-status></p><div class='lease-id-review-actions'>{refresh_button}<a class='outline-button' href='/lease/{secret}'>Back to lease</a></div></section><footer class='signing-footer'>Private verification for {name}.</footer></main><script type='module' src='/assets/stripe-identity.mjs'></script></body></html>"#,
        verify_button = crate::components::forms::button("Verify with Stripe", "type='submit'"),
        refresh_button = crate::components::forms::action_button("Refresh status", "outline-button", "type='button' data-identity-refresh"),
        secret = html_escape(secret),
        name = html_escape(name)
    );
    let mut h = headers();
    h.insert("content-security-policy", "default-src 'self'; script-src 'self' https://js.stripe.com; frame-src https://js.stripe.com https://verify.stripe.com https://hooks.stripe.com; connect-src 'self' https://*.stripe.com; img-src 'self' data: https://*.stripe.com; style-src 'self' 'unsafe-inline'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'".parse().unwrap());
    (h, Html(html)).into_response()
}
#[derive(Deserialize)]
pub struct Consent {
    consent: bool,
}
#[derive(Serialize)]
pub struct Start {
    status: String,
    publishable_key: String,
    client_secret: Option<String>,
}
pub async fn start(
    Path(secret): Path<String>,
    State(db): State<Database>,
    auth: AuthSession,
    Json(input): Json<Consent>,
) -> ApiResult<(HeaderMap, Json<Start>)> {
    let (id, _) = tenant_lease(&db, &secret, &auth).await?;
    if !input.consent {
        return Err((
            StatusCode::BAD_REQUEST,
            "Agree to ID verification before continuing.".into(),
        ));
    }
    start_session(&db, id, &configured()?)
        .await
        .map(|session| (headers(), Json(session)))
}

async fn start_session(db: &Database, id: i64, stripe: &Stripe) -> ApiResult<Start> {
    // Persist a stable idempotency key before the remote call. Concurrent/retried
    // starts receive the same Stripe session, including after a timeout.
    db.lock().await.execute("INSERT INTO lease_identity_sessions(lease_id,request_key,test_mode) SELECT $1,$2,$3 WHERE NOT EXISTS(SELECT 1 FROM lease_photo_ids WHERE lease_id=$1) ON CONFLICT(lease_id) DO NOTHING", &[&id, &uuid::Uuid::new_v4().to_string(), &stripe.test_mode()]).await.map_err(database_error)?;
    let row = db.lock().await.query_opt("SELECT request_key,session_id,test_mode FROM lease_identity_sessions WHERE lease_id=$1 AND redacted_at IS NULL", &[&id]).await.map_err(database_error)?.ok_or((StatusCode::CONFLICT, "An ID is already on file. Contact Buildry.".into()))?;
    if row.get::<_, bool>(2) != stripe.test_mode() {
        return Err(unavailable());
    }
    let value = if row.get::<_, Option<String>>(1).is_some() {
        refresh(db, id, stripe).await?.ok_or_else(unavailable)?
    } else {
        let value = stripe
            .request(
                Method::POST,
                "identity/verification_sessions",
                &[
                    ("type", "document".into()),
                    ("metadata[lease_id]", id.to_string()),
                    ("client_reference_id", id.to_string()),
                    (
                        "options[document][allowed_types][]",
                        "driving_license".into(),
                    ),
                    ("options[document][allowed_types][]", "id_card".into()),
                    ("options[document][allowed_types][]", "passport".into()),
                    ("options[document][require_matching_selfie]", "false".into()),
                ],
                Some(&format!("lease-identity-{}", row.get::<_, String>(0))),
            )
            .await
            .map_err(|_| unavailable())?;
        let session = value["id"]
            .as_str()
            .filter(|id| {
                id.starts_with("vs_") && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            })
            .ok_or_else(unavailable)?;
        let status = session_status(&value, session, id, stripe.test_mode())?;
        db.lock().await.execute("UPDATE lease_identity_sessions SET session_id=$2,status=$3,checked_at=NOW() WHERE lease_id=$1 AND (session_id IS NULL OR session_id=$2)", &[&id, &session, &status]).await.map_err(database_error)?;
        value
    };
    let status = value["status"].as_str().unwrap_or("canceled").to_string();
    let secret = if status == "requires_input" {
        Some(
            value["client_secret"]
                .as_str()
                .ok_or_else(unavailable)?
                .to_string(),
        )
    } else {
        None
    };
    Ok(Start {
        status,
        publishable_key: stripe.publishable_key.clone().ok_or_else(unavailable)?,
        client_secret: secret,
    })
}
async fn state(db: &Database, id: i64) -> ApiResult<SessionState> {
    let row = db
        .lock()
        .await
        .query_opt(
            "SELECT status,approved_at IS NOT NULL FROM lease_identity_sessions WHERE lease_id=$1",
            &[&id],
        )
        .await
        .map_err(database_error)?;
    let remote = if row.is_some() {
        refresh(db, id, &configured()?).await?
    } else {
        None
    };
    let verified_name = remote
        .as_ref()
        .filter(|v| v["status"] == "verified")
        .map(|v| {
            format!(
                "{} {}",
                v["verified_outputs"]["first_name"].as_str().unwrap_or(""),
                v["verified_outputs"]["last_name"].as_str().unwrap_or("")
            )
            .trim()
            .to_string()
        })
        .filter(|v| !v.is_empty());
    let row = db
        .lock()
        .await
        .query_opt(
            "SELECT status,approved_at IS NOT NULL FROM lease_identity_sessions WHERE lease_id=$1",
            &[&id],
        )
        .await
        .map_err(database_error)?;
    Ok(match row {
        Some(r) => SessionState {
            status: r.get(0),
            approved: r.get(1),
            verified_name,
        },
        None => SessionState {
            status: "not_started".into(),
            approved: false,
            verified_name: None,
        },
    })
}
pub async fn tenant_status(
    Path(secret): Path<String>,
    State(db): State<Database>,
    auth: AuthSession,
) -> ApiResult<(HeaderMap, Json<SessionState>)> {
    let (id, _) = tenant_lease(&db, &secret, &auth).await?;
    configured()?;
    Ok((headers(), Json(state(&db, id).await?)))
}
async fn approver(db: &Database, id: i64, auth: &AuthSession) -> ApiResult<i64> {
    let user = crate::scope::user(auth)?;
    if db.lock().await.query_opt("SELECT 1 FROM leases WHERE id=$1 AND status='signed' AND NOT is_demo AND buildry_property_can($2,property_id,'leases.approve')", &[&id, &user.id]).await.map_err(database_error)?.is_none() { return Err((StatusCode::NOT_FOUND, "Lease not found.".into())); }
    Ok(user.id)
}
pub async fn admin_status(
    Path(id): Path<i64>,
    State(db): State<Database>,
    auth: AuthSession,
) -> ApiResult<(HeaderMap, Json<SessionState>)> {
    approver(&db, id, &auth).await?;
    Ok((headers(), Json(state(&db, id).await?)))
}
pub async fn approve(
    Path(id): Path<i64>,
    State(db): State<Database>,
    auth: AuthSession,
) -> ApiResult<StatusCode> {
    let user = approver(&db, id, &auth).await?;
    let stripe = configured()?;
    refresh(&db, id, &stripe).await?;
    let changed = db.lock().await.execute("UPDATE lease_identity_sessions s SET approved_at=NOW(),approved_by=$2 WHERE lease_id=$1 AND status='verified' AND test_mode=$3 AND redacted_at IS NULL AND EXISTS(SELECT 1 FROM leases l WHERE l.id=s.lease_id AND l.status='signed' AND buildry_property_can($2,l.property_id,'leases.approve'))", &[&id, &user, &stripe.test_mode()]).await.map_err(database_error)?;
    if changed != 1 {
        return Err((
            StatusCode::CONFLICT,
            "Stripe must verify the ID before approval.".into(),
        ));
    }
    Ok(StatusCode::NO_CONTENT)
}
/// Recheck with Stripe before using a human-approved result for confirmation.
pub async fn approved(db: &Database, id: i64) -> ApiResult<bool> {
    if db.lock().await.query_opt("SELECT 1 FROM lease_identity_sessions WHERE lease_id=$1 AND approved_at IS NOT NULL AND redacted_at IS NULL", &[&id]).await.map_err(database_error)?.is_none() { return Ok(false); }
    refresh(db, id, &configured()?).await?;
    Ok(db.lock().await.query_opt("SELECT 1 FROM lease_identity_sessions WHERE lease_id=$1 AND status='verified' AND approved_at IS NOT NULL AND redacted_at IS NULL", &[&id]).await.map_err(database_error)?.is_some())
}
pub async fn delete_expired(db: &Database) -> ApiResult<()> {
    let rows = db.lock().await.query("SELECT s.lease_id,s.session_id,s.test_mode FROM lease_identity_sessions s JOIN leases l ON l.id=s.lease_id WHERE s.redacted_at IS NULL AND NOT s.legal_hold AND l.tenancy_ended_at + INTERVAL '1 year' <= (NOW() AT TIME ZONE 'America/Toronto')::date", &[]).await.map_err(database_error)?;
    if rows.is_empty() {
        return Ok(());
    }
    let stripe = Stripe::configured().ok_or_else(unavailable)?;
    for row in rows {
        let id: i64 = row.get(0);
        if row.get::<_, bool>(2) != stripe.test_mode() {
            continue;
        }
        if let Some(session) = row.get::<_, Option<String>>(1) {
            stripe
                .request(
                    Method::POST,
                    &format!("identity/verification_sessions/{session}/redact"),
                    &[],
                    Some(&format!("redact-lease-identity-{id}")),
                )
                .await
                .map_err(|_| unavailable())?;
        }
        db.lock().await.execute("UPDATE lease_identity_sessions SET redacted_at=NOW(),status='redacted',approved_at=NULL,approved_by=NULL WHERE lease_id=$1", &[&id]).await.map_err(database_error)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn verification_requires_matching_session_lease_and_mode() {
        let value = serde_json::json!({"id":"vs_test", "type":"document", "livemode":false, "metadata":{"lease_id":"42"}, "status":"verified"});
        assert_eq!(
            session_status(&value, "vs_test", 42, true).unwrap(),
            "verified"
        );
        assert!(session_status(&value, "vs_other", 42, true).is_err());
        assert!(session_status(&value, "vs_test", 43, true).is_err());
        assert!(session_status(&value, "vs_test", 42, false).is_err());
        let mut redacted = value.clone();
        redacted["redaction"] = serde_json::json!({"status":"processing"});
        assert_eq!(
            session_status(&redacted, "vs_test", 42, true).unwrap(),
            "redacted"
        );
        let mut bad = value;
        bad["status"] = serde_json::json!("unknown");
        assert!(session_status(&bad, "vs_test", 42, true).is_err());
    }
    #[tokio::test]
    #[ignore = "requires an explicitly selected disposable _test database"]
    async fn identity_session_reuses_and_reconciles_provider_state() {
        use axum::{
            extract::State,
            routing::{get, post},
            Router,
        };
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        };
        let url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
        assert!(url.rsplit('/').next().unwrap().ends_with("_test"));
        let (client, connection) = tokio_postgres::connect(&url, tokio_postgres::NoTls)
            .await
            .unwrap();
        tokio::spawn(async move {
            connection.await.unwrap();
        });
        migrate(&client).await.unwrap();
        let id: i64 = client.query_one("INSERT INTO leases(tenant_name,tenant_email,property_id,room_id,details,landlord_snapshot,status,lease_type) SELECT 'Stripe Identity regression','stripe-identity-regression@example.test',property_id,room_id,details,landlord_snapshot,'signed',lease_type FROM leases ORDER BY id LIMIT 1 RETURNING id",&[]).await.unwrap().get(0);
        let db = Arc::new(tokio::sync::Mutex::new(client));
        let calls = Arc::new(AtomicUsize::new(0));
        let provider_state = Arc::new(tokio::sync::Mutex::new("requires_input".to_string()));
        let mock_state = (id, calls.clone(), provider_state.clone());
        async fn mock(
            State((id, calls, status)): State<(
                i64,
                Arc<AtomicUsize>,
                Arc<tokio::sync::Mutex<String>>,
            )>,
            headers: axum::http::HeaderMap,
            body: String,
        ) -> Json<Value> {
            assert_eq!(headers["authorization"], "Bearer sk_test_local_fixture");
            if !body.is_empty() {
                assert!(headers["idempotency-key"]
                    .to_str()
                    .unwrap()
                    .starts_with("lease-identity-"));
                assert!(body.contains("type=document"));
                calls.fetch_add(1, Ordering::SeqCst);
            }
            Json(
                serde_json::json!({"id":"vs_regression", "type":"document", "livemode":false,"metadata":{"lease_id":id.to_string()},"status":status.lock().await.clone(),"client_secret":"synthetic-client-secret"}),
            )
        }
        let app = Router::new()
            .route("/v1/identity/verification_sessions", post(mock))
            .route(
                "/v1/identity/verification_sessions/vs_regression",
                get(mock),
            )
            .with_state(mock_state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let stripe =
            Stripe::identity_test_client(format!("http://{}", listener.local_addr().unwrap()));
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let first = start_session(&db, id, &stripe).await.unwrap();
        assert_eq!(first.status, "requires_input");
        assert!(first.client_secret.is_some());
        let repeated = start_session(&db, id, &stripe).await.unwrap();
        assert_eq!(repeated.status, "requires_input");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        *provider_state.lock().await = "processing".into();
        assert!(start_session(&db, id, &stripe)
            .await
            .unwrap()
            .client_secret
            .is_none());
        *provider_state.lock().await = "verified".into();
        refresh(&db, id, &stripe).await.unwrap();
        let row=db.lock().await.query_one("SELECT status,approved_at IS NOT NULL FROM lease_identity_sessions WHERE lease_id=$1",&[&id]).await.unwrap();
        assert_eq!(row.get::<_, String>(0), "verified");
        assert!(
            !row.get::<_, bool>(1),
            "Stripe verification must not approve a tenancy"
        );
        db.lock()
            .await
            .execute(
                "UPDATE lease_identity_sessions SET approved_at=NOW() WHERE lease_id=$1",
                &[&id],
            )
            .await
            .unwrap();
        *provider_state.lock().await = "requires_input".into();
        refresh(&db, id, &stripe).await.unwrap();
        assert!(!db
            .lock()
            .await
            .query_one(
                "SELECT approved_at IS NOT NULL FROM lease_identity_sessions WHERE lease_id=$1",
                &[&id]
            )
            .await
            .unwrap()
            .get::<_, bool>(0));
        db.lock().await.execute("DELETE FROM leases WHERE id=$1 AND tenant_email='stripe-identity-regression@example.test'",&[&id]).await.unwrap();
        server.abort();
    }
}
