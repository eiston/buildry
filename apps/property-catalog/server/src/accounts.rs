use crate::{auth, database_error, scope, ApiResult, Database};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

pub const APPS: &[(&str, &str, &str)] = &[
    ("properties", "Properties", "/properties"),
    ("pricing", "Pricing", "/pricing"),
    ("vacancy", "Vacancy", "/vacancy"),
    ("calendar", "Calendar", "/calendar"),
    ("tasks", "Tasks", "/tasks"),
    ("leases", "Lease Management", "/leases"),
    ("rentals", "My Rentals", "/my-rentals"),
    ("payments", "Payments", "/payments"),
];
pub const FUNCTIONS: &[&str] = &["details", "pricing", "tours", "tasks", "leases", "finances"];

pub async fn migrate(client: &tokio_postgres::Client) -> Result<(), tokio_postgres::Error> {
    client
        .batch_execute(include_str!("../migrations/001_resource_access.sql"))
        .await
}

pub async fn me(State(db): State<Database>, auth: auth::AuthSession) -> ApiResult<Json<Value>> {
    let user = scope::user(&auth)?;
    let row=db.lock().await.query_one("SELECT first_name,last_name,phone,photo_url,onboarding_completed,interests,installed_apps,google_sub IS NOT NULL,password_hash<>'' AND password_hash NOT LIKE 'google-only:%' FROM users WHERE id=$1",&[&user.id]).await.map_err(database_error)?;
    Ok(Json(
        json!({"id":user.id,"email":user.email,"role":user.role,"first_name":row.get::<_,String>(0),"last_name":row.get::<_,String>(1),"phone":row.get::<_,String>(2),"photo_url":row.get::<_,String>(3),"onboarding_completed":row.get::<_,bool>(4),"interests":row.get::<_,Vec<String>>(5),"installed_apps":row.get::<_,Vec<String>>(6),"google_connected":row.get::<_,bool>(7),"has_password":row.get::<_,bool>(8)}),
    ))
}

#[derive(Deserialize)]
pub struct Preferences {
    interests: Vec<String>,
    installed_apps: Vec<String>,
}
fn valid_preferences(input: &Preferences) -> bool {
    !input.interests.is_empty()
        && input.interests.len() <= 3
        && input
            .interests
            .iter()
            .all(|v| ["tenant", "owner", "worker"].contains(&v.as_str()))
        && input.installed_apps.len() <= APPS.len()
        && input
            .installed_apps
            .iter()
            .all(|v| APPS.iter().any(|a| a.0 == v))
        && input
            .installed_apps
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
            == input.installed_apps.len()
        && input
            .interests
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
            == input.interests.len()
}
pub async fn preferences(
    State(db): State<Database>,
    auth: auth::AuthSession,
    Json(input): Json<Preferences>,
) -> ApiResult<StatusCode> {
    let user = scope::user(&auth)?;
    if !valid_preferences(&input) {
        return Err((StatusCode::BAD_REQUEST, "Choose valid preferences.".into()));
    }
    db.lock()
        .await
        .execute(
            "UPDATE users SET interests=$2,installed_apps=$3,onboarding_completed=TRUE WHERE id=$1",
            &[&user.id, &input.interests, &input.installed_apps],
        )
        .await
        .map_err(database_error)?;
    Ok(StatusCode::NO_CONTENT)
}
pub async fn catalog() -> Json<Value> {
    Json(json!(APPS
        .iter()
        .map(|a| json!({"id":a.0,"name":a.1,"path":a.2}))
        .collect::<Vec<_>>()))
}

#[derive(Deserialize)]
pub struct Profile {
    first_name: String,
    last_name: String,
    phone: String,
    #[serde(default)]
    photo_url: Option<String>,
}
pub async fn profile(
    State(db): State<Database>,
    auth: auth::AuthSession,
    Json(input): Json<Profile>,
) -> ApiResult<StatusCode> {
    let user = scope::user(&auth)?;
    if input.first_name.trim().is_empty()
        || input.first_name.len() > 60
        || input.last_name.len() > 60
        || input.phone.len() > 60
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "Enter a name and valid contact details.".into(),
        ));
    }
    let client = db.lock().await;
    if let Some(photo) = &input.photo_url {
        let image = photo
            .strip_prefix("/images/")
            .and_then(|s| s.parse::<i64>().ok())
            .ok_or((StatusCode::BAD_REQUEST, "Choose an uploaded image.".into()))?;
        if client
            .query_opt(
                "SELECT 1 FROM task_image_uploads WHERE image_id=$1 AND owner_id=$2",
                &[&image, &user.id],
            )
            .await
            .map_err(database_error)?
            .is_none()
        {
            return Err((
                StatusCode::FORBIDDEN,
                "Choose your own uploaded image.".into(),
            ));
        }
    }
    client.execute("UPDATE users SET first_name=$2,last_name=$3,phone=$4,photo_url=COALESCE($5,photo_url) WHERE id=$1",&[&user.id,&input.first_name.trim(),&input.last_name.trim(),&input.phone.trim(),&input.photo_url]).await.map_err(database_error)?;
    Ok(StatusCode::NO_CONTENT)
}
pub async fn revoke_sessions(
    State(db): State<Database>,
    auth: auth::AuthSession,
) -> ApiResult<StatusCode> {
    let user = scope::user(&auth)?;
    db.lock()
        .await
        .execute(
            "UPDATE users SET auth_version=auth_version+1 WHERE id=$1",
            &[&user.id],
        )
        .await
        .map_err(database_error)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn contacts(
    State(db): State<Database>,
    auth: auth::AuthSession,
) -> ApiResult<Json<Value>> {
    let user = scope::user(&auth)?;
    let rows=db.lock().await.query("SELECT c.id,c.status,c.requester_id,u.id,btrim(u.first_name||' '||u.last_name),u.email,u.photo_url FROM contact_connections c JOIN users u ON u.id=CASE WHEN c.requester_id=$1 THEN c.recipient_id ELSE c.requester_id END WHERE (c.requester_id=$1 OR c.recipient_id=$1) AND u.active ORDER BY lower(u.first_name),c.id",&[&user.id]).await.map_err(database_error)?;
    Ok(Json(json!(rows.iter().map(|r|json!({"id":r.get::<_,i64>(0),"status":r.get::<_,String>(1),"incoming":r.get::<_,i64>(2)!=user.id,"user_id":r.get::<_,i64>(3),"name":r.get::<_,String>(4),"email":r.get::<_,String>(5),"photo_url":r.get::<_,String>(6)})).collect::<Vec<_>>())))
}
#[derive(Deserialize)]
pub struct ContactRequest {
    email: String,
}
pub async fn request_contact(
    State(db): State<Database>,
    auth: auth::AuthSession,
    Json(input): Json<ContactRequest>,
) -> ApiResult<StatusCode> {
    let user = scope::user(&auth)?;
    let client = db.lock().await;
    let target=client.query_opt("SELECT id FROM users WHERE lower(email)=lower($1) AND active AND id<>$2 AND (password_hash<>'' OR google_sub IS NOT NULL)",&[&input.email.trim(),&user.id]).await.map_err(database_error)?.ok_or((StatusCode::BAD_REQUEST,"No available account with that email.".into()))?;
    let target: i64 = target.get(0);
    let changed=client.execute("INSERT INTO contact_connections(requester_id,recipient_id) VALUES($1,$2) ON CONFLICT DO NOTHING",&[&user.id,&target]).await.map_err(database_error)?;
    if changed == 0 {
        return Err((
            StatusCode::CONFLICT,
            "A connection or request already exists.".into(),
        ));
    }
    Ok(StatusCode::CREATED)
}
pub async fn accept_contact(
    State(db): State<Database>,
    auth: auth::AuthSession,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    let user = scope::user(&auth)?;
    let changed=db.lock().await.execute("UPDATE contact_connections SET status='accepted' WHERE id=$1 AND recipient_id=$2 AND status='pending'",&[&id,&user.id]).await.map_err(database_error)?;
    if changed == 0 {
        return Err((StatusCode::NOT_FOUND, "Request not found.".into()));
    }
    Ok(StatusCode::NO_CONTENT)
}
pub async fn remove_contact(
    State(db): State<Database>,
    auth: auth::AuthSession,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    let user = scope::user(&auth)?;
    let mut client = db.lock().await;
    let tx = client.transaction().await.map_err(database_error)?;
    let r=tx.query_opt("DELETE FROM contact_connections WHERE id=$1 AND (requester_id=$2 OR recipient_id=$2) RETURNING requester_id,recipient_id",&[&id,&user.id]).await.map_err(database_error)?.ok_or((StatusCode::NOT_FOUND,"Connection not found.".into()))?;
    let a: i64 = r.get(0);
    let b: i64 = r.get(1);
    tx.execute("DELETE FROM resource_grants WHERE (user_id=$1 AND granted_by=$2) OR (user_id=$2 AND granted_by=$1)",&[&a,&b]).await.map_err(database_error)?;
    tx.execute("UPDATE housework_tickets SET assignee_user_id=NULL,assignee='' WHERE (creator_id=$1 AND assignee_user_id=$2) OR (creator_id=$2 AND assignee_user_id=$1)",&[&a,&b]).await.map_err(database_error)?;
    tx.commit().await.map_err(database_error)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn require_owner<C: tokio_postgres::GenericClient + Sync>(
    client: &C,
    user: i64,
    kind: &str,
    id: i64,
) -> ApiResult<()> {
    let sql=match kind {"property"=>"SELECT EXISTS(SELECT 1 FROM resource_owners WHERE resource_type='property' AND resource_id=$1 AND user_id=$2)","task"=>"SELECT EXISTS(SELECT 1 FROM housework_tickets WHERE id=$1 AND creator_id=$2)",_=>return Err((StatusCode::NOT_FOUND,"Resource not found.".into()))};
    if !client
        .query_one(sql, &[&id, &user])
        .await
        .map_err(database_error)?
        .get::<_, bool>(0)
    {
        return Err((StatusCode::NOT_FOUND, "Resource not found.".into()));
    }
    Ok(())
}
pub async fn sharing(
    State(db): State<Database>,
    auth: auth::AuthSession,
    Path((kind, id)): Path<(String, i64)>,
) -> ApiResult<Json<Value>> {
    let user = scope::user(&auth)?;
    let client = db.lock().await;
    require_owner(&*client, user.id, &kind, id).await?;
    let rows=client.query("SELECT g.user_id,btrim(u.first_name||' '||u.last_name),g.permissions FROM resource_grants g JOIN users u ON u.id=g.user_id WHERE g.resource_type=$1 AND g.resource_id=$2 ORDER BY u.first_name",&[&kind,&id]).await.map_err(database_error)?;
    Ok(Json(json!(rows.iter().map(|r|json!({"user_id":r.get::<_,i64>(0),"name":r.get::<_,String>(1),"permissions":r.get::<_,Vec<String>>(2)})).collect::<Vec<_>>())))
}
#[derive(Deserialize)]
pub struct Grant {
    user_id: i64,
    permissions: Vec<String>,
}
fn valid_grant(kind: &str, permissions: &[String]) -> bool {
    permissions.len() <= 13
        && permissions.iter().all(|p| {
            if kind == "task" {
                ["view", "work"].contains(&p.as_str())
            } else {
                p == "leases.approve"
                    || FUNCTIONS
                        .iter()
                        .any(|f| p == &format!("{f}.read") || p == &format!("{f}.write"))
            }
        })
}
pub async fn grant(
    State(db): State<Database>,
    auth: auth::AuthSession,
    Path((kind, id)): Path<(String, i64)>,
    Json(mut input): Json<Grant>,
) -> ApiResult<StatusCode> {
    let user = scope::user(&auth)?;
    let mut client = db.lock().await;
    let tx = client.transaction().await.map_err(database_error)?;
    require_owner(&tx, user.id, &kind, id).await?;
    if !valid_grant(&kind, &input.permissions) || input.user_id == user.id {
        return Err((StatusCode::BAD_REQUEST, "Choose valid permissions.".into()));
    }
    for permission in input.permissions.clone() {
        let implied = if permission == "work" {
            Some("view".to_string())
        } else if permission == "leases.approve" {
            Some("leases.read".to_string())
        } else {
            permission
                .strip_suffix(".write")
                .map(|f| format!("{f}.read"))
        };
        if let Some(implied) = implied {
            if !input.permissions.contains(&implied) {
                input.permissions.push(implied);
            }
        }
    }
    input.permissions.sort();
    input.permissions.dedup();
    let contact=tx.query_opt("SELECT id FROM contact_connections WHERE status='accepted' AND ((requester_id=$1 AND recipient_id=$2) OR (requester_id=$2 AND recipient_id=$1)) FOR UPDATE",&[&user.id,&input.user_id]).await.map_err(database_error)?;
    if contact.is_none() {
        return Err((StatusCode::FORBIDDEN, "Choose an accepted contact.".into()));
    }
    if input.permissions.is_empty() {
        tx.execute(
            "DELETE FROM resource_grants WHERE resource_type=$1 AND resource_id=$2 AND user_id=$3",
            &[&kind, &id, &input.user_id],
        )
        .await
        .map_err(database_error)?;
    } else {
        tx.execute("INSERT INTO resource_grants(resource_type,resource_id,user_id,granted_by,permissions) VALUES($1,$2,$3,$4,$5) ON CONFLICT(resource_type,resource_id,user_id) DO UPDATE SET permissions=EXCLUDED.permissions,granted_by=EXCLUDED.granted_by",&[&kind,&id,&input.user_id,&user.id,&input.permissions]).await.map_err(database_error)?;
    }
    tx.commit().await.map_err(database_error)?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_unknown_apps_duplicates_and_empty_onboarding() {
        assert!(!valid_preferences(&Preferences {
            interests: vec![],
            installed_apps: vec![]
        }));
        assert!(!valid_preferences(&Preferences {
            interests: vec!["tenant".into()],
            installed_apps: vec!["admin".into()]
        }));
        assert!(!valid_preferences(&Preferences {
            interests: vec!["owner".into()],
            installed_apps: vec!["tasks".into(), "tasks".into()]
        }));
        assert!(valid_preferences(&Preferences {
            interests: vec!["tenant".into(), "owner".into()],
            installed_apps: vec!["tasks".into(), "rentals".into()]
        }));
    }
    #[test]
    fn grants_are_function_scoped() {
        assert!(!valid_grant("property", &["admin".into()]));
        assert!(!valid_grant("task", &["leases.write".into()]));
        assert!(valid_grant(
            "property",
            &["leases.approve".into(), "tasks.write".into()]
        ));
    }
}

pub async fn rentals(
    State(db): State<Database>,
    auth: auth::AuthSession,
) -> ApiResult<Json<Value>> {
    let user = scope::user(&auth)?;
    let mut client = db.lock().await;
    let tx = client.transaction().await.map_err(database_error)?;
    tx.execute("UPDATE leases SET tenant_user_id=$1 WHERE tenant_user_id IS NULL AND lower(tenant_email)=lower($2) AND status<>'draft'",&[&user.id,&user.email]).await.map_err(database_error)?;
    let rows=tx.query("SELECT l.id,l.property_id,COALESCE(p.data->>'name','Property'),COALESCE(s.data->>'name','Entire house'),l.status,l.details->>'start_date',l.details->>'end_date',l.details->>'monthly_rent',l.landlord_snapshot #>> '{landlord,legal_name}',l.signed_at IS NOT NULL,l.landlord_signed_at IS NOT NULL,l.tenancy_ended_at::text,EXISTS(SELECT 1 FROM lease_photo_ids x WHERE x.lease_id=l.id),EXISTS(SELECT 1 FROM lease_photo_ids x WHERE x.lease_id=l.id AND x.verified_at IS NOT NULL) FROM leases l JOIN properties p ON p.id=l.property_id LEFT JOIN property_spaces s ON s.property_id=l.property_id AND s.id=l.room_id WHERE l.tenant_user_id=$1 AND l.status<>'draft' ORDER BY l.id DESC",&[&user.id]).await.map_err(database_error)?;
    tx.commit().await.map_err(database_error)?;
    Ok(Json(json!(rows.iter().map(|r|json!({"id":r.get::<_,i64>(0),"property_id":r.get::<_,i64>(1),"property":r.get::<_,String>(2),"unit":r.get::<_,String>(3),"status":r.get::<_,String>(4),"start":r.get::<_,Option<String>>(5),"end":r.get::<_,Option<String>>(6),"rent":r.get::<_,Option<String>>(7),"landlord":r.get::<_,Option<String>>(8),"tenant_signed":r.get::<_,bool>(9),"landlord_signed":r.get::<_,bool>(10),"ended":r.get::<_,Option<String>>(11),"id_uploaded":r.get::<_,bool>(12),"id_approved":r.get::<_,bool>(13)})).collect::<Vec<_>>())))
}
pub async fn signatures(
    State(db): State<Database>,
    auth: auth::AuthSession,
) -> ApiResult<Json<Value>> {
    let user = scope::user(&auth)?;
    let client = db.lock().await;
    let rows=client.query("SELECT id,tenant_name,landlord_snapshot #>> '{address}',landlord_signed_at IS NOT NULL FROM leases WHERE status IN ('signed','confirmed') AND (signer_user_id=$1 OR (signer_user_id IS NULL AND lower(landlord_snapshot #>> '{landlord,email}')=lower($2))) ORDER BY id DESC",&[&user.id,&user.email]).await.map_err(database_error)?;
    Ok(Json(json!(rows.iter().map(|r|json!({"id":r.get::<_,i64>(0),"tenant":r.get::<_,String>(1),"address":r.get::<_,Option<String>>(2),"signed":r.get::<_,bool>(3)})).collect::<Vec<_>>())))
}
pub async fn review_rental(
    State(db): State<Database>,
    auth: auth::AuthSession,
    Path(id): Path<i64>,
) -> ApiResult<axum::response::Redirect> {
    review_invitation(db, auth, id, false).await
}
pub async fn review_signature(
    State(db): State<Database>,
    auth: auth::AuthSession,
    Path(id): Path<i64>,
) -> ApiResult<axum::response::Redirect> {
    review_invitation(db, auth, id, true).await
}
async fn review_invitation(
    db: Database,
    auth: auth::AuthSession,
    id: i64,
    signer: bool,
) -> ApiResult<axum::response::Redirect> {
    use sha2::{Digest, Sha256};
    let user = scope::user(&auth)?;
    let secret = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let hash = format!("{:x}", Sha256::digest(secret.as_bytes()));
    let sql = if signer {
        "UPDATE leases SET landlord_token_hash=$3,landlord_token_expires_at=now()+INTERVAL '14 days',signer_user_id=$2 WHERE id=$1 AND status IN ('signed','confirmed') AND (signer_user_id=$2 OR (signer_user_id IS NULL AND lower(landlord_snapshot #>> '{landlord,email}')=lower($4)))"
    } else {
        "UPDATE leases SET token_hash=$3,token_expires_at=now()+INTERVAL '14 days',tenant_user_id=$2 WHERE id=$1 AND status<>'draft' AND (tenant_user_id=$2 OR (tenant_user_id IS NULL AND lower(tenant_email)=lower($4)))"
    };
    if db
        .lock()
        .await
        .execute(sql, &[&id, &user.id, &hash, &user.email])
        .await
        .map_err(database_error)?
        == 0
    {
        return Err((StatusCode::NOT_FOUND, "Lease not found.".into()));
    }
    Ok(axum::response::Redirect::to(&format!(
        "/{}/{secret}",
        if signer { "lease-landlord" } else { "lease" }
    )))
}
pub async fn rental_document(
    State(db): State<Database>,
    auth: auth::AuthSession,
    Path(id): Path<i64>,
) -> ApiResult<(axum::http::HeaderMap, Vec<u8>)> {
    let user = scope::user(&auth)?;
    if db.lock().await.query_opt("SELECT 1 FROM leases WHERE id=$1 AND tenant_user_id=$2 AND status IN ('signed','confirmed')",&[&id,&user.id]).await.map_err(database_error)?.is_none(){return Err((StatusCode::NOT_FOUND,"Lease not found.".into()));}
    let bytes = crate::lease_pdf::bytes_for_id(&db, id).await?;
    let mut headers = axum::http::HeaderMap::new();
    headers.insert("content-type", "application/pdf".parse().unwrap());
    headers.insert("cache-control", "no-store".parse().unwrap());
    Ok((headers, bytes))
}

#[cfg(test)]
mod database_tests {
    #[tokio::test]
    #[ignore = "Set TEST_DATABASE_URL to an isolated restored database"]
    async fn resource_grants_and_relationships_are_isolated() {
        let url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
        assert!(url.ends_with("_test"), "Use a disposable test database");
        let (mut client, connection) = tokio_postgres::connect(&url, tokio_postgres::NoTls)
            .await
            .unwrap();
        tokio::spawn(async move {
            connection.await.unwrap();
        });
        let tx = client.transaction().await.unwrap();
        let mut ids = Vec::new();
        for name in ["owner", "worker", "tenant", "stranger"] {
            let email = format!("resource-test-{name}@example.invalid");
            ids.push(tx.query_one("INSERT INTO users(first_name,email,role,password_hash) VALUES($1,$2,'user','fixture') RETURNING id",&[&name,&email]).await.unwrap().get::<_,i64>(0));
        }
        let (owner, worker, tenant, stranger) = (ids[0], ids[1], ids[2], ids[3]);
        let landlord: i64 = tx
            .query_one(
                "INSERT INTO landlords(legal_name) VALUES('Resource test') RETURNING id",
                &[],
            )
            .await
            .unwrap()
            .get(0);
        let property = 900001_i64;
        tx.execute(
            "INSERT INTO properties(id,data,landlord_id) VALUES($1,'{}',$2)",
            &[&property, &landlord],
        )
        .await
        .unwrap();
        tx.execute(
            "INSERT INTO resource_owners VALUES('property',$1,$2)",
            &[&property, &owner],
        )
        .await
        .unwrap();
        let task:i64=tx.query_one("INSERT INTO housework_tickets(creator_id,title,property_id) VALUES($1,'Private task',$2) RETURNING id",&[&owner,&property]).await.unwrap().get(0);
        for actor in [worker, tenant, stranger] {
            assert!(!tx
                .query_one(
                    "SELECT buildry_property_can($1,$2,'details.read')",
                    &[&actor, &property]
                )
                .await
                .unwrap()
                .get::<_, bool>(0));
            assert!(!tx
                .query_one("SELECT buildry_task_can($1,$2,false)", &[&actor, &task])
                .await
                .unwrap()
                .get::<_, bool>(0));
        }
        tx.execute(
            "INSERT INTO resource_grants VALUES('task',$1,$2,$3,ARRAY['work'])",
            &[&task, &worker, &owner],
        )
        .await
        .unwrap();
        assert!(tx
            .query_one("SELECT buildry_task_can($1,$2,true)", &[&worker, &task])
            .await
            .unwrap()
            .get::<_, bool>(0));
        assert!(!tx
            .query_one(
                "SELECT buildry_property_can($1,$2,'leases.read')",
                &[&worker, &property]
            )
            .await
            .unwrap()
            .get::<_, bool>(0));
        tx.execute(
            "INSERT INTO resource_grants VALUES('property',$1,$2,$3,ARRAY['leases.write'])",
            &[&property, &worker, &owner],
        )
        .await
        .unwrap();
        assert!(tx
            .query_one(
                "SELECT buildry_property_can($1,$2,'leases.read')",
                &[&worker, &property]
            )
            .await
            .unwrap()
            .get::<_, bool>(0));
        for cap in ["finances.read", "leases.approve", "details.write"] {
            assert!(!tx
                .query_one(
                    "SELECT buildry_property_can($1,$2,$3)",
                    &[&worker, &property, &cap]
                )
                .await
                .unwrap()
                .get::<_, bool>(0));
        }
        tx.execute("DELETE FROM resource_grants WHERE user_id=$1", &[&worker])
            .await
            .unwrap();
        assert!(!tx
            .query_one("SELECT buildry_task_can($1,$2,true)", &[&worker, &task])
            .await
            .unwrap()
            .get::<_, bool>(0));
        tx.execute(
            "UPDATE housework_tickets SET assignee_user_id=$2 WHERE id=$1",
            &[&task, &worker],
        )
        .await
        .unwrap();
        assert!(tx
            .query_one("SELECT buildry_task_can($1,$2,true)", &[&worker, &task])
            .await
            .unwrap()
            .get::<_, bool>(0));
        tx.execute("UPDATE users SET active=FALSE WHERE id=$1", &[&worker])
            .await
            .unwrap();
        assert!(!tx
            .query_one("SELECT buildry_task_can($1,$2,true)", &[&worker, &task])
            .await
            .unwrap()
            .get::<_, bool>(0));
        // Existing legal-landlord payment routing wins over a newly linked user's account.
        tx.execute("INSERT INTO resource_owners VALUES('landlord',$1,$2)",&[&landlord,&owner]).await.unwrap();
        tx.execute("INSERT INTO user_stripe_accounts(user_id,test_mode,request_key,account_id,ready) VALUES($1,TRUE,'fixture-user','acct_fixture_user',TRUE)",&[&owner]).await.unwrap();
        tx.execute("INSERT INTO landlord_stripe_accounts(landlord_id,test_mode,request_key,account_id,ready) VALUES($1,TRUE,'fixture-landlord','acct_fixture_legacy',TRUE)",&[&landlord]).await.unwrap();
        assert_eq!(crate::stripe_connect::for_landlord(&tx,Some(landlord.to_string()),true).await.unwrap().unwrap().id,"acct_fixture_legacy");
        tx.execute("DELETE FROM landlord_stripe_accounts WHERE landlord_id=$1",&[&landlord]).await.unwrap();
        assert_eq!(crate::stripe_connect::for_landlord(&tx,Some(landlord.to_string()),true).await.unwrap().unwrap().id,"acct_fixture_user");
        tx.rollback().await.unwrap();
    }
}

#[cfg(test)]
mod review_fixture {
    #[tokio::test]
    #[ignore = "Creates review accounts only in an explicitly named disposable test database"]
    async fn seed_api_review_accounts() {
        let url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
        assert!(url.ends_with("_test"));
        let (client, connection) = tokio_postgres::connect(&url, tokio_postgres::NoTls)
            .await
            .unwrap();
        tokio::spawn(async move {
            connection.await.unwrap();
        });
        let hash = password_auth::generate_hash("Resource-review-only-2026!");
        for label in ["owner", "worker", "tenant", "stranger"] {
            let email = format!("review-{label}@example.test");
            client.execute("INSERT INTO users(first_name,last_name,email,role,password_hash,active) VALUES($1,'Review',$2,'user',$3,TRUE) ON CONFLICT(email) DO UPDATE SET password_hash=$3,active=TRUE",&[&label,&email,&hash]).await.unwrap();
        }
        client.execute("INSERT INTO resource_owners(resource_type,resource_id,user_id) SELECT 'property',1,id FROM users WHERE email='review-owner@example.test' ON CONFLICT DO NOTHING",&[]).await.unwrap();
        client.execute("INSERT INTO resource_owners(resource_type,resource_id,user_id) SELECT 'landlord',p.landlord_id,u.id FROM properties p CROSS JOIN users u WHERE p.id=1 AND u.email='review-owner@example.test' ON CONFLICT DO NOTHING",&[]).await.unwrap();
    }
}
