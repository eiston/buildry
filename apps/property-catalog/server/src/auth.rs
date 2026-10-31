use axum::{
    extract::{Form, State},
    http::{Request, StatusCode},
    middleware::Next,
    response::{Html, IntoResponse, Redirect, Response},
    Json,
};
use axum_login::{AuthUser, AuthnBackend, UserId};
use serde::{Deserialize, Serialize};
use std::{io, sync::Arc};
use tokio::{sync::Mutex, task};
use tokio_postgres::Client;

pub type Database = Arc<Mutex<Client>>;
pub type AuthSession = axum_login::AuthSession<Backend>;

#[derive(Clone)]
pub struct User {
    pub id: i64,
    pub email: String,
    pub role: String,
    password_hash: String,
    session_hash: String,
}

impl std::fmt::Debug for User {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("User")
            .field("id", &self.id)
            .field("role", &self.role)
            .finish()
    }
}

impl AuthUser for User {
    type Id = i64;
    fn id(&self) -> Self::Id {
        self.id
    }
    fn session_auth_hash(&self) -> &[u8] {
        self.session_hash.as_bytes()
    }
}

#[derive(Clone)]
pub struct Backend {
    db: Database,
}

impl Backend {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub(crate) async fn user_by(
        &self,
        column: &str,
        value: &(dyn tokio_postgres::types::ToSql + Sync),
    ) -> Result<Option<User>, io::Error> {
        let client = self.db.lock().await;
        let sql = if column == "email" {
            "SELECT id, email, role, password_hash, auth_version FROM users WHERE lower(email) = lower($1) AND active"
        } else {
            "SELECT id, email, role, password_hash, auth_version FROM users WHERE id = $1 AND active"
        };
        let row = client
            .query_opt(sql, &[value])
            .await
            .map_err(io::Error::other)?;
        Ok(row.map(|row| {
            let password_hash: String = row.get(3);
            let auth_version: i64 = row.get(4);
            let session_hash = if auth_version == 0 {
                password_hash.clone()
            } else {
                format!("{password_hash}:{auth_version}")
            };
            User {
                id: row.get(0),
                email: row.get(1),
                role: row.get(2),
                password_hash,
                session_hash,
            }
        }))
    }
}

#[derive(Deserialize)]
pub struct Credentials {
    pub(crate) email: String,
    pub(crate) password: String,
    #[serde(default)]
    next: Option<String>,
}

impl AuthnBackend for Backend {
    type User = User;
    type Credentials = Credentials;
    type Error = io::Error;
    async fn authenticate(&self, credentials: Credentials) -> Result<Option<User>, io::Error> {
        let user = self.user_by("email", &credentials.email.trim()).await?;
        task::spawn_blocking(move || {
            user.filter(|user| {
                password_auth::verify_password(credentials.password, &user.password_hash).is_ok()
            })
        })
        .await
        .map_err(io::Error::other)
    }
    async fn get_user(&self, id: &UserId<Self>) -> Result<Option<User>, io::Error> {
        self.user_by("id", id).await
    }
}

pub async fn create_admin(db: &Database) -> Result<(), Box<dyn std::error::Error>> {
    println!("Admin email:");
    let mut email = String::new();
    std::io::stdin().read_line(&mut email)?;
    let email = email.trim().to_lowercase();
    if !email.contains('@') {
        return Err("Enter a valid email".into());
    }
    println!("Set the admin password (12 characters minimum):");
    let password = rpassword::read_password()?;
    if password.chars().count() < 12 {
        return Err("Password must have at least 12 characters".into());
    }
    println!("Confirm the admin password:");
    if password != rpassword::read_password()? {
        return Err("Passwords did not match".into());
    }
    let hash = task::spawn_blocking(move || password_auth::generate_hash(password)).await?;
    let mut client = db.lock().await;
    let transaction = client.transaction().await?;
    transaction.execute(
        "INSERT INTO users (first_name, email, role, password_hash) VALUES ('Administrator', $1, 'admin', $2) \
         ON CONFLICT (email) DO UPDATE SET role = 'admin', password_hash = EXCLUDED.password_hash, active = TRUE",
        &[&email, &hash],
    ).await?;
    transaction
        .execute(
            "UPDATE users SET active = FALSE WHERE role = 'admin' AND email <> $1",
            &[&email],
        )
        .await?;
    transaction.commit().await?;
    println!("Admin account saved. Other admin accounts were disabled.");
    Ok(())
}

pub(crate) fn oauth_return_path(next: Option<&str>) -> Option<&str> {
    let next = next?;
    let request = next.strip_prefix("/oauth/authorize?request=")?;
    (request.len() == 64 && request.bytes().all(|byte| byte.is_ascii_hexdigit())).then_some(next)
}

fn login_return_path(next: Option<&str>) -> Option<&str> {
    if next == Some("/auth/google/link") {
        next
    } else {
        oauth_return_path(next)
    }
}

#[cfg(test)]
mod oauth_return_tests {
    use super::{login_return_path, oauth_return_path};

    #[test]
    fn login_only_returns_to_a_pending_oauth_request() {
        let valid = format!("/oauth/authorize?request={}", "a".repeat(64));
        assert_eq!(oauth_return_path(Some(&valid)), Some(valid.as_str()));
        assert_eq!(oauth_return_path(Some("https://example.com")), None);
        assert_eq!(
            oauth_return_path(Some("/oauth/authorize?request=bad")),
            None
        );
        assert_eq!(
            oauth_return_path(Some(
                "/oauth/authorize?request=aaaaaaaa&redirect=https://example.com"
            )),
            None
        );
    }

    #[test]
    fn login_can_return_to_google_link_without_open_redirects() {
        assert_eq!(
            login_return_path(Some("/auth/google/link")),
            Some("/auth/google/link")
        );
        assert_eq!(login_return_path(Some("//evil.example")), None);
        assert_eq!(login_return_path(Some("/auth/google/link?other=1")), None);
    }
}

fn login_html(
    error: bool,
    next: Option<&str>,
    verified: bool,
    google_link: bool,
    google_setup: bool,
    password_set: bool,
    google_error: Option<&str>,
) -> Html<String> {
    let error_message = if error {
        "<p class=\"error\" role=\"alert\">We couldn't sign you in. Check your details, or <a href=\"/resend-verification\">resend verification</a> if your account is new.</p>"
    } else {
        ""
    };
    let return_field = login_return_path(next)
        .map(|path| {
            format!(
                "<input type=\"hidden\" name=\"next\" value=\"{}\">",
                html_escape(path)
            )
        })
        .unwrap_or_default();
    let intro = if next == Some("/auth/google/link") {
        "Sign in with your password to connect your Google account."
    } else if oauth_return_path(next).is_some() {
        "Sign in to review a request to connect to Buildry."
    } else {
        "Welcome back. Enter your details to continue."
    };
    let status_message = if verified {
        "<p class=\"notice\" role=\"status\">Your email is verified. You can sign in now.</p>"
    } else if password_set {
        "<p class=\"notice\" role=\"status\">Your password is set. You can sign in now.</p>"
    } else if google_link {
        "<p class=\"notice\" role=\"status\">This email already has a Buildry account. Sign in with its password to connect Google.</p>"
    } else if google_setup {
        "<p class=\"notice\" role=\"status\">This account needs a password setup email before Google can be connected. Ask your Buildry admin to send one from Access, or request a link below.</p>"
    } else if google_error == Some("expired") {
        "<p class=\"error\" role=\"alert\">Google sign-in expired. Please try again.</p>"
    } else if google_error == Some("cancelled") {
        "<p class=\"notice\" role=\"status\">Google sign-in was cancelled.</p>"
    } else if google_error == Some("unverified") {
        "<p class=\"notice\" role=\"status\">This account is inactive. Ask your Buildry admin to activate it before signing in.</p>"
    } else if google_error.is_some() {
        "<p class=\"error\" role=\"alert\">Google sign-in could not be completed. Please try again.</p>"
    } else {
        ""
    };
    Html(
        include_str!("login.html")
            .replace("<!-- error_message -->", error_message)
            .replace("<!-- oauth_return -->", &return_field)
            .replace("<!-- status_message -->", status_message)
            .replace(
                "<!-- google_signin -->",
                &crate::google_auth::signin_html(next),
            )
            .replace("Welcome back. Enter your details to continue.", intro),
    )
}

#[derive(Deserialize)]
pub struct LoginQuery {
    next: Option<String>,
    verified: Option<String>,
    google_link: Option<String>,
    google_setup: Option<String>,
    password_set: Option<String>,
    google_error: Option<String>,
}

pub async fn login_page(
    auth: AuthSession,
    axum::extract::Query(query): axum::extract::Query<LoginQuery>,
) -> Response {
    if let Some(user) = auth.user {
        Redirect::to(login_return_path(query.next.as_deref()).unwrap_or(home_for(&user.role)))
            .into_response()
    } else {
        login_html(
            false,
            query.next.as_deref(),
            query.verified.as_deref() == Some("1"),
            query.google_link.as_deref() == Some("1"),
            query.google_setup.as_deref() == Some("1"),
            query.password_set.as_deref() == Some("1"),
            query.google_error.as_deref(),
        )
        .into_response()
    }
}

pub async fn login(mut auth: AuthSession, Form(credentials): Form<Credentials>) -> Response {
    let next = credentials.next.clone();
    match auth.authenticate(credentials).await {
        Ok(Some(user)) => match auth.login(&user).await {
            Ok(()) => {
                Redirect::to(login_return_path(next.as_deref()).unwrap_or(home_for(&user.role)))
                    .into_response()
            }
            Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        },
        Ok(None) => (
            StatusCode::UNAUTHORIZED,
            login_html(true, next.as_deref(), false, false, false, false, None),
        )
            .into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(crate) fn home_for(role: &str) -> &'static str {
    match role {
        "admin" | "staff" | "landlord" => "/",
        "tenant" => "/my-home",
        _ => "/guest-home",
    }
}

#[derive(Deserialize)]
pub struct SignupInput {
    first_name: String,
    last_name: String,
    email: String,
    password: String,
    confirm_password: String,
}

fn signup_html(error: Option<&str>) -> Html<String> {
    let error = error
        .map(|message| {
            format!(
                "<p class=\"error\" role=\"alert\">{}</p>",
                html_escape(message)
            )
        })
        .unwrap_or_default();
    Html(
        include_str!("signup.html")
            .replace("<!-- error_message -->", &error)
            .replace(
                "<!-- google_signin -->",
                &crate::google_auth::signin_html(None),
            ),
    )
}

fn signup_result_html(title: &str, message: &str) -> Html<String> {
    Html(format!(
        "<!doctype html><html lang=\"en\"><head><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>{} | Buildry</title><link rel=\"stylesheet\" href=\"/assets/main.css\"></head><body class=\"auth-body\"><main class=\"auth-card\"><h1>{}</h1><p>{}</p><p><a href=\"/resend-verification\">Resend verification email</a></p><p><a href=\"/login\">Sign in</a> · <a href=\"/stays\">Explore stays</a></p></main></body></html>",
        html_escape(title),
        html_escape(title),
        html_escape(message)
    ))
}

pub async fn signup_page(auth: AuthSession) -> Response {
    if let Some(user) = auth.user {
        return Redirect::to(home_for(&user.role)).into_response();
    }
    if std::env::var("RESEND_API_KEY").is_err()
        || std::env::var("RESEND_FROM_EMAIL").is_err()
        || std::env::var("APP_PUBLIC_URL").is_err()
    {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            signup_result_html(
                "Signup unavailable",
                "Email verification is being configured. Please check back soon.",
            ),
        )
            .into_response();
    }
    signup_html(None).into_response()
}

pub async fn signup(
    auth: AuthSession,
    State(db): State<Database>,
    Form(input): Form<SignupInput>,
) -> Response {
    if let Some(user) = auth.user {
        return Redirect::to(home_for(&user.role)).into_response();
    }
    if std::env::var("RESEND_API_KEY").is_err()
        || std::env::var("RESEND_FROM_EMAIL").is_err()
        || std::env::var("APP_PUBLIC_URL").is_err()
    {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            signup_html(Some(
                "Email verification is temporarily unavailable. Please try again later.",
            )),
        )
            .into_response();
    }
    let email = input.email.trim().to_lowercase();
    if input.password != input.confirm_password {
        return (
            StatusCode::BAD_REQUEST,
            signup_html(Some("Passwords do not match.")),
        )
            .into_response();
    }
    if input.first_name.trim().is_empty()
        || input.last_name.trim().is_empty()
        || input.first_name.len() > 60
        || input.last_name.len() > 60
        || email.len() > 254
        || !email.contains('@')
        || input.password.chars().count() < 12
    {
        return (StatusCode::BAD_REQUEST, signup_html(Some("Enter your first and last name, a valid email, and a password of at least 12 characters."))).into_response();
    }
    let hash =
        match task::spawn_blocking(move || password_auth::generate_hash(input.password)).await {
            Ok(hash) => hash,
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        };
    let row = db.lock().await.query_one(
        "INSERT INTO users (first_name, last_name, email, role, password_hash, active) VALUES ($1, $2, $3, 'guest', $4, FALSE) RETURNING id",
        &[&input.first_name.trim(), &input.last_name.trim(), &email, &hash]
    ).await;
    match row {
        Ok(row) => {
            let id: i64 = row.get(0);
            match crate::leasing::send_verification(&db,id,&email,&input.first_name.trim()).await {
                Ok(())=>signup_result_html("Check your email", "Open the verification link we sent to finish creating your account. The link expires in 24 hours.").into_response(),
                Err((code,message))=>{let _=db.lock().await.execute("DELETE FROM users WHERE id=$1 AND role='guest' AND active=FALSE", &[&id]).await;(code,signup_html(Some(&message))).into_response()}
            }
        }
        Err(error) if error.code() == Some(&tokio_postgres::error::SqlState::UNIQUE_VIOLATION) => (
            StatusCode::CONFLICT,
            signup_html(Some(
                "An account with this email already exists. Sign in instead.",
            )),
        )
            .into_response(),
        Err(error) => {
            eprintln!("signup failed: {error}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct ResendVerificationInput {
    email: String,
}

pub async fn resend_verification_page() -> Html<String> {
    Html("<!doctype html><html lang=\"en\"><head><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>Resend verification | Buildry</title><link rel=\"stylesheet\" href=\"/assets/main.css\"></head><body class=\"auth-body\"><main class=\"auth-card\"><h1>Resend verification email</h1><p>Enter the email you used to create your account. You can request a new link every two minutes.</p><form method=\"post\" action=\"/resend-verification\"><label>Email<input name=\"email\" type=\"email\" autocomplete=\"email\" required></label><button type=\"submit\">Send verification email</button></form><p><a href=\"/login\">Back to sign in</a></p></main></body></html>".to_string())
}

pub async fn resend_verification(
    State(db): State<Database>,
    Form(input): Form<ResendVerificationInput>,
) -> Response {
    let email = input.email.trim().to_lowercase();
    if email.len() > 254 || !email.contains('@') {
        return (StatusCode::BAD_REQUEST, resend_verification_page().await).into_response();
    }
    let row = db.lock().await.query_opt(
        "UPDATE email_verifications v SET sent_at=NOW() FROM users u WHERE v.user_id=u.id AND lower(u.email)=lower($1) AND u.role='guest' AND NOT u.active AND v.used_at IS NULL AND v.sent_at < NOW()-INTERVAL '2 minutes' RETURNING u.id,u.first_name",
        &[&email],
    ).await;
    match row {
        Ok(Some(row)) => {
            let id: i64 = row.get(0);
            let name: String = row.get(1);
            if let Err((_, message)) =
                crate::leasing::send_verification(&db, id, &email, &name).await
            {
                eprintln!("resend verification: {message}");
            }
        }
        Ok(None) => {}
        Err(error) => {
            eprintln!("resend verification lookup: {error}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    }
    signup_result_html("Check your email", "If an unverified account exists for that address, a new verification link will arrive shortly. Check your spam folder too.").into_response()
}

pub async fn forgot_password_page() -> Html<String> {
    Html("<!doctype html><html lang=\"en\"><head><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>Reset password | Buildry</title><link rel=\"stylesheet\" href=\"/assets/main.css\"></head><body class=\"auth-body\"><main class=\"auth-card\"><h1>Reset your password</h1><p>Enter the email on your Buildry account. We’ll send a reset link if the account is active.</p><form method=\"post\" action=\"/forgot-password\"><label>Email<input name=\"email\" type=\"email\" autocomplete=\"email\" required></label><button type=\"submit\">Send reset link</button></form><p><a href=\"/login\">Back to sign in</a></p></main></body></html>".to_string())
}

pub async fn forgot_password(
    State(db): State<Database>,
    Form(input): Form<ResendVerificationInput>,
) -> Response {
    let email = input.email.trim().to_lowercase();
    if email.len() > 254 || !email.contains('@') {
        return (StatusCode::BAD_REQUEST, forgot_password_page().await).into_response();
    }
    if let Err((_, message)) = crate::leasing::send_password_reset(&db, &email).await {
        eprintln!("password reset request: {message}");
    }
    Html("<!doctype html><html lang=\"en\"><head><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>Check your email | Buildry</title><link rel=\"stylesheet\" href=\"/assets/main.css\"></head><body class=\"auth-body\"><main class=\"auth-card\"><h1>Check your email</h1><p>If an active account exists for that address, a password reset link will arrive shortly. Check your spam folder too.</p><a href=\"/login\">Back to sign in</a></main></body></html>".to_string()).into_response()
}

pub async fn guest_home(auth: AuthSession) -> Response {
    match auth.user {
        Some(user) if user.role == "guest" => Html(format!(r#"<!doctype html><html lang="en"><head><meta name="viewport" content="width=device-width, initial-scale=1"><title>My account | Buildry</title><link rel="stylesheet" href="/assets/main.css"></head><body class="auth-body"><main class="auth-card"><h1>Welcome, {}</h1><p>Your account is ready. If we send you a lease, use the private link in that email to review and sign it.</p><a href="/stays">Explore stays</a><form method="post" action="/logout"><button type="submit">Sign out</button></form></main></body></html>"#, html_escape(&user.email))).into_response(),
        Some(user) => Redirect::to(home_for(&user.role)).into_response(),
        None => Redirect::to("/login").into_response(),
    }
}

pub async fn landlord_home(auth: AuthSession) -> Response {
    match auth.user {
        None => Redirect::to("/login").into_response(),
        Some(user) => Redirect::to(home_for(&user.role)).into_response(),
    }
}

pub fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

pub async fn logout(mut auth: AuthSession) -> Response {
    match auth.logout().await {
        Ok(_) => Redirect::to("/login").into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

#[derive(Serialize)]
pub struct Me {
    id: i64,
    email: String,
    role: String,
}
pub async fn me(auth: AuthSession) -> Result<Json<Me>, StatusCode> {
    let user = auth.user.ok_or(StatusCode::UNAUTHORIZED)?;
    Ok(Json(Me {
        id: user.id,
        email: user.email,
        role: user.role,
    }))
}

#[derive(Deserialize)]
pub struct PasswordChange {
    current_password: String,
    new_password: String,
}

pub async fn change_password(
    auth: AuthSession,
    axum::extract::State(db): axum::extract::State<Database>,
    Json(input): Json<PasswordChange>,
) -> Result<StatusCode, (StatusCode, String)> {
    let user = auth
        .user
        .ok_or((StatusCode::UNAUTHORIZED, "Sign in required.".into()))?;
    if input.new_password.chars().count() < 12 {
        return Err((
            StatusCode::BAD_REQUEST,
            "New password must have at least 12 characters.".into(),
        ));
    }
    let mut client = db.lock().await;
    let transaction = client.transaction().await.map_err(crate::database_error)?;
    let row = transaction
        .query_one(
            "SELECT password_hash FROM users WHERE id = $1 FOR UPDATE",
            &[&user.id],
        )
        .await
        .map_err(crate::database_error)?;
    let current_hash: String = row.get(0);
    let verified = task::spawn_blocking(move || {
        password_auth::verify_password(input.current_password, &current_hash).is_ok()
    })
    .await
    .map_err(crate::database_error)?;
    if !verified {
        return Err((
            StatusCode::BAD_REQUEST,
            "Current password is incorrect.".into(),
        ));
    }
    let new_hash = task::spawn_blocking(move || password_auth::generate_hash(input.new_password))
        .await
        .map_err(crate::database_error)?;
    transaction
        .execute(
            "UPDATE users SET password_hash = $2 WHERE id = $1",
            &[&user.id, &new_hash],
        )
        .await
        .map_err(crate::database_error)?;
    transaction.commit().await.map_err(crate::database_error)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn require_admin_api(
    auth: AuthSession,
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let Some(user) = auth.user else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if !matches!(user.role.as_str(), "admin" | "staff" | "landlord") {
        return StatusCode::FORBIDDEN.into_response();
    }
    next.run(request).await
}

pub async fn require_task_api(
    auth: AuthSession,
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    match auth.user {
        None => StatusCode::UNAUTHORIZED.into_response(),
        Some(user) if matches!(user.role.as_str(), "admin" | "staff" | "landlord") => {
            next.run(request).await
        }
        Some(_) => StatusCode::FORBIDDEN.into_response(),
    }
}

pub async fn require_task_page(
    auth: AuthSession,
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    match auth.user {
        None => Redirect::to("/login").into_response(),
        Some(user) if matches!(user.role.as_str(), "admin" | "staff" | "landlord") => {
            next.run(request).await
        }
        Some(user) => Redirect::to(home_for(&user.role)).into_response(),
    }
}

pub async fn require_admin_page(
    auth: AuthSession,
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    match auth.user {
        None => Redirect::to(if request.uri().path() == "/" {
            "/stays"
        } else {
            "/login"
        })
        .into_response(),
        Some(user) if !matches!(user.role.as_str(), "admin" | "staff" | "landlord") => {
            Redirect::to(home_for(&user.role)).into_response()
        }
        Some(_) => next.run(request).await,
    }
}

pub async fn require_access_api(
    auth: AuthSession,
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    match auth.user {
        None => StatusCode::UNAUTHORIZED.into_response(),
        Some(user) if user.role != "admin" => StatusCode::FORBIDDEN.into_response(),
        Some(_) => next.run(request).await,
    }
}
pub async fn require_access_page(
    auth: AuthSession,
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    match auth.user {
        None => Redirect::to("/login").into_response(),
        Some(user) if user.role != "admin" => Redirect::to(home_for(&user.role)).into_response(),
        Some(_) => next.run(request).await,
    }
}

pub async fn require_tenant_page(
    auth: AuthSession,
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    match auth.user {
        None => Redirect::to("/login").into_response(),
        Some(user) if user.role != "tenant" => Redirect::to("/").into_response(),
        Some(_) => next.run(request).await,
    }
}
