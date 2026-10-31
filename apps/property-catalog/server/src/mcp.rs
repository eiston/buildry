//! Buildry MCP endpoint and OAuth authorization server.
use crate::{auth, guest_listings, housework, Database};
use axum::{
    body::Bytes,
    extract::{Form, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
    Json, Router,
};
use base64::Engine;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio_postgres::Client;
use uuid::Uuid;

const SCOPE: &str = "listings.read";
const TASKS_READ: &str = "tasks.read";
const TASKS_WRITE: &str = "tasks.write";

fn valid_scopes(scopes: &str) -> bool {
    !scopes.is_empty()
        && scopes
            .split_whitespace()
            .all(|scope| [SCOPE, TASKS_READ, TASKS_WRITE].contains(&scope))
}

fn has_scope(scopes: &str, scope: &str) -> bool {
    scopes.split_whitespace().any(|item| item == scope)
}

fn origin() -> String {
    std::env::var("APP_PUBLIC_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:8080".into())
        .trim_end_matches('/')
        .to_owned()
}
fn resource() -> String {
    format!("{}/mcp", origin())
}
fn valid_resource(value: Option<&str>) -> bool {
    value.is_none_or(|value| value == resource())
}
fn random_secret() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}
fn hash(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

pub async fn migrate(db: &Client) -> Result<(), tokio_postgres::Error> {
    db.batch_execute(
        "CREATE TABLE IF NOT EXISTS mcp_oauth_clients (
            client_id TEXT PRIMARY KEY, client_name TEXT NOT NULL, redirect_uris JSONB NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );
        CREATE TABLE IF NOT EXISTS mcp_oauth_pending (
            nonce_hash TEXT PRIMARY KEY, client_id TEXT NOT NULL REFERENCES mcp_oauth_clients(client_id),
            redirect_uri TEXT NOT NULL, state TEXT, code_challenge TEXT NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );
        CREATE TABLE IF NOT EXISTS mcp_oauth_codes (
            code_hash TEXT PRIMARY KEY, client_id TEXT NOT NULL, user_id BIGINT NOT NULL REFERENCES users(id),
            redirect_uri TEXT NOT NULL, code_challenge TEXT NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );
        CREATE TABLE IF NOT EXISTS mcp_oauth_tokens (
            token_hash TEXT PRIMARY KEY, client_id TEXT NOT NULL, user_id BIGINT NOT NULL REFERENCES users(id),
            expires_at TIMESTAMPTZ NOT NULL, created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );
        CREATE TABLE IF NOT EXISTS mcp_oauth_refresh_tokens (
            token_hash TEXT PRIMARY KEY, client_id TEXT NOT NULL, user_id BIGINT NOT NULL REFERENCES users(id),
            expires_at TIMESTAMPTZ NOT NULL, created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );
        CREATE INDEX IF NOT EXISTS mcp_oauth_tokens_expiry ON mcp_oauth_tokens(expires_at);
        ALTER TABLE mcp_oauth_pending ADD COLUMN IF NOT EXISTS scope TEXT NOT NULL DEFAULT 'listings.read';
        ALTER TABLE mcp_oauth_codes ADD COLUMN IF NOT EXISTS scope TEXT NOT NULL DEFAULT 'listings.read';
        ALTER TABLE mcp_oauth_tokens ADD COLUMN IF NOT EXISTS scope TEXT NOT NULL DEFAULT 'listings.read';
        ALTER TABLE mcp_oauth_refresh_tokens ADD COLUMN IF NOT EXISTS scope TEXT NOT NULL DEFAULT 'listings.read';"
    ).await
}

pub fn routes() -> Router<Database> {
    Router::new()
        .route(
            "/.well-known/oauth-protected-resource",
            get(protected_resource),
        )
        .route(
            "/.well-known/oauth-protected-resource/mcp",
            get(protected_resource),
        )
        .route(
            "/.well-known/oauth-authorization-server",
            get(auth_metadata),
        )
        .route("/oauth/register", post(register))
        .route(
            "/oauth/authorize",
            get(authorize_page).post(authorize_submit),
        )
        .route("/oauth/token", post(token))
        .route("/mcp", get(mcp_get).post(mcp_post))
}

async fn protected_resource() -> Json<Value> {
    Json(
        json!({"resource": resource(), "authorization_servers": [origin()],
        "scopes_supported": [SCOPE, TASKS_READ, TASKS_WRITE], "bearer_methods_supported": ["header"],
        "resource_documentation": format!("{}/stays", origin())}),
    )
}
async fn auth_metadata() -> Json<Value> {
    let base = origin();
    Json(
        json!({"issuer": base, "authorization_endpoint": format!("{base}/oauth/authorize"),
        "token_endpoint": format!("{base}/oauth/token"),
        "registration_endpoint": format!("{base}/oauth/register"),
        "response_types_supported": ["code"], "grant_types_supported": ["authorization_code","refresh_token"],
        "token_endpoint_auth_methods_supported": ["none"],
        "code_challenge_methods_supported": ["S256"], "scopes_supported": [SCOPE, TASKS_READ, TASKS_WRITE]}),
    )
}

#[derive(Deserialize)]
struct Registration {
    client_name: Option<String>,
    redirect_uris: Vec<String>,
    token_endpoint_auth_method: Option<String>,
}
fn valid_redirect(value: &str) -> bool {
    reqwest::Url::parse(value).ok().is_some_and(|url| {
        url.fragment().is_none()
            && url.username().is_empty()
            && url.password().is_none()
            && (url.scheme() == "https"
                || (url.scheme() == "http"
                    && matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "::1"))))
    })
}
async fn register(State(db): State<Database>, Json(input): Json<Registration>) -> Response {
    if input.redirect_uris.is_empty()
        || input.redirect_uris.len() > 10
        || input.redirect_uris.iter().any(|uri| !valid_redirect(uri))
        || input
            .token_endpoint_auth_method
            .as_deref()
            .is_some_and(|method| method != "none")
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":"invalid_client_metadata"})),
        )
            .into_response();
    }
    let name = input.client_name.unwrap_or_else(|| "MCP client".into());
    if name.len() > 120 {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let client_id = random_secret();
    let uris = json!(input.redirect_uris);
    let uri_text = uris.to_string();
    let result = db.lock().await.execute("INSERT INTO mcp_oauth_clients(client_id,client_name,redirect_uris) VALUES($1,$2,$3::text::jsonb)", &[&client_id,&name,&uri_text]).await;
    if let Err(error) = result {
        eprintln!("MCP client registration failed: {error}");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    (
        StatusCode::CREATED,
        Json(json!({"client_id":client_id,"client_name":name,
        "redirect_uris":uris,"token_endpoint_auth_method":"none"})),
    )
        .into_response()
}

#[derive(Deserialize)]
struct AuthorizeQuery {
    request: Option<String>,
    response_type: Option<String>,
    client_id: Option<String>,
    redirect_uri: Option<String>,
    state: Option<String>,
    code_challenge: Option<String>,
    code_challenge_method: Option<String>,
    resource: Option<String>,
    scope: Option<String>,
}
async fn authorize_page(
    State(db): State<Database>,
    auth: auth::AuthSession,
    Query(q): Query<AuthorizeQuery>,
) -> Response {
    let nonce = if let Some(request) = q.request {
        if request.len() != 64 || !request.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return (StatusCode::BAD_REQUEST, "Invalid authorization request.").into_response();
        }
        request
    } else {
        let (Some(client_id), Some(redirect_uri), Some(challenge)) =
            (q.client_id, q.redirect_uri, q.code_challenge)
        else {
            return (StatusCode::BAD_REQUEST, "Invalid authorization request.").into_response();
        };
        if q.response_type.as_deref() != Some("code")
            || q.code_challenge_method.as_deref() != Some("S256")
            || !valid_resource(q.resource.as_deref())
            || challenge.len() != 43
            || !challenge
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            || !valid_scopes(q.scope.as_deref().unwrap_or(SCOPE))
        {
            return (StatusCode::BAD_REQUEST, "Invalid authorization request.").into_response();
        }
        let client = db
            .lock()
            .await
            .query_opt(
                "SELECT redirect_uris::text FROM mcp_oauth_clients WHERE client_id=$1",
                &[&client_id],
            )
            .await;
        let Ok(Some(row)) = client else {
            return (StatusCode::BAD_REQUEST, "Unknown OAuth client.").into_response();
        };
        let uris: Value = serde_json::from_str(&row.get::<_, String>(0)).unwrap_or_default();
        if !uris
            .as_array()
            .is_some_and(|items| items.iter().any(|uri| uri.as_str() == Some(&redirect_uri)))
        {
            return (StatusCode::BAD_REQUEST, "Unregistered redirect URI.").into_response();
        }
        let nonce = random_secret();
        let scope = q.scope.unwrap_or_else(|| SCOPE.into());
        let inserted = db.lock().await.execute(
            "INSERT INTO mcp_oauth_pending(nonce_hash,client_id,redirect_uri,state,code_challenge,scope) VALUES($1,$2,$3,$4,$5,$6)",
            &[&hash(&nonce),&client_id,&redirect_uri,&q.state,&challenge,&scope]).await;
        if inserted.is_err() {
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
        nonce
    };
    let details = db.lock().await.query_opt(
        "SELECT c.client_name,p.redirect_uri,p.scope FROM mcp_oauth_pending p JOIN mcp_oauth_clients c ON c.client_id=p.client_id WHERE p.nonce_hash=$1 AND p.created_at>NOW()-INTERVAL '10 minutes'",
        &[&hash(&nonce)]).await;
    let Ok(Some(row)) = details else {
        return (
            StatusCode::BAD_REQUEST,
            "Authorization request expired. Please reconnect.",
        )
            .into_response();
    };
    let Some(user) = auth.user else {
        return Redirect::to(&format!(
            "/login?next=%2Foauth%2Fauthorize%3Frequest%3D{nonce}"
        ))
        .into_response();
    };
    let client_name: String = row.get(0);
    let redirect_uri: String = row.get(1);
    let scope: String = row.get(2);
    if (has_scope(&scope, TASKS_READ) || has_scope(&scope, TASKS_WRITE))
        && !matches!(user.role.as_str(), "admin" | "staff" | "landlord")
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    let host = reqwest::Url::parse(&redirect_uri)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .unwrap_or_default();
    let html = include_str!("oauth_consent.html")
        .replace("{{CLIENT_NAME}}", &escape(&client_name))
        .replace("{{CLIENT_HOST}}", &escape(&host))
        .replace("{{USER_EMAIL}}", &escape(&user.email))
        .replace("{{LISTINGS_PERMISSION}}", if has_scope(&scope, SCOPE) { "<div class=\"permission\"><div><strong>View public listings</strong><span>Read houses, rooms, prices, availability, and listing images.</span></div></div>" } else { "" })
        .replace("{{TASKS_READ_PERMISSION}}", if has_scope(&scope, TASKS_READ) { "<div class=\"permission\"><div><strong>View tasks</strong><span>Read task titles, descriptions, assignments, schedules, and images.</span></div></div>" } else { "" })
        .replace("{{TASKS_WRITE_PERMISSION}}", if has_scope(&scope, TASKS_WRITE) { "<div class=\"permission\"><div><strong>Create tasks</strong><span>Add tasks and assign them to house managers.</span></div></div>" } else { "" })
        .replace("{{ALLOW_LABEL}}", if has_scope(&scope, TASKS_WRITE) { "Allow task creation" } else { "Allow access" })
        .replace("{{NONCE}}", &nonce);
    Html(html).into_response()
}

#[derive(Deserialize)]
struct Consent {
    nonce: String,
    decision: String,
}
async fn authorize_submit(
    State(db): State<Database>,
    auth: auth::AuthSession,
    Form(form): Form<Consent>,
) -> Response {
    if form.decision == "allow" && auth.user.is_none() {
        return Redirect::to(&format!(
            "/login?next=%2Foauth%2Fauthorize%3Frequest%3D{}",
            form.nonce
        ))
        .into_response();
    }
    let row = db.lock().await.query_opt("DELETE FROM mcp_oauth_pending WHERE nonce_hash=$1 AND created_at > NOW()-INTERVAL '10 minutes' RETURNING client_id,redirect_uri,state,code_challenge,scope", &[&hash(&form.nonce)]).await;
    let Ok(Some(row)) = row else {
        return (
            StatusCode::BAD_REQUEST,
            "Authorization request expired. Please reconnect.",
        )
            .into_response();
    };
    let client_id: String = row.get(0);
    let redirect_uri: String = row.get(1);
    let state: Option<String> = row.get(2);
    let challenge: String = row.get(3);
    let scope: String = row.get(4);
    let mut redirect = match reqwest::Url::parse(&redirect_uri) {
        Ok(u) => u,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    if form.decision != "allow" {
        redirect
            .query_pairs_mut()
            .append_pair("error", "access_denied");
    } else {
        let Some(user) = auth.user else {
            return StatusCode::UNAUTHORIZED.into_response();
        };
        if (has_scope(&scope, TASKS_READ) || has_scope(&scope, TASKS_WRITE))
            && !matches!(user.role.as_str(), "admin" | "staff" | "landlord")
        {
            return StatusCode::FORBIDDEN.into_response();
        }
        let code = random_secret();
        let inserted = db.lock().await.execute("INSERT INTO mcp_oauth_codes(code_hash,client_id,user_id,redirect_uri,code_challenge,scope) VALUES($1,$2,$3,$4,$5,$6)", &[&hash(&code),&client_id,&user.id,&redirect_uri,&challenge,&scope]).await;
        if inserted.is_err() {
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
        redirect.query_pairs_mut().append_pair("code", &code);
    }
    if let Some(state) = state {
        redirect.query_pairs_mut().append_pair("state", &state);
    }
    Redirect::to(redirect.as_str()).into_response()
}

#[derive(Deserialize)]
struct TokenRequest {
    grant_type: String,
    code: Option<String>,
    redirect_uri: Option<String>,
    client_id: String,
    code_verifier: Option<String>,
    refresh_token: Option<String>,
    resource: Option<String>,
}
async fn token(State(db): State<Database>, Form(form): Form<TokenRequest>) -> Response {
    if !valid_resource(form.resource.as_deref()) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":"invalid_request"})),
        )
            .into_response();
    }
    let client = db.lock().await;
    let (user_id, scope): (i64, String) = match form.grant_type.as_str() {
        "authorization_code" => {
            let (Some(code), Some(redirect_uri), Some(verifier)) =
                (&form.code, &form.redirect_uri, &form.code_verifier)
            else {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error":"invalid_request"})),
                )
                    .into_response();
            };
            if !(43..=128).contains(&verifier.len())
                || !verifier
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-._~".contains(&b))
            {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error":"invalid_request"})),
                )
                    .into_response();
            }
            let row = client.query_opt("DELETE FROM mcp_oauth_codes WHERE code_hash=$1 AND client_id=$2 AND redirect_uri=$3 AND created_at > NOW()-INTERVAL '10 minutes' RETURNING user_id,code_challenge,scope", &[&hash(code),&form.client_id,redirect_uri]).await;
            let Ok(Some(row)) = row else {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error":"invalid_grant"})),
                )
                    .into_response();
            };
            let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(Sha256::digest(verifier.as_bytes()));
            if challenge != row.get::<_, String>(1) {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error":"invalid_grant"})),
                )
                    .into_response();
            }
            (row.get(0), row.get(2))
        }
        "refresh_token" => {
            let Some(refresh) = &form.refresh_token else {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error":"invalid_request"})),
                )
                    .into_response();
            };
            let row = client.query_opt("DELETE FROM mcp_oauth_refresh_tokens WHERE token_hash=$1 AND client_id=$2 AND expires_at>NOW() RETURNING user_id,scope", &[&hash(refresh),&form.client_id]).await;
            let Ok(Some(row)) = row else {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error":"invalid_grant"})),
                )
                    .into_response();
            };
            (row.get(0), row.get(1))
        }
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error":"unsupported_grant_type"})),
            )
                .into_response()
        }
    };
    let role = client
        .query_opt("SELECT role FROM users WHERE id=$1 AND active", &[&user_id])
        .await
        .ok()
        .flatten()
        .map(|row| row.get::<_, String>(0));
    if role.is_none()
        || ((has_scope(&scope, TASKS_READ) || has_scope(&scope, TASKS_WRITE))
            && !matches!(role.as_deref(), Some("admin" | "staff" | "landlord")))
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":"invalid_grant"})),
        )
            .into_response();
    }
    let access_token = random_secret();
    let refresh_token = random_secret();
    let inserted = client.execute("INSERT INTO mcp_oauth_tokens(token_hash,client_id,user_id,expires_at,scope) VALUES($1,$2,$3,NOW()+INTERVAL '1 hour',$4)", &[&hash(&access_token),&form.client_id,&user_id,&scope]).await;
    if inserted.is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    let inserted = client.execute("INSERT INTO mcp_oauth_refresh_tokens(token_hash,client_id,user_id,expires_at,scope) VALUES($1,$2,$3,NOW()+INTERVAL '30 days',$4)", &[&hash(&refresh_token),&form.client_id,&user_id,&scope]).await;
    if inserted.is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    ([(header::CACHE_CONTROL,"no-store"),(header::PRAGMA,"no-cache")],Json(json!({"access_token":access_token,"refresh_token":refresh_token,"token_type":"Bearer","expires_in":3600,"scope":scope}))).into_response()
}

fn challenge() -> Response {
    let value = format!("Bearer resource_metadata=\"{}/.well-known/oauth-protected-resource/mcp\", scope=\"{}\", error=\"invalid_token\", error_description=\"Authentication required\"",origin(),SCOPE);
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, value)],
        "Authentication required",
    )
        .into_response()
}
async fn authorized(db: &Database, headers: &HeaderMap) -> Option<(String, String, i64)> {
    let Some(raw) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
    else {
        return None;
    };
    db.lock().await.query_opt("SELECT t.scope,u.role,u.id FROM mcp_oauth_tokens t JOIN users u ON u.id=t.user_id WHERE t.token_hash=$1 AND t.expires_at>NOW() AND u.active", &[&hash(raw)]).await.ok().flatten().map(|row| (row.get(0), row.get(1), row.get(2)))
}
async fn mcp_get(State(db): State<Database>, headers: HeaderMap) -> Response {
    if authorized(&db, &headers).await.is_none() {
        return challenge();
    }
    (StatusCode::METHOD_NOT_ALLOWED, "Use POST for MCP requests.").into_response()
}

#[derive(Deserialize)]
struct Rpc {
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}
fn rpc(id: Value, result: Value) -> Response {
    Json(json!({"jsonrpc":"2.0","id":id,"result":result})).into_response()
}
fn rpc_error(id: Value, code: i32, message: &str) -> Response {
    Json(json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})).into_response()
}
fn tool_result(id: Value, value: Value) -> Response {
    rpc(
        id,
        json!({"content":[{"type":"text","text":value.to_string()}],"structuredContent":value}),
    )
}
fn tool_error(id: Value, message: &str) -> Response {
    rpc(
        id,
        json!({"content":[{"type":"text","text":message}],"isError":true}),
    )
}
async fn mcp_post(State(db): State<Database>, headers: HeaderMap, body: Bytes) -> Response {
    let Some((scopes, role, user_id)) = authorized(&db, &headers).await else {
        return challenge();
    };
    let input: Rpc = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(_) => return rpc_error(Value::Null, -32700, "Invalid JSON-RPC request"),
    };
    let Some(id) = input.id else {
        return StatusCode::ACCEPTED.into_response();
    };
    match input.method.as_str() {
        "initialize" => rpc(
            id,
            json!({"protocolVersion":"2025-06-18","capabilities":{"tools":{}},"serverInfo":{"name":"buildry-property-catalog","version":"1.0.0"}}),
        ),
        "ping" => rpc(id, json!({})),
        "tools/list" => {
            let mut tools = Vec::new();
            if has_scope(&scopes, SCOPE) {
                tools.push(json!({"name":"read_guest_listings","title":"Read Buildry listings","description":"Read public house and room listings, prices, amenities, occupancy and all image URLs. Images are illustrative where marked in their URLs.","inputSchema":{"type":"object","properties":{},"additionalProperties":false},"annotations":{"readOnlyHint":true},"_meta":{"securitySchemes":[{"type":"oauth2","scopes":[SCOPE]}]}}));
                tools.push(json!({"name":"get_listing_image","title":"View a listing image","description":"Return an image from a public Buildry listing by its numeric image ID. Use image IDs found in read_guest_listings photo URLs.","inputSchema":{"type":"object","properties":{"image_id":{"type":"integer","minimum":1}},"required":["image_id"],"additionalProperties":false},"annotations":{"readOnlyHint":true},"_meta":{"securitySchemes":[{"type":"oauth2","scopes":[SCOPE]}]}}));
            }
            if matches!(role.as_str(), "admin" | "staff" | "landlord")
                && has_scope(&scopes, TASKS_READ)
            {
                tools.push(json!({"name":"list_tasks","title":"List Buildry tasks","description":"List tasks with IDs, title, description, status, house, assignee, schedule, and image URLs.","inputSchema":{"type":"object","properties":{},"additionalProperties":false},"annotations":{"readOnlyHint":true},"_meta":{"securitySchemes":[{"type":"oauth2","scopes":[TASKS_READ]}]}}));
                tools.push(json!({"name":"get_task","title":"View a Buildry task","description":"View one task by numeric ID.","inputSchema":{"type":"object","properties":{"task_id":{"type":"integer","minimum":1}},"required":["task_id"],"additionalProperties":false},"annotations":{"readOnlyHint":true},"_meta":{"securitySchemes":[{"type":"oauth2","scopes":[TASKS_READ]}]}}));
            }
            if matches!(role.as_str(), "admin" | "staff" | "landlord")
                && (has_scope(&scopes, TASKS_READ) || has_scope(&scopes, TASKS_WRITE))
            {
                tools.push(json!({"name":"list_task_options","title":"List task houses and managers","description":"Get house IDs and active house-manager names for task creation.","inputSchema":{"type":"object","properties":{},"additionalProperties":false},"annotations":{"readOnlyHint":true},"_meta":{"securitySchemes":[{"type":"oauth2","scopes":[TASKS_READ]},{"type":"oauth2","scopes":[TASKS_WRITE]}]}}));
            }
            if matches!(role.as_str(), "admin" | "staff" | "landlord")
                && has_scope(&scopes, TASKS_WRITE)
            {
                tools.push(json!({"name":"create_task","title":"Create a Buildry task","description":"Create a task in Backlog. A house ID and active house manager name are required; use list_task_options to find them. The creator and all current managers of the house can see it. Planning and scheduling happen in the task page.","inputSchema":{"type":"object","properties":{"title":{"type":"string","minLength":1,"maxLength":180},"description":{"type":"string","maxLength":10000},"property_id":{"type":"integer","minimum":1},"assignee":{"type":"string","minLength":1,"maxLength":120}},"required":["title","property_id","assignee"],"additionalProperties":false},"annotations":{"readOnlyHint":false,"destructiveHint":false},"_meta":{"securitySchemes":[{"type":"oauth2","scopes":[TASKS_WRITE]}]}}));
            }
            rpc(id, json!({"tools":tools}))
        }
        "tools/call" => match input.params.get("name").and_then(Value::as_str) {
            Some("read_guest_listings") if has_scope(&scopes, SCOPE) => {
                match guest_listings(State(db.clone())).await {
                    Ok(Json(listings)) => {
                        let mut value = serde_json::to_value(listings).unwrap_or_default();
                        if let Some(items) = value.as_array_mut() {
                            for item in items {
                                absolutize_photos(item.get_mut("photos"));
                                if let Some(rooms) =
                                    item.get_mut("rooms").and_then(Value::as_array_mut)
                                {
                                    for room in rooms {
                                        absolutize_photos(room.get_mut("photos"));
                                    }
                                }
                            }
                        }
                        rpc(
                            id,
                            json!({"content":[{"type":"text","text":value.to_string()}],"structuredContent":{"listings":value}}),
                        )
                    }
                    Err(_) => rpc_error(id, -32603, "Could not load listings"),
                }
            }
            Some("get_listing_image") if has_scope(&scopes, SCOPE) => {
                let Some(image_id) = input
                    .params
                    .get("arguments")
                    .and_then(|a| a.get("image_id"))
                    .and_then(Value::as_i64)
                    .filter(|v| *v > 0)
                else {
                    return rpc_error(id, -32602, "image_id must be a positive integer");
                };
                let Ok(Json(listings)) = guest_listings(State(db.clone())).await else {
                    return rpc_error(id, -32603, "Could not load listings");
                };
                let needle = format!("/images/{image_id}");
                let allowed = listings
                    .iter()
                    .flat_map(|l| {
                        l.photos
                            .iter()
                            .chain(l.rooms.iter().flat_map(|r| r.photos.iter()))
                    })
                    .any(|url| url.split('?').next().unwrap_or("").ends_with(&needle));
                if !allowed {
                    return rpc_error(id, -32602, "Image is not part of a public listing");
                }
                let row = db
                    .lock()
                    .await
                    .query_opt(
                        "SELECT content_type,data FROM property_images WHERE id=$1",
                        &[&image_id],
                    )
                    .await;
                let Ok(Some(row)) = row else {
                    return rpc_error(id, -32602, "Image not found");
                };
                let mime: String = row.get(0);
                let data: Vec<u8> = row.get(1);
                if !["image/jpeg", "image/png", "image/webp"].contains(&mime.as_str()) {
                    return rpc_error(id, -32603, "Unsupported image type");
                }
                rpc(
                    id,
                    json!({"content":[{"type":"image","mimeType":mime,"data":base64::engine::general_purpose::STANDARD.encode(data)}]}),
                )
            }
            Some("list_tasks")
                if has_scope(&scopes, TASKS_READ)
                    && matches!(role.as_str(), "admin" | "staff" | "landlord") =>
            {
                match housework::list_for(db.clone(), user_id).await {
                    Ok(Json(tasks)) => {
                        let mut value = serde_json::to_value(tasks).unwrap_or_default();
                        if let Some(items) = value.as_array_mut() {
                            for item in items {
                                absolutize_photos(item.get_mut("photos"));
                            }
                        }
                        tool_result(id, json!({"tasks":value}))
                    }
                    Err(_) => tool_error(id, "Could not load tasks"),
                }
            }
            Some("get_task")
                if has_scope(&scopes, TASKS_READ)
                    && matches!(role.as_str(), "admin" | "staff" | "landlord") =>
            {
                let Some(task_id) = input
                    .params
                    .get("arguments")
                    .and_then(|a| a.get("task_id"))
                    .and_then(Value::as_i64)
                    .filter(|v| *v > 0)
                else {
                    return rpc_error(id, -32602, "task_id must be a positive integer");
                };
                match housework::list_for(db.clone(), user_id).await {
                    Ok(Json(tasks)) => match tasks.into_iter().find(|task| task.id == task_id) {
                        Some(task) => {
                            let mut value = serde_json::to_value(task).unwrap_or_default();
                            absolutize_photos(value.get_mut("photos"));
                            tool_result(id, json!({"task":value}))
                        }
                        None => tool_error(id, "Task not found"),
                    },
                    Err(_) => tool_error(id, "Could not load task"),
                }
            }
            Some("list_task_options")
                if (has_scope(&scopes, TASKS_READ) || has_scope(&scopes, TASKS_WRITE))
                    && matches!(role.as_str(), "admin" | "staff" | "landlord") =>
            {
                let client = db.lock().await;
                let houses = client.query(&format!("SELECT p.id,COALESCE(p.data->>'name',''),COALESCE(p.data->'manager_staff_ids','[]'::jsonb)::text,COALESCE(p.data->>'owner_name',''),COALESCE((p.data->>'owner_manages_tours')::boolean,false) FROM properties p WHERE {} ORDER BY p.data->>'name'", crate::scope::property_write_sql("p", 1)), &[&user_id]).await;
                let managers = client.query("SELECT id,btrim(first_name || ' ' || last_name) FROM users WHERE role='staff' AND active ORDER BY first_name,last_name", &[]).await;
                match (houses, managers) {
                    (Ok(houses), Ok(managers)) => {
                        let staff: Vec<(i64, String)> = managers
                            .into_iter()
                            .map(|row| (row.get(0), row.get(1)))
                            .collect();
                        let houses: Vec<Value> = houses.into_iter().map(|row| {
                            let ids: Vec<i64> = serde_json::from_str(&row.get::<_,String>(2)).unwrap_or_default();
                            let mut names: Vec<String> = staff.iter().filter(|(id,_)| ids.contains(id)).map(|(_,name)| name.clone()).collect();
                            let owner: String = row.get(3);
                            if row.get::<_,bool>(4) && !owner.is_empty() { names.push(owner); }
                            json!({"id":row.get::<_,i64>(0),"name":row.get::<_,String>(1),"assignees":names})
                        }).collect();
                        tool_result(id, json!({"houses":houses}))
                    }
                    _ => tool_error(id, "Could not load task options"),
                }
            }
            Some("create_task")
                if has_scope(&scopes, TASKS_WRITE)
                    && matches!(role.as_str(), "admin" | "staff" | "landlord") =>
            {
                let Some(args) = input.params.get("arguments") else {
                    return rpc_error(id, -32602, "Task details are required");
                };
                let Some(title) = args.get("title").and_then(Value::as_str) else {
                    return rpc_error(id, -32602, "title is required");
                };
                let Some(property_id) = args
                    .get("property_id")
                    .and_then(Value::as_i64)
                    .filter(|id| *id > 0)
                else {
                    return rpc_error(id, -32602, "property_id must be a positive integer");
                };
                let Some(assignee) = args.get("assignee").and_then(Value::as_str) else {
                    return rpc_error(id, -32602, "assignee is required");
                };
                let description = args
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let task = housework::HouseworkInput {
                    title: title.into(),
                    description: description.into(),
                    property_id: Some(property_id),
                    assignee: assignee.into(),
                    status: "backlog".into(),
                    start_date: None,
                    start_time: None,
                    estimate_minutes: None,
                    photos: vec![],
                };
                match housework::create_for(db.clone(), user_id, &role, task).await {
                    Ok(Json(saved)) => tool_result(id, json!({"task":saved})),
                    Err((_, message)) => tool_error(id, &message),
                }
            }
            _ => rpc_error(id, -32601, "Unknown tool"),
        },
        _ => rpc_error(id, -32601, "Unknown method"),
    }
}
fn absolutize_photos(photos: Option<&mut Value>) {
    if let Some(items) = photos.and_then(Value::as_array_mut) {
        for photo in items {
            if let Some(path) = photo.as_str().filter(|s| s.starts_with('/')) {
                *photo = Value::String(format!("{}{}", origin(), path));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn redirect_uris_are_limited_to_https_or_localhost() {
        assert!(valid_redirect(
            "https://chatgpt.com/connector/oauth/callback"
        ));
        assert!(valid_redirect("http://127.0.0.1:49152/callback"));
        assert!(!valid_redirect("http://other.example/callback"));
        assert!(!valid_redirect("https://user:pass@example.com/callback"));
        assert!(!valid_redirect("https://example.com/callback#fragment"));
    }
    #[test]
    fn photo_urls_are_absolute_for_remote_agents() {
        let mut photos = json!([
            "/images/42?illustrative=ai",
            "https://cdn.example/photo.jpg"
        ]);
        absolutize_photos(Some(&mut photos));
        assert_eq!(photos[0], format!("{}/images/42?illustrative=ai", origin()));
        assert_eq!(photos[1], "https://cdn.example/photo.jpg");
    }
    #[test]
    fn task_scopes_are_explicit_and_do_not_come_from_listing_access() {
        assert!(valid_scopes("listings.read tasks.read tasks.write"));
        assert!(valid_scopes("tasks.read"));
        assert!(!valid_scopes(""));
        assert!(!valid_scopes("listings.read tasks.admin"));
        assert!(!has_scope("listings.read", TASKS_READ));
        assert!(!has_scope("tasks.read", TASKS_WRITE));
    }
    #[test]
    fn resource_can_be_omitted_but_cannot_name_another_server() {
        assert!(valid_resource(None));
        assert!(valid_resource(Some(&resource())));
        assert!(!valid_resource(Some("https://other.example/mcp")));
    }
}
