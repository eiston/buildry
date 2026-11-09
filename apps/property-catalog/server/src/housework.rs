use super::{auth, database_error, ApiResult, Database};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::NaiveDate;
#[path = "../../src/housework_model.rs"]
mod housework_model;
pub(crate) use housework_model::{HouseworkInput, HouseworkTicket};

fn validate(input: &HouseworkInput) -> ApiResult<()> {
    if input.title.trim().is_empty() || input.title.len() > 180 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Add a title of 180 characters or fewer.".into(),
        ));
    }
    if input.description.len() > 10_000 || input.assignee.len() > 120 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Description or assignee is too long.".into(),
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
    if !["backlog", "planned", "in_progress", "done", "aborted"].contains(&input.status.as_str()) {
        return Err((StatusCode::BAD_REQUEST, "Choose a valid status.".into()));
    }
    if input.status == "backlog" || input.status == "aborted" {
        if input.start_date.is_some()
            || input.start_time.is_some()
            || input.estimate_minutes.is_some()
        {
            return Err((
                StatusCode::BAD_REQUEST,
                "Backlog and aborted tasks cannot have a calendar time.".into(),
            ));
        }
    } else {
        if input.status != "planned"
            || input.start_date.is_some()
            || input.start_time.is_some()
            || input.estimate_minutes.is_some()
        {
            if input.start_date.is_none() {
                return Err((StatusCode::BAD_REQUEST, "Choose a start date.".into()));
            }
            if input.start_time.is_none() {
                return Err((StatusCode::BAD_REQUEST, "Choose a start time.".into()));
            }
            if input.estimate_minutes.is_none() {
                return Err((StatusCode::BAD_REQUEST, "Add a time estimate.".into()));
            }
        }
        if input
            .start_date
            .as_deref()
            .is_some_and(|date| NaiveDate::parse_from_str(date, "%Y-%m-%d").is_err())
        {
            return Err((StatusCode::BAD_REQUEST, "Choose a valid start date.".into()));
        }
        if let Some(time) = input.start_time.as_deref() {
            let bytes = time.as_bytes();
            if bytes.len() != 5
                || bytes[2] != b':'
                || !bytes
                    .iter()
                    .enumerate()
                    .all(|(i, b)| i == 2 || b.is_ascii_digit())
                || time[..2].parse::<u8>().unwrap_or(99) > 23
                || time[3..].parse::<u8>().unwrap_or(99) > 59
            {
                return Err((StatusCode::BAD_REQUEST, "Choose a valid start time.".into()));
            }
        }
        if input
            .estimate_minutes
            .is_some_and(|minutes| !(1..=10080).contains(&minutes))
        {
            return Err((
                StatusCode::BAD_REQUEST,
                "Estimate must be between 1 minute and 7 days.".into(),
            ));
        }
    }
    Ok(())
}

fn validate_assignment(_input: &HouseworkInput) -> ApiResult<()> {
    Ok(())
}

async fn validate_visibility(
    db: &Database,
    user_id: i64,
    _role: &str,
    input: &HouseworkInput,
) -> ApiResult<()> {
    let client = db.lock().await;
    if let Some(property_id)=input.property_id { crate::scope::require_cap(&*client,user_id,property_id,"tasks.write").await?; }
    Ok(())
}

async fn assignment<C:tokio_postgres::GenericClient+Sync>(client:&C,owner:i64,value:&str)->ApiResult<Option<i64>> {
 if value.is_empty(){return Ok(None);}
 let row=client.query_opt("SELECT id FROM users WHERE id::text=$2 AND active AND (id=$1 OR EXISTS(SELECT 1 FROM contact_connections c WHERE c.status='accepted' AND ((c.requester_id=$1 AND c.recipient_id=users.id) OR (c.recipient_id=$1 AND c.requester_id=users.id))))",&[&owner,&value]).await.map_err(database_error)?;
 row.map(|r|Some(r.get(0))).ok_or((StatusCode::BAD_REQUEST,"Choose an accepted contact.".into()))
}
async fn validate_photos<C:tokio_postgres::GenericClient+Sync>(client:&C,user:i64,photos:&[String])->ApiResult<()> {
 for photo in photos {
  let image=photo.strip_prefix("/images/").and_then(|s|s.parse::<i64>().ok()).ok_or((StatusCode::BAD_REQUEST,"Choose an uploaded task image.".into()))?;
  let allowed:bool=client.query_one("SELECT EXISTS(SELECT 1 FROM task_image_uploads WHERE image_id=$1 AND owner_id=$2) OR EXISTS(SELECT 1 FROM housework_tickets h WHERE $3=ANY(h.photos) AND buildry_task_can($2,h.id,false)) OR EXISTS(SELECT 1 FROM properties p WHERE p.data->'photos' @> to_jsonb(ARRAY[$3]::text[])) OR EXISTS(SELECT 1 FROM property_spaces s WHERE s.data->'photos' @> to_jsonb(ARRAY[$3]::text[]))",&[&image,&user,&photo]).await.map_err(database_error)?.get(0);
  if !allowed{return Err((StatusCode::FORBIDDEN,"Image not available to this account.".into()));}
 }Ok(())
}
pub(crate) fn panel_access_sql(user_parameter: usize) -> String {
    panel_permission_sql(user_parameter, false)
}

fn panel_write_sql(user_parameter: usize) -> String {
    panel_permission_sql(user_parameter, true)
}

fn panel_permission_sql(user_parameter: usize, write: bool) -> String {
    format!("buildry_task_can(${user_parameter},h.id,{write})")
}

fn can_transition(current: &str, next: &str) -> bool {
    match current {
        "backlog" => ["backlog", "planned", "aborted"].contains(&next),
        "planned" => ["planned", "in_progress", "aborted"].contains(&next),
        "in_progress" => ["in_progress", "done", "aborted"].contains(&next),
        "done" => next == "done",
        "aborted" => next == "aborted",
        _ => false,
    }
}

pub(crate) fn ticket(row: tokio_postgres::Row) -> HouseworkTicket {
    HouseworkTicket {
        id: row.get("id"),
        creator_id: row.get("creator_id"),
        title: row.get("title"),
        description: row.get("description"),
        property_id: row.get("property_id"),
        property_name: row.get("property_name"),
        assignee: row.get("assignee"),
        assignee_user_id:row.try_get("assignee_user_id").unwrap_or(None),
        can_edit:row.try_get("can_edit").unwrap_or(false),
        can_share:row.try_get("can_share").unwrap_or(false),
        status: row.get("status"),
        start_date: row.get("start_date"),
        start_time: row.get("start_time"),
        estimate_minutes: row.get("estimate_minutes"),
        photos: row.get("photos"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        tenant_request: row.get("tenant_request"),
    }
}

pub(crate) const SELECT: &str = "SELECT h.id,h.creator_id,h.title,h.description,h.property_id,COALESCE(p.data->>'name','') AS property_name,COALESCE((SELECT btrim(u.first_name||' '||u.last_name) FROM users u WHERE u.id=h.assignee_user_id),h.assignee) AS assignee,h.assignee_user_id,h.status,h.start_date::text AS start_date,h.start_time,h.estimate_minutes,h.photos,h.created_at::text AS created_at,h.updated_at::text AS updated_at,EXISTS(SELECT 1 FROM tenant_maintenance_requests r WHERE r.task_id=h.id) AS tenant_request FROM housework_tickets h LEFT JOIN properties p ON p.id=h.property_id";

pub async fn list(
    State(db): State<Database>,
    auth: auth::AuthSession,
) -> ApiResult<Json<Vec<HouseworkTicket>>> {
    let user = auth
        .user
        .ok_or((StatusCode::UNAUTHORIZED, "Sign in to view tasks.".into()))?;
    list_for(db, user.id).await
}

pub async fn list_for(db: Database, user_id: i64) -> ApiResult<Json<Vec<HouseworkTicket>>> {
    let rows = db
        .lock()
        .await
        .query(
            &format!(
                "{SELECT} WHERE {} ORDER BY h.created_at DESC, h.id DESC",
                panel_access_sql(1)
            ).replacen("SELECT ","SELECT buildry_task_can($1,h.id,true) AS can_edit,h.creator_id=$1 AS can_share,",1),
            &[&user_id],
        )
        .await
        .map_err(database_error)?;
    Ok(Json(rows.into_iter().map(ticket).collect()))
}

pub async fn create(
    State(db): State<Database>,
    auth: auth::AuthSession,
    Json(input): Json<HouseworkInput>,
) -> ApiResult<Json<HouseworkTicket>> {
    let user = auth
        .user
        .ok_or((StatusCode::UNAUTHORIZED, "Sign in to create tasks.".into()))?;
    create_for(db, user.id, &user.role, input).await
}

pub async fn create_for(
    db: Database,
    user_id: i64,
    role: &str,
    mut input: HouseworkInput,
) -> ApiResult<Json<HouseworkTicket>> {
    input.title = input.title.trim().into();
    input.assignee = input.assignee.trim().into();
    validate(&input)?;
    validate_assignment(&input)?;
    if !matches!(input.status.as_str(), "backlog" | "planned") {
        return Err((
            StatusCode::BAD_REQUEST,
            "New tasks must start in Backlog or Planned.".into(),
        ));
    }
    validate_visibility(&db, user_id, role, &input).await?;
    let mut db = db.lock().await;
    let tx = db.transaction().await.map_err(database_error)?;
    validate_photos(&tx,user_id,&input.photos).await?;
    let assigned=assignment(&tx,user_id,&input.assignee).await?;
    let row = tx.query_one("INSERT INTO housework_tickets (creator_id,title,description,property_id,assignee,status,start_date,start_time,estimate_minutes,photos) VALUES ($1,$2,$3,$4,$5,$6,$7::text::date,$8,$9,$10) RETURNING id", &[&user_id,&input.title,&input.description,&input.property_id,&input.assignee,&input.status,&input.start_date,&input.start_time,&input.estimate_minutes,&input.photos]).await.map_err(database_error)?;
    let id: i64 = row.get(0);
    tx.execute("UPDATE housework_tickets SET assignee_user_id=$2 WHERE id=$1",&[&id,&assigned]).await.map_err(database_error)?;
    let saved = tx
        .query_one(&format!("{SELECT} WHERE h.id=$1"), &[&id])
        .await
        .map_err(database_error)?;
    tx.commit().await.map_err(database_error)?;
    Ok(Json(ticket(saved)))
}

pub async fn update(
    State(db): State<Database>,
    auth: auth::AuthSession,
    Path(id): Path<i64>,
    Json(mut input): Json<HouseworkInput>,
) -> ApiResult<Json<HouseworkTicket>> {
    let user = auth
        .user
        .ok_or((StatusCode::UNAUTHORIZED, "Sign in to update tasks.".into()))?;
    input.title = input.title.trim().into();
    input.assignee = input.assignee.trim().into();
    validate(&input)?;
    let client = db.lock().await;
    let current = client
        .query_opt(
            &format!("SELECT h.status,h.title,h.description,h.property_id,EXISTS(SELECT 1 FROM tenant_maintenance_requests r WHERE r.task_id=h.id) AS tenant_request FROM housework_tickets h WHERE h.id=$1 AND {}", panel_write_sql(2)),
            &[&id,&user.id],
        )
        .await
        .map_err(database_error)?;
    let Some(current) = current else {
        return Err((StatusCode::NOT_FOUND, "Task not found.".into()));
    };
    let current_status: String = current.get(0);
    if current.get::<_, bool>("tenant_request")
        && input.property_id != current.get::<_, Option<i64>>("property_id")
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "The request property cannot be changed.".into(),
        ));
    }
    drop(client);
    if input.property_id!=current.get::<_,Option<i64>>("property_id") {
        crate::accounts::require_owner(&*db.lock().await,user.id,"task",id).await?;
        validate_visibility(&db,user.id,&user.role,&input).await?;
    }
    let client = db.lock().await;
    if input.property_id != current.get::<_,Option<i64>>("property_id") && client.query_one("SELECT EXISTS(SELECT 1 FROM account_payment_requests WHERE task_id=$1)",&[&id]).await.map_err(database_error)?.get::<_,bool>(0) {
        return Err((StatusCode::CONFLICT,"A task with bills cannot change property.".into()));
    }
    if !can_transition(&current_status, &input.status) {
        return Err((
            StatusCode::BAD_REQUEST,
            "Move through the task steps in order.".into(),
        ));
    }
    if input.status != current_status && input.status != "aborted" {
        validate_assignment(&input)?;
    }
    if matches!(current_status.as_str(), "in_progress" | "done" | "aborted")
        && (input.title != current.get::<_, String>("title")
            || input.description != current.get::<_, String>("description")
            || input.property_id != current.get::<_, Option<i64>>("property_id"))
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "Title, description and house are locked after work starts.".into(),
        ));
    }
    let owner:i64=client.query_one("SELECT creator_id FROM housework_tickets WHERE id=$1",&[&id]).await.map_err(database_error)?.get(0);
    let old_assignment:String=client.query_one("SELECT assignee FROM housework_tickets WHERE id=$1",&[&id]).await.map_err(database_error)?.get(0);
    if input.assignee!=old_assignment && owner!=user.id { return Err((StatusCode::FORBIDDEN,"Only the task owner can assign people.".into())); }
    validate_photos(&*client,user.id,&input.photos).await?;
    let assigned=assignment(&*client,owner,&input.assignee).await?;
    let changed = client.execute(&format!("UPDATE housework_tickets h SET title=$2,description=$3,property_id=$4,assignee=$5,status=$6,start_date=$7::text::date,start_time=$8,estimate_minutes=$9,photos=$10,updated_at=NOW() WHERE h.id=$1 AND {}", panel_write_sql(11)), &[&id,&input.title,&input.description,&input.property_id,&input.assignee,&input.status,&input.start_date,&input.start_time,&input.estimate_minutes,&input.photos,&user.id]).await.map_err(database_error)?;
    if changed == 0 {
        return Err((StatusCode::NOT_FOUND, "Task not found.".into()));
    }
    client.execute("UPDATE housework_tickets SET assignee_user_id=$2 WHERE id=$1",&[&id,&assigned]).await.map_err(database_error)?;
    let row = client
        .query_one(&format!("{SELECT} WHERE h.id=$1"), &[&id])
        .await
        .map_err(database_error)?;
    Ok(Json(ticket(row)))
}

pub async fn delete(
    State(db): State<Database>,
    auth: auth::AuthSession,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    let user = auth
        .user
        .ok_or((StatusCode::UNAUTHORIZED, "Sign in to delete tasks.".into()))?;
    let removed = db
        .lock()
        .await
        .execute(
            &format!("DELETE FROM housework_tickets h WHERE h.id=$1 AND h.creator_id=$2 AND {}", panel_write_sql(2)),
            &[&id, &user.id],
        )
        .await
        .map_err(|e|if e.code()==Some(&tokio_postgres::error::SqlState::FOREIGN_KEY_VIOLATION){(StatusCode::CONFLICT,"Tasks with bills cannot be deleted.".into())}else{database_error(e)})?;
    if removed == 0 {
        return Err((StatusCode::NOT_FOUND, "Task not found.".into()));
    }
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input(status: &str) -> HouseworkInput {
        HouseworkInput {
            title: "Clean kitchen".into(),
            description: String::new(),
            property_id: None,
            assignee: String::new(),
            status: status.into(),
            start_date: None,
            start_time: None,
            estimate_minutes: None,
            photos: vec![],
        }
    }
    #[test]
    fn planned_can_be_drafted_but_work_requires_schedule() {
        let mut item = input("planned");
        assert!(validate(&item).is_ok());
        item.estimate_minutes = Some(90);
        assert!(validate(&item).is_err());
        item.estimate_minutes = None;
        item.start_date = Some("2026-10-20".into());
        item.start_time = Some("09:30".into());
        item.estimate_minutes = Some(90);
        assert!(validate(&item).is_ok());
        let mut started = item.clone();
        started.status = "in_progress".into();
        assert!(validate(&started).is_ok());
        started.start_time = None;
        assert!(validate(&started).is_err());
    }
    #[test]
    fn backlog_has_no_schedule() {
        let mut item = input("backlog");
        assert!(validate(&item).is_ok());
        item.start_date = Some("2026-10-20".into());
        assert!(validate(&item).is_err());
    }
    #[test]
    fn task_steps_move_forward_and_terminal_states_stay_terminal() {
        assert!(can_transition("backlog", "planned"));
        assert!(can_transition("planned", "in_progress"));
        assert!(can_transition("in_progress", "done"));
        assert!(can_transition("planned", "aborted"));
        assert!(!can_transition("backlog", "done"));
        assert!(!can_transition("done", "planned"));
        assert!(!can_transition("aborted", "backlog"));
    }
}
