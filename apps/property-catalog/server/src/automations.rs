use super::{auth, database_error, ApiResult, Database};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::{Datelike, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Serialize)]
pub struct Template {
    id: i64,
    key: String,
    name: String,
    description: String,
    enabled: bool,
    trigger: Value,
    conditions: Value,
    action: Value,
}

#[derive(Serialize)]
pub struct PropertyPreset {
    template: Template,
    enabled: bool,
}

#[derive(Deserialize)]
pub struct Toggle {
    enabled: bool,
}

#[derive(Deserialize)]
pub struct EventInput {
    event_type: String,
    #[serde(default)]
    payload: Value,
    source_key: Option<String>,
}

#[derive(Deserialize)]
pub struct TemplateInput {
    #[serde(default)]
    key: String,
    name: String,
    description: String,
    enabled: bool,
    trigger: Value,
    conditions: Value,
    action: Value,
}

pub async fn migrate(client: &tokio_postgres::Client) -> Result<(), tokio_postgres::Error> {
    client.batch_execute(r#"
        CREATE TABLE IF NOT EXISTS automation_templates (
            id BIGSERIAL PRIMARY KEY, key TEXT NOT NULL UNIQUE, name TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '', enabled BOOLEAN NOT NULL DEFAULT TRUE,
            trigger JSONB NOT NULL, conditions JSONB NOT NULL DEFAULT '{}'::jsonb,
            action JSONB NOT NULL, updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
        );
        CREATE TABLE IF NOT EXISTS property_automations (
            property_id BIGINT NOT NULL REFERENCES properties(id) ON DELETE CASCADE,
            template_id BIGINT NOT NULL REFERENCES automation_templates(id) ON DELETE CASCADE,
            enabled BOOLEAN NOT NULL DEFAULT FALSE, updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
            PRIMARY KEY (property_id, template_id)
        );
        CREATE TABLE IF NOT EXISTS automation_events (
            id BIGSERIAL PRIMARY KEY, property_id BIGINT NOT NULL REFERENCES properties(id) ON DELETE CASCADE,
            event_type TEXT NOT NULL, payload JSONB NOT NULL DEFAULT '{}'::jsonb,
            source_key TEXT,
            occurred_at TIMESTAMPTZ NOT NULL DEFAULT now()
        );
        ALTER TABLE automation_events ADD COLUMN IF NOT EXISTS source_key TEXT;
        CREATE UNIQUE INDEX IF NOT EXISTS automation_events_source_idx ON automation_events(property_id,event_type,source_key) WHERE source_key IS NOT NULL;
        CREATE TABLE IF NOT EXISTS automation_runs (
            id BIGSERIAL PRIMARY KEY, property_id BIGINT NOT NULL REFERENCES properties(id) ON DELETE CASCADE,
            template_id BIGINT NOT NULL REFERENCES automation_templates(id) ON DELETE CASCADE,
            occurrence_key TEXT NOT NULL, status TEXT NOT NULL,
            task_id BIGINT REFERENCES housework_tickets(id) ON DELETE SET NULL,
            detail TEXT NOT NULL DEFAULT '', created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
            UNIQUE(property_id, template_id, occurrence_key)
        );
        ALTER TABLE housework_tickets ADD COLUMN IF NOT EXISTS automation_run_id BIGINT REFERENCES automation_runs(id) ON DELETE SET NULL;
        CREATE INDEX IF NOT EXISTS automation_runs_recent_idx ON automation_runs(created_at DESC);
        INSERT INTO automation_templates(key,name,description,trigger,action) VALUES
          ('snow-removal','Snow removal','Create a task when the forecast calls for at least 2 cm of snow.', '{"type":"weather","metric":"snowfall_sum","min":2}', '{"type":"create_task","title":"Clear snow and ice","description":"Clear walkways, steps, and access points after forecast snowfall."}'),
          ('lawn-care','Lawn care','Create a recurring lawn care task during the growing season.', '{"type":"schedule","every_days":14,"months":[4,5,6,7,8,9,10]}', '{"type":"create_task","title":"Maintain lawn","description":"Mow and tidy the lawn and outdoor areas."}'),
          ('gutter-cleaning','Gutter cleaning','Inspect and clean gutters each spring and fall.', '{"type":"season","month_days":["04-15","10-15"]}', '{"type":"create_task","title":"Clean gutters","description":"Inspect and clear gutters and downspouts."}'),
          ('hvac-filter','HVAC filter replacement','Replace HVAC filters every 90 days.', '{"type":"schedule","every_days":90}', '{"type":"create_task","title":"Replace HVAC filter","description":"Replace the HVAC filter and check airflow."}'),
          ('winter-preparation','Winter preparation','Prepare the property each autumn.', '{"type":"season","month_days":["10-01"]}', '{"type":"create_task","title":"Prepare property for winter","description":"Check heating, exterior taps, weather sealing, and winter supplies."}')
        ON CONFLICT (key) DO NOTHING;
    "#).await
}

fn template(row: &tokio_postgres::Row) -> Template {
    Template {
        id: row.get(0),
        key: row.get(1),
        name: row.get(2),
        description: row.get(3),
        enabled: row.get(4),
        trigger: row.get(5),
        conditions: row.get(6),
        action: row.get(7),
    }
}

const TEMPLATE_COLUMNS: &str =
    "t.id,t.key,t.name,t.description,t.enabled,t.trigger,t.conditions,t.action";

pub async fn list_templates(State(db): State<Database>) -> ApiResult<Json<Vec<Template>>> {
    let rows = db
        .lock()
        .await
        .query(
            &format!("SELECT {TEMPLATE_COLUMNS} FROM automation_templates t ORDER BY t.id"),
            &[],
        )
        .await
        .map_err(database_error)?;
    Ok(Json(rows.iter().map(template).collect()))
}

fn valid_rule(input: &TemplateInput) -> bool {
    if input.name.trim().is_empty() || input.name.len() > 120 || input.description.len() > 1000 {
        return false;
    }
    let trigger = &input.trigger;
    let valid_trigger = match trigger["type"].as_str() {
        Some("schedule") => trigger["every_days"]
            .as_i64()
            .is_some_and(|n| (1..=3650).contains(&n)),
        Some("season") => trigger["month_days"].as_array().is_some_and(|days| {
            !days.is_empty()
                && days.iter().all(|day| {
                    day.as_str().is_some_and(|s| {
                        NaiveDate::parse_from_str(&format!("2024-{s}"), "%Y-%m-%d").is_ok()
                    })
                })
        }),
        Some("weather") => {
            matches!(
                trigger["metric"].as_str(),
                Some("snowfall_sum" | "rain_sum" | "temperature_2m_min")
            ) && (trigger["min"].as_f64().is_some() || trigger["max"].as_f64().is_some())
        }
        Some("property_event") => trigger["event"]
            .as_str()
            .is_some_and(|event| !event.is_empty() && event.len() <= 100),
        _ => false,
    };
    let valid_action = input.action["type"] == "create_task"
        && input.action["title"]
            .as_str()
            .is_some_and(|s| !s.trim().is_empty() && s.len() <= 180)
        && input.action["description"]
            .as_str()
            .is_some_and(|s| s.len() <= 10_000);
    let valid_conditions = input.conditions.is_object()
        && input.conditions.as_object().is_some_and(|map| {
            map.keys()
                .all(|key| key == "property_equals" || key == "requires_coordinates")
        })
        && (input.conditions["property_equals"].is_null()
            || input.conditions["property_equals"].is_object());
    valid_trigger && valid_action && valid_conditions
}

pub async fn save_template(
    State(db): State<Database>,
    Path(id): Path<i64>,
    Json(input): Json<TemplateInput>,
) -> ApiResult<Json<Template>> {
    if !valid_rule(&input) {
        return Err((
            StatusCode::BAD_REQUEST,
            "Enter a valid trigger, conditions, and task action.".into(),
        ));
    }
    let row = db.lock().await.query_opt(&format!("UPDATE automation_templates t SET name=$2,description=$3,enabled=$4,trigger=$5,conditions=$6,action=$7,updated_at=now() WHERE id=$1 RETURNING {TEMPLATE_COLUMNS}"),
        &[&id,&input.name.trim(),&input.description,&input.enabled,&input.trigger,&input.conditions,&input.action]).await.map_err(database_error)?;
    row.map(|r| Json(template(&r))).ok_or((
        StatusCode::NOT_FOUND,
        "Automation template not found.".into(),
    ))
}

pub async fn create_template(
    State(db): State<Database>,
    Json(input): Json<TemplateInput>,
) -> ApiResult<Json<Template>> {
    if !valid_rule(&input)
        || input.key.is_empty()
        || input.key.len() > 80
        || !input
            .key
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "Enter a unique lowercase key and valid rule.".into(),
        ));
    }
    let row = db.lock().await.query_one(&format!("INSERT INTO automation_templates t(key,name,description,enabled,trigger,conditions,action) VALUES($1,$2,$3,$4,$5,$6,$7) RETURNING {TEMPLATE_COLUMNS}"),
        &[&input.key,&input.name.trim(),&input.description,&input.enabled,&input.trigger,&input.conditions,&input.action]).await.map_err(database_error)?;
    Ok(Json(template(&row)))
}

async fn owned_property(
    db: &Database,
    property_id: i64,
    user_id: i64,
    role: &str,
) -> ApiResult<()> {
    let row = db.lock().await.query_opt("SELECT p.id FROM properties p LEFT JOIN users u ON u.id=$2 WHERE p.id=$1 AND ($3::text IN ('admin','staff') OR (u.role='landlord' AND u.landlord_id=p.landlord_id))", &[&property_id,&user_id,&role]).await.map_err(database_error)?;
    if row.is_none() {
        return Err((StatusCode::NOT_FOUND, "Property not found.".into()));
    }
    Ok(())
}

pub async fn property_presets(
    State(db): State<Database>,
    auth: auth::AuthSession,
    Path(id): Path<i64>,
) -> ApiResult<Json<Vec<PropertyPreset>>> {
    let user = auth
        .user
        .ok_or((StatusCode::UNAUTHORIZED, "Sign in.".into()))?;
    owned_property(&db, id, user.id, &user.role).await?;
    let rows = db.lock().await.query(&format!("SELECT {TEMPLATE_COLUMNS},COALESCE(pa.enabled,false) FROM automation_templates t LEFT JOIN property_automations pa ON pa.template_id=t.id AND pa.property_id=$1 ORDER BY t.id"), &[&id]).await.map_err(database_error)?;
    Ok(Json(
        rows.iter()
            .map(|r| PropertyPreset {
                template: template(r),
                enabled: r.get(8),
            })
            .collect(),
    ))
}

pub async fn set_property_preset(
    State(db): State<Database>,
    auth: auth::AuthSession,
    Path((id, template_id)): Path<(i64, i64)>,
    Json(input): Json<Toggle>,
) -> ApiResult<StatusCode> {
    let user = auth
        .user
        .ok_or((StatusCode::UNAUTHORIZED, "Sign in.".into()))?;
    owned_property(&db, id, user.id, &user.role).await?;
    let changed = db.lock().await.execute("INSERT INTO property_automations(property_id,template_id,enabled) SELECT $1,id,$3 FROM automation_templates WHERE id=$2 ON CONFLICT(property_id,template_id) DO UPDATE SET enabled=EXCLUDED.enabled,updated_at=now()", &[&id,&template_id,&input.enabled]).await.map_err(database_error)?;
    if changed == 0 {
        return Err((StatusCode::NOT_FOUND, "Preset not found.".into()));
    }
    Ok(StatusCode::NO_CONTENT)
}

pub async fn history(
    State(db): State<Database>,
    auth: auth::AuthSession,
    Path(id): Path<i64>,
) -> ApiResult<Json<Vec<Value>>> {
    let user = auth
        .user
        .ok_or((StatusCode::UNAUTHORIZED, "Sign in.".into()))?;
    owned_property(&db, id, user.id, &user.role).await?;
    let rows = db.lock().await.query("SELECT t.name,r.occurrence_key,r.status,r.task_id,r.detail,r.created_at::text FROM automation_runs r JOIN automation_templates t ON t.id=r.template_id WHERE r.property_id=$1 ORDER BY r.created_at DESC LIMIT 50", &[&id]).await.map_err(database_error)?;
    Ok(Json(rows.iter().map(|r| json!({"name":r.get::<_,String>(0),"occurrence_key":r.get::<_,String>(1),"status":r.get::<_,String>(2),"task_id":r.get::<_,Option<i64>>(3),"detail":r.get::<_,String>(4),"created_at":r.get::<_,String>(5)})).collect()))
}

pub async fn record_event(
    State(db): State<Database>,
    auth: auth::AuthSession,
    Path(id): Path<i64>,
    Json(input): Json<EventInput>,
) -> ApiResult<StatusCode> {
    let user = auth
        .user
        .ok_or((StatusCode::UNAUTHORIZED, "Sign in.".into()))?;
    owned_property(&db, id, user.id, &user.role).await?;
    if input.event_type.is_empty()
        || input.event_type.len() > 100
        || !input
            .event_type
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        || !input.payload.is_object()
        || input
            .source_key
            .as_deref()
            .is_some_and(|key| key.is_empty() || key.len() > 120)
    {
        return Err((StatusCode::BAD_REQUEST, "Invalid property event.".into()));
    }
    db.lock().await.execute("INSERT INTO automation_events(property_id,event_type,payload,source_key) VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING", &[&id,&input.event_type,&input.payload,&input.source_key]).await.map_err(database_error)?;
    Ok(StatusCode::ACCEPTED)
}

fn schedule_due(trigger: &Value, date: NaiveDate, enabled_on: NaiveDate) -> Option<String> {
    match trigger["type"].as_str()? {
        "schedule" => {
            let every = trigger["every_days"].as_i64()?;
            let elapsed = (date - enabled_on).num_days();
            if elapsed < 0 {
                return None;
            }
            let due = enabled_on + chrono::Duration::days(elapsed / every * every);
            let months = trigger["months"].as_array();
            if months.is_some_and(|items| {
                !items
                    .iter()
                    .any(|month| month.as_u64() == Some(due.month() as u64))
            }) {
                return None;
            }
            Some(due.to_string())
        }
        "season" => {
            let mut dates = Vec::new();
            for year in [date.year() - 1, date.year()] {
                for day in trigger["month_days"].as_array()? {
                    let Ok(candidate) =
                        NaiveDate::parse_from_str(&format!("{year}-{}", day.as_str()?), "%Y-%m-%d")
                    else {
                        continue;
                    };
                    if candidate >= enabled_on && candidate <= date {
                        dates.push(candidate);
                    }
                }
            }
            dates.into_iter().max().map(|due| due.to_string())
        }
        _ => None,
    }
}

async fn forecast(client: &reqwest::Client, lat: f64, lon: f64) -> Result<Value, String> {
    let response = client
        .get("https://api.open-meteo.com/v1/forecast")
        .query(&[
            ("latitude", lat.to_string()),
            ("longitude", lon.to_string()),
            ("daily", "snowfall_sum,rain_sum,temperature_2m_min".into()),
            ("timezone", "America/Toronto".into()),
            ("forecast_days", "2".into()),
        ])
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;
    response.json().await.map_err(|e| e.to_string())
}

async fn run_once(db: &Database, http: &reqwest::Client) -> Result<(), String> {
    let today = Utc::now()
        .with_timezone(&chrono_tz::America::Toronto)
        .date_naive();
    let rows = db.lock().await.query("SELECT pa.property_id,t.id,t.trigger,t.conditions,t.action,p.data,(pa.updated_at AT TIME ZONE 'America/Toronto')::date::text FROM property_automations pa JOIN automation_templates t ON t.id=pa.template_id JOIN properties p ON p.id=pa.property_id WHERE pa.enabled AND t.enabled", &[]).await.map_err(|e|e.to_string())?;
    for row in rows {
        let property_id: i64 = row.get(0);
        let template_id: i64 = row.get(1);
        let trigger: Value = row.get(2);
        let conditions: Value = row.get(3);
        let action: Value = row.get(4);
        let property: Value = row.get(5);
        let enabled_on =
            NaiveDate::parse_from_str(&row.get::<_, String>(6), "%Y-%m-%d").unwrap_or(today);
        if conditions["property_equals"]
            .as_object()
            .is_some_and(|expected| {
                expected
                    .iter()
                    .any(|(key, value)| property.get(key) != Some(value))
            })
        {
            continue;
        }
        if conditions["requires_coordinates"] == true
            && (property["latitude"].as_f64().is_none() || property["longitude"].as_f64().is_none())
        {
            continue;
        }
        let occurrences: Vec<String> = match trigger["type"].as_str() {
            Some("schedule" | "season") => schedule_due(&trigger, today, enabled_on)
                .into_iter()
                .collect(),
            Some("weather") => {
                if let (Some(lat), Some(lon)) = (
                    property["latitude"].as_f64(),
                    property["longitude"].as_f64(),
                ) {
                    match forecast(http, lat, lon).await {
                        Ok(f) => f["daily"]["time"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .enumerate()
                            .filter_map(|(i, day)| {
                                let value = f["daily"][trigger["metric"].as_str()?][i].as_f64()?;
                                let meets_min =
                                    trigger["min"].as_f64().is_none_or(|min| value >= min);
                                let meets_max =
                                    trigger["max"].as_f64().is_none_or(|max| value <= max);
                                (meets_min && meets_max).then(|| {
                                    format!(
                                        "weather:{}:{}",
                                        trigger["metric"].as_str().unwrap_or(""),
                                        day.as_str().unwrap_or("")
                                    )
                                })
                            })
                            .collect(),
                        Err(error) => {
                            eprintln!("Weather automation for property {property_id}: {error}");
                            vec![]
                        }
                    }
                } else {
                    vec![]
                }
            }
            Some("property_event") => {
                let event = trigger["event"].as_str().unwrap_or("");
                let rows = db.lock().await.query("SELECT id FROM automation_events WHERE property_id=$1 AND event_type=$2 AND occurred_at>now()-interval '90 days'", &[&property_id,&event]).await.map_err(|e|e.to_string())?;
                rows.iter()
                    .map(|r| format!("event:{}", r.get::<_, i64>(0)))
                    .collect()
            }
            _ => vec![],
        };
        for key in occurrences {
            let mut locked = db.lock().await;
            let tx = locked.transaction().await.map_err(|e| e.to_string())?;
            let run = tx.query_opt("INSERT INTO automation_runs(property_id,template_id,occurrence_key,status) VALUES($1,$2,$3,'running') ON CONFLICT DO NOTHING RETURNING id", &[&property_id,&template_id,&key]).await.map_err(|e|e.to_string())?;
            let Some(run) = run else {
                continue;
            };
            let run_id: i64 = run.get(0);
            let staff_ids: Vec<i64> = property["manager_staff_ids"]
                .as_array()
                .map(|a| a.iter().filter_map(Value::as_i64).collect())
                .unwrap_or_default();
            let manager = tx.query_opt("SELECT btrim(first_name || ' ' || last_name) FROM users WHERE active AND id=ANY($1) ORDER BY id LIMIT 1", &[&staff_ids]).await.map_err(|e|e.to_string())?;
            let assignee = manager
                .map(|r| r.get::<_, String>(0))
                .or_else(|| {
                    if property["owner_manages_tours"] == true {
                        property["owner_name"].as_str().map(str::to_string)
                    } else {
                        None
                    }
                })
                .unwrap_or_default();
            let creator = tx
                .query_opt(
                    "SELECT id FROM users WHERE role='admin' AND active ORDER BY id LIMIT 1",
                    &[],
                )
                .await
                .map_err(|e| e.to_string())?
                .map(|r| r.get::<_, i64>(0));
            let task = tx.query_one("INSERT INTO housework_tickets(creator_id,title,description,property_id,assignee,status,automation_run_id) VALUES($1,$2,$3,$4,$5,'backlog',$6) RETURNING id", &[&creator,&action["title"].as_str().unwrap_or("Maintenance task"),&action["description"].as_str().unwrap_or(""),&property_id,&assignee,&run_id]).await.map_err(|e|e.to_string())?;
            let task_id: i64 = task.get(0);
            tx.execute(
                "UPDATE automation_runs SET status='created',task_id=$2 WHERE id=$1",
                &[&run_id, &task_id],
            )
            .await
            .map_err(|e| e.to_string())?;
            tx.commit().await.map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

pub fn start_worker(db: Database) {
    tokio::spawn(async move {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(12))
            .build()
            .expect("HTTP client");
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60 * 60));
        loop {
            interval.tick().await;
            if let Err(error) = run_once(&db, &http).await {
                eprintln!("Automation evaluation failed: {error}");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recurrence_is_deterministic() {
        let day = NaiveDate::from_ymd_opt(2026, 10, 15).unwrap();
        assert_eq!(
            schedule_due(&json!({"type":"season","month_days":["10-15"]}), day, day),
            Some(day.to_string())
        );
        assert_eq!(
            schedule_due(&json!({"type":"season","month_days":["04-15"]}), day, day),
            None
        );
        assert_eq!(
            schedule_due(&json!({"type":"schedule","every_days":14}), day, day),
            Some(day.to_string())
        );
        assert_eq!(
            schedule_due(
                &json!({"type":"schedule","every_days":14}),
                day + chrono::Duration::days(16),
                day
            ),
            Some((day + chrono::Duration::days(14)).to_string())
        );
    }
}
