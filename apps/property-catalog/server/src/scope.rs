use super::{auth, database_error, ApiResult, Database};
use axum::{
    extract::{Path, State},
    http::{Method, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Value};
use tokio_postgres::{Client, GenericClient};

pub(crate) fn capability_sql(alias: &str, user_parameter: usize, cap: &str) -> String {
    format!("buildry_property_can(${user_parameter},{alias}.id,'{cap}')")
}
pub(crate) async fn effective_capabilities<C:GenericClient+Sync>(client:&C,user:i64,id:i64)->ApiResult<Vec<String>>{
 let caps:Vec<String>=crate::accounts::FUNCTIONS.iter().flat_map(|f|[format!("{f}.read"),format!("{f}.write")]).chain(std::iter::once("leases.approve".to_string())).collect();
 Ok(client.query_one("SELECT ARRAY(SELECT cap FROM unnest($3::text[]) AS c(cap) WHERE buildry_property_can($1,$2,cap))",&[&user,&id,&caps]).await.map_err(database_error)?.get(0))
}
pub(crate) async fn ids_for(client: &Client, user_id: i64, cap: &str) -> ApiResult<Vec<i64>> {
    Ok(client
        .query(
            "SELECT id FROM properties WHERE buildry_property_can($1,id,$2) ORDER BY id",
            &[&user_id, &cap],
        )
        .await
        .map_err(database_error)?
        .into_iter()
        .map(|r| r.get(0))
        .collect())
}
pub(crate) async fn require_cap<C: GenericClient + Sync>(
    client: &C,
    user: i64,
    property: i64,
    cap: &str,
) -> ApiResult<()> {
    if client
        .query_opt(
            "SELECT 1 FROM properties WHERE id=$1 AND buildry_property_can($2,id,$3)",
            &[&property, &user, &cap],
        )
        .await
        .map_err(database_error)?
        .is_none()
    {
        return Err((StatusCode::NOT_FOUND, "Property not found.".into()));
    }
    Ok(())
}
pub(crate) async fn require_property<C: GenericClient + Sync>(
    client: &C,
    user: i64,
    property: i64,
) -> ApiResult<()> {
    require_cap(client, user, property, "details.read").await
}
pub(crate) async fn require_property_write<C: GenericClient + Sync>(
    client: &C,
    user: i64,
    property: i64,
) -> ApiResult<()> {
    require_cap(client, user, property, "details.write").await
}
pub(crate) fn user(auth: &auth::AuthSession) -> ApiResult<&auth::User> {
    auth.user
        .as_ref()
        .ok_or((StatusCode::UNAUTHORIZED, "Sign in to continue.".into()))
}
pub(crate) async fn property_access(
    Path(id): Path<i64>,
    State(db): State<Database>,
    auth: auth::AuthSession,
) -> ApiResult<Json<Value>> {
    let user = user(&auth)?;
    let client = db.lock().await;
    require_property(&*client, user.id, id).await?;
    let row=client.query_one("SELECT COALESCE(l.legal_name,''),buildry_property_owner($2,p.id) FROM properties p LEFT JOIN landlords l ON l.id=p.landlord_id WHERE p.id=$1",&[&id,&user.id]).await.map_err(database_error)?;
    let mut capabilities = Vec::new();
    for f in crate::accounts::FUNCTIONS {
        for a in ["read", "write"] {
            let cap = format!("{f}.{a}");
            if client
                .query_one(
                    "SELECT buildry_property_can($1,$2,$3)",
                    &[&user.id, &id, &cap],
                )
                .await
                .map_err(database_error)?
                .get::<_, bool>(0)
            {
                capabilities.push(cap);
            }
        }
    }
    Ok(Json(
        json!({"owner_name":row.get::<_,String>(0),"can_manage":row.get::<_,bool>(1),"capabilities":capabilities}),
    ))
}
pub(crate) async fn require_entity(
    db: &Database,
    user: i64,
    table: &str,
    id: i64,
) -> ApiResult<()> {
    require_entity_access(db, user, table, id, false).await
}
pub(crate) async fn require_entity_write(
    db: &Database,
    user: i64,
    table: &str,
    id: i64,
) -> ApiResult<()> {
    require_entity_access(db, user, table, id, true).await
}
async fn require_entity_access(
    db: &Database,
    user: i64,
    table: &str,
    id: i64,
    write: bool,
) -> ApiResult<()> {
    let (sql,cap)=match table{
 "tour_bookings"=>("SELECT 1 FROM tour_bookings WHERE id=$1 AND buildry_property_can($2,property_id,$3)",if write{"tours.write"}else{"tours.read"}),
 "leases"=>("SELECT 1 FROM leases WHERE id=$1 AND buildry_property_can($2,property_id,$3)",if write{"leases.write"}else{"leases.read"}),
 "lease_templates"=>("SELECT 1 FROM lease_templates t WHERE t.id=$1 AND (EXISTS(SELECT 1 FROM users WHERE id=$2 AND active AND role='admin') OR EXISTS(SELECT 1 FROM resource_owners o WHERE o.resource_type='landlord' AND o.resource_id=t.landlord_id AND o.user_id=$2) OR EXISTS(SELECT 1 FROM properties p WHERE ((t.landlord_id IS NOT NULL AND p.landlord_id=t.landlord_id) OR p.id=t.property_id) AND buildry_property_can($2,p.id,$3)))",if write{"leases.write"}else{"leases.read"}),
 "housework_tickets"=>("SELECT 1 FROM housework_tickets WHERE id=$1 AND buildry_task_can($2,id,$3::boolean)",""),
 _=>return Err((StatusCode::INTERNAL_SERVER_ERROR,"Unknown resource.".into()))};
    let client = db.lock().await;
    let found = if table == "housework_tickets" {
        client.query_opt(sql, &[&id, &user, &write]).await
    } else {
        client.query_opt(sql, &[&id, &user, &cap]).await
    }
    .map_err(database_error)?;
    if found.is_none() {
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
    let path = request.uri().path();
    let write = request.method() != Method::GET;
    let parts: Vec<_> = path.trim_matches('/').split('/').collect();
    let result = match parts.as_slice() {
        ["api", "properties", "bootstrap"] => {
            if user.role == "admin" {
                Ok(())
            } else {
                Err((StatusCode::FORBIDDEN, "".into()))
            }
        }

        ["api", "properties", id] => {
            if let Ok(id) = id.parse::<i64>() {
                if !write {
                    require_property(&*db.lock().await, user.id, id).await
                } else {
                    Ok(())
                }
            } else {
                Err((StatusCode::NOT_FOUND, "".into()))
            }
        } // save checks existing access and assigns new ownership transactionally
        ["api", "tours", "houses" | "admin-options"] => Ok(()),
        ["api", "tours", "houses", id] => match id.parse::<i64>() {
            Ok(id) => {
                require_cap(
                    &*db.lock().await,
                    user.id,
                    id,
                    if write { "tours.write" } else { "tours.read" },
                )
                .await
            }
            Err(_) => Err((StatusCode::NOT_FOUND, "".into())),
        },
        ["api", "tours", id, ..] | ["api", "prospects", id, "lease"] => match id.parse::<i64>() {
            Ok(id) => require_entity_access(&db, user.id, "tour_bookings", id, write).await,
            Err(_) => Err((StatusCode::NOT_FOUND, "".into())),
        },
        ["api", "leases", id, rest @ ..] => match id.parse::<i64>() {
            Ok(id) => {
                let cap = if rest.first() == Some(&"confirm") || rest.first() == Some(&"photo-id") {
                    "leases.approve"
                } else if rest
                    .first()
                    .is_some_and(|s| ["payments", "payment-history"].contains(s))
                {
                    if write {
                        "finances.write"
                    } else {
                        "finances.read"
                    }
                } else if write {
                    "leases.write"
                } else {
                    "leases.read"
                };
                let client = db.lock().await;
                match client.query_opt("SELECT 1 FROM leases WHERE id=$1 AND buildry_property_can($2,property_id,$3)",&[&id,&user.id,&cap]).await{Ok(Some(_))=>Ok(()),Ok(None)=>Err((StatusCode::NOT_FOUND,"Record not found.".into())),Err(e)=>Err(database_error(e))}
            }
            Err(_) => Err((StatusCode::NOT_FOUND, "".into())),
        },
        ["api", "lease-templates", id, ..] => match id.parse::<i64>() {
            Ok(0) if write => Ok(()),
            Ok(id) => require_entity_access(&db, user.id, "lease_templates", id, write).await,
            Err(_) => Err((StatusCode::NOT_FOUND, "".into())),
        },
        // Collection handlers authorize their payloads and scope their queries.
        ["api", "properties" | "leases" | "lease-templates" | "landlords" | "images"] => Ok(()),
        ["api", "landlords", _] => Ok(()),
        _ if !write => Ok(()),
        _ => Err((StatusCode::FORBIDDEN, "Permission required.".into())),
    };
    match result {
        Ok(()) => next.run(request).await,
        Err(e) => e.into_response(),
    }
}

/// Token URLs locate an invitation; authentication and the named party still control access.
pub(crate) async fn guard_invitation(
    State(db): State<Database>,
    auth: auth::AuthSession,
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let path = request.uri().path();
    let parts: Vec<_> = path.trim_matches('/').split('/').collect();
    let Some(raw) = parts.get(1) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(actor) = auth.user.as_ref() else {
        return axum::response::Redirect::to(&format!(
            "/login?next={}",
            urlencoding_path(
                request
                    .uri()
                    .path_and_query()
                    .map(|v| v.as_str())
                    .unwrap_or(path)
            )
        ))
        .into_response();
    };
    use sha2::{Digest, Sha256};
    let hash = format!("{:x}", Sha256::digest(raw.as_bytes()));
    let (table, column, email_path) = if parts[0] == "lease-landlord" {
        (
            "leases",
            "landlord_token_hash",
            "landlord_snapshot #>> '{landlord,email}'",
        )
    } else if parts[0] == "lease-template" {
        (
            "lease_templates",
            "landlord_token_hash",
            "landlord_snapshot #>> '{landlord,email}'",
        )
    } else {
        ("leases", "token_hash", "tenant_email")
    };
    let identity = if table == "leases" {
        let bound = if parts[0] == "lease-landlord" {
            "signer_user_id"
        } else {
            "tenant_user_id"
        };
        format!("({bound}=$3 OR ({bound} IS NULL AND lower({email_path})=lower($2)))")
    } else {
        format!("lower({email_path})=lower($2) AND $3::bigint>0")
    };
    let sql = format!("SELECT id FROM {table} WHERE {column}=$1 AND {identity}");
    let client = db.lock().await;
    match client
        .query_opt(&sql, &[&hash, &actor.email, &actor.id])
        .await
    {
        Ok(Some(row)) => {
            if table == "leases" {
                let id: i64 = row.get(0);
                let column = if parts[0] == "lease-landlord" {
                    "signer_user_id"
                } else {
                    "tenant_user_id"
                };
                if let Err(e)=client.execute(&format!("UPDATE leases SET {column}=$2 WHERE id=$1 AND ({column} IS NULL OR {column}=$2)"),&[&id,&actor.id]).await{return database_error(e).into_response();}
            }
            drop(client);
            next.run(request).await
        }
        Ok(None) => (
            StatusCode::FORBIDDEN,
            "Sign in with the account invited to this lease.",
        )
            .into_response(),
        Err(e) => database_error(e).into_response(),
    }
}
fn urlencoding_path(path: &str) -> String {
    path.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
