use crate::{
    auth::{AuthSession, Database},
    database_error, housework, load_properties, ApiResult,
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct Tenant {
    pub id: i64,
    pub name: String,
    pub email: String,
    pub phone: String,
    pub active: bool,
}

#[derive(Serialize)]
pub struct MyTenancy {
    id: i64,
    property_id: i64,
    property_name: String,
    room_name: String,
    start_date: String,
    end_date: String,
}

pub async fn my_tenancies(
    auth: AuthSession,
    State(db): State<Database>,
) -> ApiResult<Json<Vec<MyTenancy>>> {
    let user = auth
        .user
        .ok_or((StatusCode::UNAUTHORIZED, "Sign in required.".into()))?;
    if user.role != "tenant" {
        return Err((StatusCode::FORBIDDEN, "Tenant account required.".into()));
    }
    let client = db.lock().await;
    let rows = client
        .query(
            "SELECT l.id,l.property_id,l.room_id,l.details->>'start_date',l.details->>'end_date'
             FROM leases l JOIN users u ON lower(u.email)=lower(l.tenant_email)
             WHERE u.id=$1 AND l.status='confirmed'
             ORDER BY l.details->>'start_date' DESC,l.id DESC",
            &[&user.id],
        )
        .await
        .map_err(database_error)?;
    let Json(properties) = load_properties(&client).await?;
    Ok(Json(
        rows.into_iter()
            .map(|row| {
                let property_id: i64 = row.get(1);
                let room_id: i64 = row.get(2);
                let house = properties
                    .iter()
                    .find(|house| house.id as i64 == property_id);
                MyTenancy {
                    id: row.get(0),
                    property_id,
                    property_name: house
                        .map(|house| house.name.clone())
                        .unwrap_or_else(|| "Property".into()),
                    room_name: house
                        .and_then(|house| {
                            house.spaces.iter().find(|room| room.id as i64 == room_id)
                        })
                        .map(|room| room.name.clone())
                        .unwrap_or_else(|| "Room".into()),
                    start_date: row.get::<_, Option<String>>(3).unwrap_or_default(),
                    end_date: row.get::<_, Option<String>>(4).unwrap_or_default(),
                }
            })
            .collect(),
    ))
}

#[derive(Deserialize)]
pub struct MaintenanceRequestInput {
    title: String,
    description: String,
    photos: Vec<String>,
    property_id: Option<i64>,
}

fn tenant_id(auth: AuthSession) -> ApiResult<i64> {
    let user = auth
        .user
        .ok_or((StatusCode::UNAUTHORIZED, "Sign in required.".into()))?;
    if user.role != "tenant" {
        return Err((StatusCode::FORBIDDEN, "Tenant account required.".into()));
    }
    Ok(user.id)
}

pub async fn list_maintenance_requests(
    auth: AuthSession,
    State(db): State<Database>,
) -> ApiResult<Json<Vec<housework::HouseworkTicket>>> {
    let user_id = tenant_id(auth)?;
    let rows = db.lock().await.query(
        &format!("{} JOIN tenant_maintenance_requests r ON r.task_id=h.id WHERE r.tenant_id=$1 ORDER BY h.created_at DESC,h.id DESC", housework::SELECT),
        &[&user_id],
    ).await.map_err(database_error)?;
    Ok(Json(rows.into_iter().map(housework::ticket).collect()))
}

pub async fn get_maintenance_request(
    auth: AuthSession,
    State(db): State<Database>,
    Path(id): Path<i64>,
) -> ApiResult<Json<housework::HouseworkTicket>> {
    let user_id = tenant_id(auth)?;
    let row = db.lock().await.query_opt(
        &format!("{} JOIN tenant_maintenance_requests r ON r.task_id=h.id WHERE r.tenant_id=$1 AND h.id=$2", housework::SELECT),
        &[&user_id, &id],
    ).await.map_err(database_error)?;
    row.map(|row| Json(housework::ticket(row)))
        .ok_or((StatusCode::NOT_FOUND, "Request not found.".into()))
}

pub async fn create_maintenance_request(
    auth: AuthSession,
    State(db): State<Database>,
    Json(mut input): Json<MaintenanceRequestInput>,
) -> ApiResult<Json<housework::HouseworkTicket>> {
    let user_id = tenant_id(auth)?;
    input.title = input.title.trim().to_string();
    input.description = input.description.trim().to_string();
    if input.title.is_empty()
        || input.title.len() > 180
        || input.description.is_empty()
        || input.description.len() > 10_000
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "Add a title and description within the limits.".into(),
        ));
    }
    if input.photos.len() > 24
        || input
            .photos
            .iter()
            .any(|photo| !photo.starts_with("/images/") || photo.len() > 200)
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "Add up to 24 uploaded images.".into(),
        ));
    }
    let mut client = db.lock().await;
    let tx = client.transaction().await.map_err(database_error)?;
    let property = tx.query_opt(
        "SELECT l.property_id FROM leases l JOIN users u ON lower(u.email)=lower(l.tenant_email) WHERE u.id=$1 AND l.status='confirmed' AND ($2::bigint IS NULL OR l.property_id=$2) ORDER BY l.details->>'start_date' DESC,l.id DESC LIMIT 1",
        &[&user_id, &input.property_id],
    ).await.map_err(database_error)?
        .ok_or((StatusCode::FORBIDDEN, "A confirmed tenancy is required.".into()))?;
    let property_id: i64 = property.get(0);
    for photo in &input.photos {
        let image_id = photo
            .trim_start_matches("/images/")
            .parse::<i64>()
            .map_err(|_| (StatusCode::BAD_REQUEST, "Choose an uploaded image.".into()))?;
        let owned = tx
            .query_opt(
                "SELECT 1 FROM task_image_uploads WHERE image_id=$1 AND owner_id=$2",
                &[&image_id, &user_id],
            )
            .await
            .map_err(database_error)?;
        if owned.is_none() {
            return Err((
                StatusCode::BAD_REQUEST,
                "Choose images you uploaded.".into(),
            ));
        }
    }
    let id: i64 = tx.query_one(
        "INSERT INTO housework_tickets (creator_id,title,description,property_id,assignee,status,photos) VALUES ($1,$2,$3,$4,'','backlog',$5) RETURNING id",
        &[&user_id, &input.title, &input.description, &property_id, &input.photos],
    ).await.map_err(database_error)?.get(0);
    tx.execute(
        "INSERT INTO tenant_maintenance_requests (task_id,tenant_id) VALUES ($1,$2)",
        &[&id, &user_id],
    )
    .await
    .map_err(database_error)?;
    let saved = tx
        .query_one(&format!("{} WHERE h.id=$1", housework::SELECT), &[&id])
        .await
        .map_err(database_error)?;
    tx.commit().await.map_err(database_error)?;
    Ok(Json(housework::ticket(saved)))
}

pub async fn list(State(db): State<Database>, auth: AuthSession) -> ApiResult<Json<Vec<Tenant>>> {
    let user = crate::scope::user(&auth)?;
    let client = db.lock().await;
    let ids = crate::scope::property_ids(&client, user.id).await?;
    let rows = client
        .query(
            "SELECT id, btrim(first_name || ' ' || last_name), email, phone, active FROM users WHERE role = 'tenant' AND ($1::text='admin' OR EXISTS (SELECT 1 FROM leases l WHERE lower(l.tenant_email)=lower(users.email) AND l.property_id=ANY($2::bigint[]))) ORDER BY first_name, last_name, id",
            &[&user.role, &ids],
        )
        .await
        .map_err(database_error)?;
    Ok(Json(
        rows.into_iter()
            .map(|row| Tenant {
                id: row.get(0),
                name: row.get(1),
                email: row.get(2),
                phone: row.get(3),
                active: row.get(4),
            })
            .collect(),
    ))
}
