use axum::{
    extract::Form,
    http::{Request, StatusCode},
    middleware::Next,
    response::{Html, IntoResponse, Redirect, Response},
};
use axum_login::{AuthUser, AuthnBackend, UserId};
use serde::Deserialize;
use std::{io, sync::Arc};
use tokio::{sync::Mutex, task};
use tokio_postgres::Client;

pub type Database = Arc<Mutex<Client>>;
pub type AuthSession = axum_login::AuthSession<Backend>;

#[derive(Clone)]
pub struct Admin {
    id: i64,
    password_hash: String,
}

impl std::fmt::Debug for Admin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Admin").field("id", &self.id).finish()
    }
}

impl AuthUser for Admin {
    type Id = i64;

    fn id(&self) -> Self::Id {
        self.id
    }

    fn session_auth_hash(&self) -> &[u8] {
        self.password_hash.as_bytes()
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

    async fn admin(&self) -> Result<Option<Admin>, io::Error> {
        let client = self.db.lock().await;
        let row = client
            .query_opt(
                "SELECT id, password_hash FROM admin_users WHERE id = 1",
                &[],
            )
            .await
            .map_err(io::Error::other)?;
        Ok(row.map(|row| Admin {
            id: row.get(0),
            password_hash: row.get(1),
        }))
    }
}

#[derive(Deserialize)]
pub struct Credentials {
    password: String,
}

impl AuthnBackend for Backend {
    type User = Admin;
    type Credentials = Credentials;
    type Error = io::Error;

    async fn authenticate(&self, credentials: Credentials) -> Result<Option<Admin>, io::Error> {
        let admin = self.admin().await?;
        task::spawn_blocking(move || {
            admin.filter(|admin| {
                password_auth::verify_password(credentials.password, &admin.password_hash).is_ok()
            })
        })
        .await
        .map_err(io::Error::other)
    }

    async fn get_user(&self, id: &UserId<Self>) -> Result<Option<Admin>, io::Error> {
        if *id == 1 {
            self.admin().await
        } else {
            Ok(None)
        }
    }
}

pub async fn create_admin(db: &Database) -> Result<(), Box<dyn std::error::Error>> {
    println!("Set the admin password (12 characters minimum):");
    let password = rpassword::read_password()?;
    if password.chars().count() < 12 {
        return Err("Password must have at least 12 characters".into());
    }
    println!("Confirm the admin password:");
    let confirm = rpassword::read_password()?;
    if password != confirm {
        return Err("Passwords did not match".into());
    }
    let hash = task::spawn_blocking(move || password_auth::generate_hash(password)).await?;
    db.lock()
        .await
        .execute(
            "INSERT INTO admin_users (id, password_hash) VALUES (1, $1) \
         ON CONFLICT (id) DO UPDATE SET password_hash = EXCLUDED.password_hash",
            &[&hash],
        )
        .await?;
    println!("Admin password saved. Existing login sessions are invalidated.");
    Ok(())
}

fn login_html(error: bool) -> Html<String> {
    let error_message = if error {
        "<p class=error>Incorrect password. Try again.</p>"
    } else {
        ""
    };
    Html(format!(
        r#"<!doctype html><html lang="en"><head><meta name="viewport" content="width=device-width, initial-scale=1"><title>Sign in | Buildry</title><style>
    *{{box-sizing:border-box}}body{{margin:0;min-height:100svh;display:grid;place-items:center;background:#f5f7f5;color:#172821;font:16px system-ui,-apple-system,sans-serif;padding:16px}}main{{width:min(100%,390px);background:white;border:1px solid #dce5df;border-radius:20px;box-shadow:0 18px 55px #1b3b2715;padding:32px}}.brand{{font-weight:800;letter-spacing:.04em;color:#25694b;font-size:14px}}h1{{font-size:28px;margin:20px 0 26px}}label{{display:block;font-weight:650;margin-bottom:8px}}input{{width:100%;min-height:48px;padding:12px 14px;border:1px solid #aebfb3;border-radius:10px;font:inherit}}input:focus{{outline:3px solid #b9e5ce;border-color:#277550}}button{{width:100%;min-height:48px;background:#24734e;color:white;border:0;border-radius:10px;padding:12px 14px;font:inherit;font-weight:700;margin-top:18px;cursor:pointer}}.error{{color:#a42626;background:#fff0ef;border-radius:8px;padding:10px;margin:12px 0 0;line-height:1.4}}@media(max-width:480px){{main{{padding:26px 22px;border-radius:16px}}h1{{font-size:26px}}}}
    </style></head><body><main><div class="brand">BUILDRY</div><h1>Sign in</h1><form method="post" action="/login"><label for="password">Password</label><input id="password" name="password" type="password" autocomplete="current-password" required autofocus>{error_message}<button type="submit">Sign in</button></form></main></body></html>"#
    ))
}

pub async fn login_page(auth: AuthSession) -> Response {
    if auth.user.is_some() {
        Redirect::to("/").into_response()
    } else {
        login_html(false).into_response()
    }
}

pub async fn login(mut auth: AuthSession, Form(credentials): Form<Credentials>) -> Response {
    match auth.authenticate(credentials).await {
        Ok(Some(admin)) => match auth.login(&admin).await {
            Ok(()) => Redirect::to("/").into_response(),
            Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        },
        Ok(None) => (StatusCode::UNAUTHORIZED, login_html(true)).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub async fn logout(mut auth: AuthSession) -> Response {
    match auth.logout().await {
        Ok(_) => Redirect::to("/login").into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub async fn require_api(
    auth: AuthSession,
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    if auth.user.is_none() {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if auth
        .session
        .insert(
            "last_seen",
            time::OffsetDateTime::now_utc().unix_timestamp(),
        )
        .await
        .is_err()
    {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    next.run(request).await
}

pub async fn require_page(
    auth: AuthSession,
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    if auth.user.is_none() {
        return Redirect::to(if request.uri().path() == "/" {
            "/stays"
        } else {
            "/login"
        })
        .into_response();
    }
    if auth
        .session
        .insert(
            "last_seen",
            time::OffsetDateTime::now_utc().unix_timestamp(),
        )
        .await
        .is_err()
    {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    next.run(request).await
}
