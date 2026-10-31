use super::{auth, database_error, ApiResult, Database};
use axum::http::StatusCode;
use axum::{
    extract::{Path, State},
    http::{Method, Request},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use tokio_postgres::{Client, GenericClient};

/// Ownership is held by one landlord profile. Assigned property managers are
/// the staff collaborators for that property.
pub(crate) fn property_sql(alias: &str, user_parameter: usize) -> String {
    property_permission_sql(alias, user_parameter, false)
}

pub(crate) fn property_write_sql(alias: &str, user_parameter: usize) -> String {
    property_permission_sql(alias, user_parameter, true)
}

fn property_permission_sql(alias: &str, user_parameter: usize, _write: bool) -> String {
    format!("EXISTS (SELECT 1 FROM users scope_user WHERE scope_user.id=${user_parameter} AND scope_user.active AND (scope_user.role='admin' OR (scope_user.role='landlord' AND scope_user.landlord_id={alias}.landlord_id) OR (scope_user.role='staff' AND COALESCE({alias}.data->'manager_staff_ids','[]'::jsonb) @> to_jsonb(ARRAY[scope_user.id]::bigint[]))))")
}

pub(crate) async fn property_ids(client: &Client, user_id: i64) -> ApiResult<Vec<i64>> {
    let rows = client
        .query(
            &format!(
                "SELECT p.id FROM properties p WHERE {} ORDER BY p.id",
                property_sql("p", 1)
            ),
            &[&user_id],
        )
        .await
        .map_err(database_error)?;
    Ok(rows.into_iter().map(|row| row.get(0)).collect())
}

pub(crate) async fn require_property<C: GenericClient + Sync>(
    client: &C,
    user_id: i64,
    property_id: i64,
) -> ApiResult<()> {
    require_property_access(client, user_id, property_id, false).await
}

pub(crate) async fn require_property_write<C: GenericClient + Sync>(
    client: &C,
    user_id: i64,
    property_id: i64,
) -> ApiResult<()> {
    require_property_access(client, user_id, property_id, true).await
}

async fn require_property_access<C: GenericClient + Sync>(
    client: &C,
    user_id: i64,
    property_id: i64,
    write: bool,
) -> ApiResult<()> {
    let predicate = if write {
        property_write_sql("p", 2)
    } else {
        property_sql("p", 2)
    };
    let found = client
        .query_opt(
            &format!("SELECT 1 FROM properties p WHERE p.id=$1 AND {predicate}"),
            &[&property_id, &user_id],
        )
        .await
        .map_err(database_error)?;
    if found.is_none() {
        return Err((StatusCode::NOT_FOUND, "Property not found.".into()));
    }
    Ok(())
}

pub(crate) async fn can_write_any(client: &Client, user_id: i64) -> ApiResult<bool> {
    Ok(client
        .query_one(
            &format!(
                "SELECT EXISTS(SELECT 1 FROM properties p WHERE {})",
                property_write_sql("p", 1)
            ),
            &[&user_id],
        )
        .await
        .map_err(database_error)?
        .get(0))
}

#[derive(Serialize)]
pub(crate) struct PropertyAccess {
    owner_name: String,
    can_manage: bool,
}

pub(crate) async fn property_access(
    Path(property_id): Path<i64>,
    State(db): State<Database>,
    auth: auth::AuthSession,
) -> ApiResult<Json<PropertyAccess>> {
    let user = user(&auth)?;
    let client = db.lock().await;
    require_property(&*client, user.id, property_id).await?;
    let owner = client.query_one("SELECT p.landlord_id,COALESCE(l.legal_name,'') FROM properties p LEFT JOIN landlords l ON l.id=p.landlord_id WHERE p.id=$1", &[&property_id]).await.map_err(database_error)?;
    let owner_id: Option<i64> = owner.get(0);
    let can_manage = user.role == "admin"
        || (user.role == "landlord"
            && client
                .query_one("SELECT landlord_id FROM users WHERE id=$1", &[&user.id])
                .await
                .map_err(database_error)?
                .get::<_, Option<i64>>(0)
                == owner_id
            && owner_id.is_some());
    Ok(Json(PropertyAccess {
        owner_name: owner.get(1),
        can_manage,
    }))
}

pub(crate) fn user(auth: &auth::AuthSession) -> ApiResult<&auth::User> {
    auth.user
        .as_ref()
        .ok_or((StatusCode::UNAUTHORIZED, "Sign in to continue.".into()))
}

pub(crate) async fn require_entity(
    db: &Database,
    user_id: i64,
    table: &str,
    id: i64,
) -> ApiResult<()> {
    require_entity_access(db, user_id, table, id, false).await
}

pub(crate) async fn require_entity_write(
    db: &Database,
    user_id: i64,
    table: &str,
    id: i64,
) -> ApiResult<()> {
    require_entity_access(db, user_id, table, id, true).await
}

async fn require_entity_access(
    db: &Database,
    user_id: i64,
    table: &str,
    id: i64,
    write: bool,
) -> ApiResult<()> {
    let predicate = if write {
        property_write_sql("p", 2)
    } else {
        property_sql("p", 2)
    };
    let sql = match table {
        "tour_bookings" => format!("SELECT 1 FROM tour_bookings e JOIN properties p ON p.id=e.property_id WHERE e.id=$1 AND {predicate}"),
        "leases" => format!("SELECT 1 FROM leases e JOIN properties p ON p.id=e.property_id WHERE e.id=$1 AND {predicate}"),
        "lease_templates" => format!("SELECT 1 FROM lease_templates e WHERE e.id=$1 AND (EXISTS (SELECT 1 FROM users u WHERE u.id=$2 AND u.active AND (u.role='admin' OR (u.role='landlord' AND u.landlord_id=e.landlord_id))) OR EXISTS (SELECT 1 FROM properties p WHERE ((e.landlord_id IS NOT NULL AND p.landlord_id=e.landlord_id) OR (e.landlord_id IS NULL AND p.id=e.property_id)) AND {predicate}))"),
        "housework_tickets" => format!("SELECT 1 FROM housework_tickets e JOIN properties p ON p.id=e.property_id WHERE e.id=$1 AND {predicate}"),
        _ => return Err((StatusCode::INTERNAL_SERVER_ERROR, "Unknown scoped record.".into())),
    };
    if db
        .lock()
        .await
        .query_opt(&sql, &[&id, &user_id])
        .await
        .map_err(database_error)?
        .is_none()
    {
        return Err((StatusCode::NOT_FOUND, "Record not found.".into()));
    }
    Ok(())
}

pub(crate) async fn guard_panel_api(
    State(db): State<Database>,
    auth: auth::AuthSession,
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let Ok(user) = user(&auth) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if user.role == "admin" {
        return next.run(request).await;
    }
    let path = request.uri().path();
    let method = request.method();
    let parts: Vec<_> = path.trim_matches('/').split('/').collect();
    let target = match parts.as_slice() {
        ["api", "tours", "houses" | "admin-options"] => None,
        ["api", "properties", id] | ["api", "tours", "houses", id] => Some(("properties", *id)),
        ["api", "tours", id] | ["api", "tours", id, _] | ["api", "prospects", id, "lease"] => {
            Some(("tour_bookings", *id))
        }
        ["api", "leases", id, ..] => Some(("leases", *id)),
        ["api", "lease-templates", id, ..] => Some(("lease_templates", *id)),
        _ => None,
    };
    if let Some((table, raw_id)) = target {
        let Ok(id) = raw_id.parse::<i64>() else {
            return StatusCode::NOT_FOUND.into_response();
        };
        let write = method != Method::GET;
        let result =
            if table == "lease_templates" && id == 0 && parts.len() == 3 && method == Method::PUT {
                if can_write_any(&*db.lock().await, user.id)
                    .await
                    .unwrap_or(false)
                {
                    Ok(())
                } else {
                    Err((StatusCode::FORBIDDEN, "Editing access required.".into()))
                }
            } else if table == "properties" {
                scope_property(&db, user.id, id, write).await
            } else if write {
                require_entity_write(&db, user.id, table, id).await
            } else {
                require_entity(&db, user.id, table, id).await
            };
        if result.is_err() {
            return StatusCode::NOT_FOUND.into_response();
        }
    } else if method != Method::GET && matches!(path, "/api/leases" | "/api/images") {
        if !can_write_any(&*db.lock().await, user.id)
            .await
            .unwrap_or(false)
        {
            return StatusCode::FORBIDDEN.into_response();
        }
    } else if method != Method::GET {
        return StatusCode::FORBIDDEN.into_response();
    }
    next.run(request).await
}

async fn scope_property(db: &Database, user_id: i64, id: i64, write: bool) -> ApiResult<()> {
    if write {
        require_property_write(&*db.lock().await, user_id, id).await
    } else {
        require_property(&*db.lock().await, user_id, id).await
    }
}
