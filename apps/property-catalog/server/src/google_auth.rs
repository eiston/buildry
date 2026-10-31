use crate::auth::{self, Backend, Database};
use axum::{
    extract::{Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Redirect, Response},
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::time::Duration;

fn config() -> Option<(String, String, String)> {
    let id = std::env::var("GOOGLE_CLIENT_ID").ok()?;
    let secret = std::env::var("GOOGLE_CLIENT_SECRET").ok()?;
    let origin = std::env::var("APP_PUBLIC_URL").ok()?;
    let origin = origin.trim_end_matches('/');
    if id.is_empty() || secret.is_empty() || !origin.starts_with("https://") {
        return None;
    }
    Some((id, secret, format!("{origin}/auth/google/callback")))
}

pub fn signin_html(next: Option<&str>) -> String {
    if config().is_none() {
        return String::new();
    }
    let mut url = reqwest::Url::parse("https://app.buildry.ca/auth/google/start").unwrap();
    if let Some(next) = auth::oauth_return_path(next) {
        url.query_pairs_mut().append_pair("next", next);
    }
    format!(
        "<div class=\"social-signin\"><p class=\"social-signin-label\">Or continue with</p><a class=\"google-signin\" href=\"{}\" aria-label=\"Continue with Google\" title=\"Continue with Google\"><svg viewBox=\"0 0 24 24\" aria-hidden=\"true\" focusable=\"false\"><path fill=\"#4285F4\" d=\"M21.35 12.22c0-.78-.07-1.52-.2-2.23H12v4.22h5.23a4.47 4.47 0 0 1-1.94 2.94v2.44h3.14c1.84-1.69 2.92-4.18 2.92-7.37Z\"/><path fill=\"#34A853\" d=\"M12 21.5c2.63 0 4.84-.87 6.45-2.35l-3.14-2.44c-.87.58-1.98.93-3.31.93-2.54 0-4.7-1.72-5.47-4.03H3.3v2.51A9.75 9.75 0 0 0 12 21.5Z\"/><path fill=\"#FBBC05\" d=\"M6.53 13.61a5.85 5.85 0 0 1 0-3.72V7.38H3.3a9.75 9.75 0 0 0 0 8.74l3.23-2.51Z\"/><path fill=\"#EA4335\" d=\"M12 5.86c1.43 0 2.71.49 3.72 1.45l2.79-2.79A9.36 9.36 0 0 0 12 2.5a9.75 9.75 0 0 0-8.7 5.38l3.23 2.51C7.3 7.58 9.46 5.86 12 5.86Z\"/></svg></a></div>",
        url.path().to_string() + &url.query().map(|q| format!("?{q}")).unwrap_or_default()
    )
}

fn hash(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

fn may_auto_link(role: &str, password_hash: &str, email: &str) -> bool {
    role == "guest"
        && !password_hash.is_empty()
        && !password_hash.starts_with("google-only:")
        && (email.ends_with("@gmail.com") || email.ends_with("@googlemail.com"))
}

#[cfg(test)]
mod linking_tests {
    use super::may_auto_link;

    #[test]
    fn only_verified_password_guest_with_gmail_can_auto_link() {
        assert!(may_auto_link("guest", "argon2-hash", "owner@gmail.com"));
        assert!(!may_auto_link("admin", "argon2-hash", "owner@gmail.com"));
        assert!(!may_auto_link("guest", "", "owner@gmail.com"));
        assert!(!may_auto_link("guest", "google-only:old", "owner@gmail.com"));
        assert!(!may_auto_link("guest", "argon2-hash", "owner@company.com"));
    }
}

#[derive(Deserialize)]
pub struct StartQuery {
    next: Option<String>,
}

async fn begin(db: Database, user_id: Option<i64>, next: Option<String>) -> Response {
    let Some((client_id, _, redirect_uri)) = config() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "Google sign-in is not configured.",
        )
            .into_response();
    };
    let state = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let next = auth::oauth_return_path(next.as_deref()).map(str::to_owned);
    if db.lock().await.execute(
        "INSERT INTO google_login_states(state_hash,user_id,next_path,expires_at) VALUES($1,$2,$3,NOW()+INTERVAL '10 minutes')",
        &[&hash(&state), &user_id, &next],
    ).await.is_err() { return StatusCode::INTERNAL_SERVER_ERROR.into_response(); }
    let mut url = reqwest::Url::parse("https://accounts.google.com/o/oauth2/v2/auth").unwrap();
    url.query_pairs_mut()
        .append_pair("client_id", &client_id)
        .append_pair("redirect_uri", &redirect_uri)
        .append_pair("response_type", "code")
        .append_pair("scope", "openid email profile")
        .append_pair("state", &state);
    let cookie = format!("google_oauth_state={state}; Path=/auth/google; HttpOnly; SameSite=None; Secure; Max-Age=600");
    let mut response = Redirect::to(url.as_str()).into_response();
    response
        .headers_mut()
        .insert(header::SET_COOKIE, HeaderValue::from_str(&cookie).unwrap());
    response
}

pub async fn start(State(db): State<Database>, Query(query): Query<StartQuery>) -> Response {
    begin(db, None, query.next).await
}

pub async fn link(State(db): State<Database>, auth: auth::AuthSession) -> Response {
    let Some(user) = auth.user else {
        return Redirect::to("/login?next=%2Fauth%2Fgoogle%2Flink").into_response();
    };
    begin(db, Some(user.id), None).await
}

#[derive(Deserialize)]
pub struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}
#[derive(Deserialize)]
struct Token {
    access_token: String,
}
#[derive(Deserialize)]
struct Profile {
    sub: String,
    email: String,
    email_verified: bool,
    #[serde(default)]
    given_name: String,
    #[serde(default)]
    family_name: String,
}

pub async fn callback(
    State(db): State<Database>,
    mut auth: auth::AuthSession,
    headers: HeaderMap,
    Query(query): Query<CallbackQuery>,
) -> Response {
    if query.error.is_some() {
        return Redirect::to("/login?google_error=cancelled").into_response();
    }
    let (Some(code), Some(state), Some((client_id, client_secret, redirect_uri))) =
        (query.code, query.state, config())
    else {
        return Redirect::to("/login?google_error=failed").into_response();
    };
    let cookie_state = headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|cookies| {
            cookies
                .split(';')
                .map(str::trim)
                .find_map(|cookie| cookie.strip_prefix("google_oauth_state="))
        });
    if state.len() != 64 || cookie_state != Some(state.as_str()) {
        return Redirect::to("/login?google_error=expired").into_response();
    }
    let row = db.lock().await.query_opt(
        "DELETE FROM google_login_states WHERE state_hash=$1 AND expires_at>NOW() RETURNING user_id,next_path",
        &[&hash(&state)],
    ).await;
    let Ok(Some(row)) = row else {
        return Redirect::to("/login?google_error=expired").into_response();
    };
    let link_id: Option<i64> = row.get(0);
    let next: Option<String> = row.get(1);
    if link_id.is_some() && auth.user.as_ref().is_some_and(|user| Some(user.id) != link_id) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
    {
        Ok(client) => client,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let response = client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("code", code.as_str()),
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("redirect_uri", redirect_uri.as_str()),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await;
    let Ok(response) = response else {
        return Redirect::to("/login?google_error=failed").into_response();
    };
    if !response.status().is_success() {
        return Redirect::to("/login?google_error=failed").into_response();
    }
    let Ok(token) = response.json::<Token>().await else {
        return Redirect::to("/login?google_error=failed").into_response();
    };
    let response = client
        .get("https://openidconnect.googleapis.com/v1/userinfo")
        .bearer_auth(token.access_token)
        .send()
        .await;
    let Ok(response) = response else {
        return Redirect::to("/login?google_error=failed").into_response();
    };
    if !response.status().is_success() {
        return Redirect::to("/login?google_error=failed").into_response();
    }
    let Ok(profile) = response.json::<Profile>().await else {
        return Redirect::to("/login?google_error=failed").into_response();
    };
    let email = profile.email.trim().to_lowercase();
    if !profile.email_verified
        || profile.sub.is_empty()
        || email.is_empty()
        || email.len() > 254
        || !email.contains('@')
    {
        return (
            StatusCode::FORBIDDEN,
            "Google did not provide a verified email address.",
        )
            .into_response();
    }
    let user_id = if let Some(id) = link_id {
        let linked_account = db.lock().await.query_opt("SELECT email,active FROM users WHERE id=$1", &[&id]).await;
        let Ok(Some(linked_account)) = linked_account else {
            return StatusCode::FORBIDDEN.into_response();
        };
        let account_email: String = linked_account.get(0);
        if !linked_account.get::<_, bool>(1) || !account_email.eq_ignore_ascii_case(&email) {
            return (
                StatusCode::CONFLICT,
                "Use the Google account with the same email as your Buildry account.",
            )
                .into_response();
        }
        let result = db.lock().await.execute(
            "UPDATE users SET google_sub=$1 WHERE id=$2 AND active AND (google_sub IS NULL OR google_sub=$1)",
            &[&profile.sub, &id]).await;
        if !matches!(result, Ok(1)) {
            return (
                StatusCode::CONFLICT,
                "Google account is already linked elsewhere.",
            )
                .into_response();
        }
        id
    } else {
        let mut client = db.lock().await;
        let tx = match client.transaction().await {
            Ok(tx) => tx,
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        };
        let id = match tx.query_opt("SELECT id,active FROM users WHERE google_sub=$1 FOR UPDATE", &[&profile.sub]).await {
            Ok(Some(row)) => {
                if !row.get::<_, bool>(1) { return StatusCode::FORBIDDEN.into_response(); }
                row.get::<_, i64>(0)
            },
            Ok(None) => match tx.query_opt("SELECT id,active,google_sub,role,password_hash FROM users WHERE lower(email)=lower($1) FOR UPDATE", &[&email]).await {
                Ok(Some(row)) => {
                    if row.get::<_, Option<String>>(2).is_some() { return StatusCode::CONFLICT.into_response(); }
                    let role: String = row.get(3);
                    let password_hash: String = row.get(4);
                    if !row.get::<_, bool>(1) {
                        if role != "guest" || password_hash.is_empty() {
                            return Redirect::to("/login?google_error=unverified").into_response();
                        }
                        let id: i64 = row.get(0);
                        let no_password = format!("google-only:{}", uuid::Uuid::new_v4());
                        if tx.execute("UPDATE users SET active=TRUE,password_hash=$2,google_sub=$3,first_name=$4,last_name=$5 WHERE id=$1", &[&id,&no_password,&profile.sub,&profile.given_name,&profile.family_name]).await.is_err()
                            || tx.execute("DELETE FROM email_verifications WHERE user_id=$1", &[&id]).await.is_err() {
                            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                        }
                        id
                    } else {
                    if password_hash.is_empty() || password_hash.starts_with("google-only:") {
                        return Redirect::to("/login?google_setup=1").into_response();
                    }
                    if !may_auto_link(&role, &password_hash, &email) {
                        return Redirect::to("/login?next=%2Fauth%2Fgoogle%2Flink&google_link=1").into_response();
                    }
                    let id: i64 = row.get(0);
                    if tx.execute("UPDATE users SET google_sub=$1 WHERE id=$2", &[&profile.sub, &id]).await.is_err() {
                        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                    }
                    id
                    }
                },
                Ok(None) => {
                    let no_password = format!("google-only:{}", uuid::Uuid::new_v4());
                    match tx.query_one("INSERT INTO users(first_name,last_name,email,role,password_hash,google_sub) VALUES($1,$2,$3,'guest',$4,$5) RETURNING id",
                        &[&profile.given_name, &profile.family_name, &email, &no_password, &profile.sub]).await {
                        Ok(row) => row.get::<_, i64>(0), Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
                    }
                },
                Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
            },
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        };
        if tx.commit().await.is_err() {
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
        id
    };
    let backend = Backend::new(db);
    let Ok(Some(user)) = backend.user_by("id", &user_id).await else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    if auth.login(&user).await.is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    let destination =
        auth::oauth_return_path(next.as_deref()).unwrap_or(auth::home_for(&user.role));
    let mut response = Redirect::to(destination).into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_static(
            "google_oauth_state=; Path=/auth/google; HttpOnly; SameSite=None; Secure; Max-Age=0",
        ),
    );
    response
}
