use crate::{
    auth::{AuthSession, Database, User},
    billing::{self, unavailable, Stripe},
    database_error,
    payout_setup::for_user,
    ApiResult,
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub async fn migrate(db: &tokio_postgres::Client) -> Result<(), tokio_postgres::Error> {
    db.batch_execute("CREATE TABLE IF NOT EXISTS account_payment_requests (
      id UUID PRIMARY KEY, creator_id BIGINT NOT NULL REFERENCES users(id), payer_id BIGINT NOT NULL REFERENCES users(id),
      receiver_id BIGINT NOT NULL REFERENCES users(id), lease_id BIGINT NOT NULL REFERENCES leases(id),
      amount_cents BIGINT NOT NULL CHECK(amount_cents BETWEEN 100 AND 99999999), description TEXT NOT NULL,
      test_mode BOOLEAN NOT NULL, account_id TEXT, session_id TEXT, status TEXT NOT NULL DEFAULT 'requested',
      created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(), CHECK(payer_id<>receiver_id)
    ); ALTER TABLE account_payment_requests ADD COLUMN IF NOT EXISTS receipt_url TEXT;
      ALTER TABLE account_payment_requests ALTER COLUMN payer_id DROP NOT NULL;
      ALTER TABLE account_payment_requests ALTER COLUMN receiver_id DROP NOT NULL;
      ALTER TABLE account_payment_requests ALTER COLUMN lease_id DROP NOT NULL;
      ALTER TABLE account_payment_requests ADD COLUMN IF NOT EXISTS payer_email TEXT NOT NULL DEFAULT '';
      ALTER TABLE account_payment_requests ADD COLUMN IF NOT EXISTS receiver_email TEXT NOT NULL DEFAULT '';
      ALTER TABLE account_payment_requests ADD COLUMN IF NOT EXISTS payer_name TEXT NOT NULL DEFAULT '';
      ALTER TABLE account_payment_requests ADD COLUMN IF NOT EXISTS receiver_name TEXT NOT NULL DEFAULT '';
      ALTER TABLE account_payment_requests ADD COLUMN IF NOT EXISTS due_date DATE;
      ALTER TABLE account_payment_requests ADD COLUMN IF NOT EXISTS bill_number BIGSERIAL;
      ALTER TABLE account_payment_requests ADD COLUMN IF NOT EXISTS task_id BIGINT REFERENCES housework_tickets(id);
      ALTER TABLE account_payment_requests ADD COLUMN IF NOT EXISTS bill_property_id BIGINT REFERENCES properties(id);
      UPDATE account_payment_requests r SET payer_email=u.email,payer_name=btrim(u.first_name||' '||u.last_name) FROM users u WHERE u.id=r.payer_id AND r.payer_email='';
      UPDATE account_payment_requests r SET receiver_email=u.email,receiver_name=btrim(u.first_name||' '||u.last_name) FROM users u WHERE u.id=r.receiver_id AND r.receiver_email='';
      UPDATE account_payment_requests SET due_date=(created_at AT TIME ZONE 'America/Toronto')::date WHERE due_date IS NULL;
 CREATE INDEX IF NOT EXISTS account_payment_parties ON account_payment_requests(payer_id,receiver_id)").await
}
pub(crate) fn actor(auth: &AuthSession) -> ApiResult<&User> {
    auth.user
        .as_ref()
        .filter(|u| matches!(u.role.as_str(), "user" | "admin"))
        .ok_or((StatusCode::FORBIDDEN, "Payment account required.".into()))
}
#[derive(Serialize)]
pub struct Party {
    user_id: i64,
    lease_id: i64,
    name: String,
    property: String,
    ready: bool,
}
#[derive(Serialize)]
pub struct Activity {
    #[serde(skip_serializing)]
    sort_time: i64,
    id: String,
    kind: String,
    date: String,
    amount_cents: u64,
    status: String,
    direction: &'static str,
    counterparty: String,
    property: String,
    invoice_url: Option<String>,
    payable: bool,
    invoice_number: String,
    due_date: Option<String>,
    source_url: Option<String>,
}
#[derive(Serialize)]
pub struct Account {
    role: String,
    today: String,
    test_mode: bool,
    unavailable: bool,
    receive_status: &'static str,
    parties: Vec<Party>,
    rows: Vec<Activity>,
    leases: Vec<Value>,
}
// Both roles use the same relationship boundary. No arbitrary user directory or client-selected Stripe account.
const RELATIONSHIPS:&str="SELECT l.id,l.details->>'monthly_rent',COALESCE(p.data->>'name','Property'),
 b.id,btrim(b.first_name||' '||b.last_name), l.details->>'start_date',l.details->>'end_date',
 l.tenant_user_id=a.id AS outgoing,b.active AS contact_active
 FROM users a JOIN leases l ON (l.tenant_user_id=a.id OR EXISTS(SELECT 1 FROM resource_owners o WHERE o.resource_type='landlord' AND o.resource_id::text=l.landlord_snapshot #>> '{landlord,id}' AND o.user_id=a.id))
 JOIN properties p ON p.id=l.property_id
 JOIN users b ON b.id<>a.id AND
 ((l.tenant_user_id=a.id AND EXISTS(SELECT 1 FROM resource_owners o WHERE o.resource_type='landlord' AND o.resource_id::text=l.landlord_snapshot #>> '{landlord,id}' AND o.user_id=b.id))
 OR (EXISTS(SELECT 1 FROM resource_owners o WHERE o.resource_type='landlord' AND o.resource_id::text=l.landlord_snapshot #>> '{landlord,id}' AND o.user_id=a.id) AND b.id=l.tenant_user_id))
 WHERE a.id=$1 AND a.active AND l.status='confirmed' AND NOT l.is_demo AND NOT l.is_seeded";

pub async fn account(auth: AuthSession, State(db): State<Database>) -> ApiResult<Json<Account>> {
    let user = actor(&auth)?;
    let stripe = Stripe::configured();
    let mode = stripe.as_ref().is_some_and(Stripe::test_mode);
    let (relations, requests, own) = {
        let client = db.lock().await;
        let relations = client
            .query(RELATIONSHIPS, &[&user.id])
            .await
            .map_err(database_error)?;
        let requests=client.query("SELECT r.id::text,r.description,(r.created_at AT TIME ZONE 'America/Toronto')::date::text,r.amount_cents,r.status,
           (COALESCE(r.payer_id=$1,false) OR (r.payer_id IS NULL AND lower(r.payer_email)=lower($3))),
           CASE WHEN r.payer_id=$1 OR (r.payer_id IS NULL AND lower(r.payer_email)=lower($3)) THEN COALESCE(NULLIF(r.receiver_name,''),r.receiver_email) ELSE COALESCE(NULLIF(r.payer_name,''),r.payer_email) END,
           COALESCE(p.data->>'name',''),r.session_id,r.account_id,r.receipt_url,extract(epoch from r.created_at)::bigint,r.bill_number,r.due_date::text,r.task_id,r.lease_id
           FROM account_payment_requests r LEFT JOIN leases l ON l.id=r.lease_id LEFT JOIN properties p ON p.id=COALESCE(r.bill_property_id,l.property_id)
           WHERE (r.payer_id=$1 OR r.receiver_id=$1 OR r.creator_id=$1 OR (r.payer_id IS NULL AND lower(r.payer_email)=lower($3)) OR (r.receiver_id IS NULL AND lower(r.receiver_email)=lower($3))) AND r.test_mode=$2 AND (r.task_id IS NOT NULL OR r.status IN ('paid','processing','attention')) ORDER BY r.created_at DESC",&[&user.id,&mode,&user.email]).await.map_err(database_error)?;
        (
            relations,
            requests,
            for_user(&*client, user.id, mode).await?,
        )
    };
    let mut result = Account {
        role: user.role.clone(),
        today: chrono::Utc::now()
            .with_timezone(&chrono_tz::America::Toronto)
            .date_naive()
            .to_string(),
        test_mode: mode,
        unavailable: stripe.is_none(),
        receive_status: if own.as_ref().is_some_and(|a| a.ready) {
            "ready"
        } else if own.is_some() {
            "pending"
        } else {
            "new"
        },
        parties: vec![],
        rows: vec![],
        leases: vec![],
    };
    for relation in relations {
        let id: i64 = relation.get(0);
        let counter: i64 = relation.get(3);
        let outgoing: bool = relation.get(7);
        let property: String = relation.get(2);
        let name: String = relation.get(4);
        let ready = for_user(&*db.lock().await, counter, mode)
            .await?
            .is_some_and(|a| a.ready);
        if relation.get::<_, bool>(8) {
            result.parties.push(Party {
                user_id: counter,
                lease_id: id,
                name: name.clone(),
                property: property.clone(),
                ready,
            });
        }
        // Contact access can end without removing the lease’s financial history.
        if result
            .leases
            .iter()
            .any(|lease| lease["id"].as_i64() == Some(id))
        {
            continue;
        }
        result.leases.push(json!({"id":id,"property":property,"counterparty":name,"outgoing":outgoing,"amount_cents":relation.get::<_,Option<String>>(1).and_then(|s|billing::cents(&s)),"start_date":relation.get::<_,Option<String>>(5),"end_date":relation.get::<_,Option<String>>(6)}));
        // Existing rent and deposit receipts retain their authoritative source.
        match billing::payment_history(&db, id, user.id, outgoing).await {
            Ok(history) => {
                result.unavailable |= history.stripe_unavailable;
                for row in history.rows {
                    result.rows.push(Activity {
                        id: format!("lease-{id}-{}", row.id),
                        sort_time: chrono::NaiveDate::parse_from_str(&row.date, "%Y-%m-%d")
                            .ok()
                            .and_then(|d| d.and_hms_opt(0, 0, 0))
                            .map(|d| d.and_utc().timestamp())
                            .unwrap_or(0),
                        kind: match row.kind {
                            "rent_deposit" => "Rent deposit",
                            "key_deposit" => "Key deposit",
                            _ => "Monthly rent",
                        }
                        .into(),
                        date: row.date.clone(),
                        amount_cents: row.amount_cents,
                        status: row.status.into(),
                        direction: if outgoing { "out" } else { "in" },
                        counterparty: name.clone(),
                        property: property.clone(),
                        invoice_url: Some(format!("/api/payments/lease-{id}-{}/invoice", row.id)),
                        invoice_number: format!("LEASE-{id}-{}", row.id),
                        due_date: Some(row.date.clone()),
                        source_url: Some(if outgoing {
                            format!("/payments/settings/autopay?lease={id}")
                        } else {
                            format!("/leases/view?lease={id}")
                        }),
                        payable: false,
                    });
                }
            }
            Err(_) => result.unavailable = true,
        }
    }
    for row in requests {
        let id: String = row.get(0);
        let mut status: String = row.get(4);
        let mut invoice_url: Option<String> = row.get(10);
        if status != "canceled" && (status != "paid" || invoice_url.is_none()) {
            if let (Some(stripe), Some(session)) = (&stripe, row.get::<_, Option<String>>(8)) {
                match stripe
                    .for_account(row.get(9))
                    .request(
                        Method::GET,
                        &format!(
                            "checkout/sessions/{session}?expand[]=payment_intent.latest_charge"
                        ),
                        &[],
                        None,
                    )
                    .await
                {
                    Ok(session) => {
                        status = checkout_status(&session).into();
                        invoice_url = session["payment_intent"]["latest_charge"]["receipt_url"]
                            .as_str()
                            .filter(|s| {
                                reqwest::Url::parse(s).is_ok_and(|u| {
                                    u.scheme() == "https" && u.host_str() == Some("pay.stripe.com")
                                })
                            })
                            .map(str::to_owned);
                        let client = db.lock().await;
                        let saved=client.query_one("UPDATE account_payment_requests SET status=CASE WHEN status IN ('paid','canceled') THEN status ELSE $2 END,receipt_url=COALESCE($3,receipt_url) WHERE id=$1::text::uuid RETURNING status,receipt_url",&[&id,&status,&invoice_url]).await.map_err(database_error)?;
                        status = saved.get(0);
                        invoice_url = saved.get(1);
                    }
                    Err(_) => result.unavailable = true,
                }
            }
        }
        let date: String = row.get(2);
        let outgoing: bool = row.get(5);
        result.rows.push(Activity {
            sort_time: row.get(11),
            id: id.clone(),
            kind: row.get(1),
            date,
            amount_cents: row.get::<_, i64>(3) as u64,
            status: status.clone(),
            direction: if outgoing { "out" } else { "in" },
            counterparty: row.get(6),
            property: row.get(7),
            invoice_url: Some(format!("/api/payments/{id}/invoice")),
            invoice_number: format!("BRY-{:06}", row.get::<_, i64>(12)),
            due_date: row.get(13),
            source_url: if outgoing {
                row.get::<_, Option<i64>>(14)
                    .map(|id| format!("/tasks/{id}"))
            } else {
                None
            },
            payable: outgoing && status == "requested" && row.get::<_, Option<i64>>(14).is_some(),
        });
    }
    result
        .rows
        .sort_by(|a, b| b.sort_time.cmp(&a.sort_time).then_with(|| a.id.cmp(&b.id)));
    result.rows.dedup_by(|a, b| a.id == b.id);
    Ok(Json(result))
}
pub(crate) fn checkout_status(session: &Value) -> &'static str {
    if session["payment_status"] == "paid" {
        "paid"
    } else if session["payment_intent"]["status"] == "processing" {
        "processing"
    } else {
        "requested"
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewRequest {
    key: String,
    email: String,
    amount: String,
    due_date: Option<String>,
}
fn valid_bill_email(value: &str) -> bool {
    value.len() <= 254
        && !value
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || matches!(c, '<' | '>' | '"' | '\\'))
        && value.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty()
                && !domain.contains('@')
                && domain.contains('.')
                && !domain.starts_with('.')
                && !domain.ends_with('.')
        })
}
async fn owned_task<C: tokio_postgres::GenericClient + Sync>(
    client: &C,
    task_id: i64,
    user_id: i64,
) -> ApiResult<tokio_postgres::Row> {
    client.query_opt("SELECT h.title,h.property_id,h.status,COALESCE(p.data->>'name','Property') FROM housework_tickets h LEFT JOIN properties p ON p.id=h.property_id JOIN users u ON u.id=$2 AND u.active AND (h.creator_id=u.id OR buildry_property_can(u.id,p.id,'finances.write')) WHERE h.id=$1 FOR SHARE OF h", &[&task_id,&user_id]).await.map_err(database_error)?.ok_or((StatusCode::NOT_FOUND,"Task not found.".into()))
}
pub async fn task_bill_context(
    auth: AuthSession,
    State(db): State<Database>,
    Path(task_id): Path<i64>,
) -> ApiResult<Json<Value>> {
    let user = actor(&auth)?;
    let client = db.lock().await;
    let task = owned_task(&*client, task_id, user.id).await?;
    let mode = Stripe::configured().ok_or_else(unavailable)?.test_mode();
    let bills=client.query("SELECT id::text,bill_number,amount_cents,status FROM account_payment_requests WHERE task_id=$1 AND test_mode=$2 AND (payer_id=$3 OR creator_id=$3) ORDER BY created_at DESC",&[&task_id,&mode,&user.id]).await.map_err(database_error)?.into_iter().map(|r|json!({"id":r.get::<_,String>(0),"number":format!("BRY-{:06}",r.get::<_,i64>(1)),"amount_cents":r.get::<_,i64>(2),"status":r.get::<_,String>(3)})).collect::<Vec<_>>();
    Ok(Json(
        json!({"id":task_id,"title":task.get::<_,String>(0),"property":task.get::<_,String>(3),"can_create":task.get::<_,String>(2)!="aborted","bills":bills}),
    ))
}
pub async fn create_task_bill(
    Path(task_id): Path<i64>,
    auth: AuthSession,
    State(db): State<Database>,
    Json(input): Json<NewRequest>,
) -> ApiResult<Json<Value>> {
    let user = actor(&auth)?;
    let mode = Stripe::configured().ok_or_else(unavailable)?.test_mode();
    let id = uuid::Uuid::parse_str(&input.key)
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid bill reference.".into()))?
        .to_string();
    let email = input.email.trim().to_lowercase();
    if !valid_bill_email(&email) || email.eq_ignore_ascii_case(&user.email) {
        return Err((
            StatusCode::BAD_REQUEST,
            "Enter the other party’s email address.".into(),
        ));
    }
    let amount = billing::cents(&input.amount)
        .filter(|n| (100..=99999999).contains(n))
        .ok_or((
            StatusCode::BAD_REQUEST,
            "Enter an amount between $1 and $999,999.99.".into(),
        ))? as i64;
    let today = chrono::Utc::now()
        .with_timezone(&chrono_tz::America::Toronto)
        .date_naive();
    let due = input
        .due_date
        .as_deref()
        .filter(|v| !v.is_empty())
        .map(|v| chrono::NaiveDate::parse_from_str(v, "%Y-%m-%d"))
        .transpose()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Choose a valid due date.".into()))?
        .unwrap_or(today)
        .to_string();
    let mut client = db.lock().await;
    let client = client.transaction().await.map_err(database_error)?;
    let task = owned_task(&client, task_id, user.id).await?;
    if task.get::<_, String>(2) == "aborted" {
        return Err((
            StatusCode::CONFLICT,
            "Canceled tasks cannot create bills.".into(),
        ));
    }
    let description = format!("Task: {}", task.get::<_, String>(0));
    let property_id: i64 = task.get(1);
    let own_name: String = client
        .query_one(
            "SELECT COALESCE(NULLIF(l.legal_name,''),btrim(u.first_name||' '||u.last_name)) FROM users u LEFT JOIN landlords l ON l.id=u.landlord_id WHERE u.id=$1",
            &[&user.id],
        )
        .await
        .map_err(database_error)?
        .get(0);
    let other=client.query_opt("SELECT u.id,COALESCE(NULLIF(l.legal_name,''),btrim(u.first_name||' '||u.last_name)) FROM users u LEFT JOIN landlords l ON l.id=u.landlord_id WHERE lower(u.email)=$1 AND u.active", &[&email]).await.map_err(database_error)?;
    let other_id = other.as_ref().map(|r| r.get::<_, i64>(0));
    let other_name = other
        .as_ref()
        .map(|r| r.get::<_, String>(1))
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| email.clone());
    let (payer, receiver, payer_email, receiver_email, payer_name, receiver_name) = (
        Some(user.id),
        other_id,
        user.email.to_lowercase(),
        email,
        own_name,
        other_name,
    );
    client.execute("INSERT INTO account_payment_requests(id,creator_id,payer_id,receiver_id,amount_cents,description,test_mode,payer_email,receiver_email,payer_name,receiver_name,due_date,task_id,bill_property_id)
       VALUES($1::text::uuid,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12::text::date,$13,$14) ON CONFLICT DO NOTHING", &[&id,&user.id,&payer,&receiver,&amount,&description,&mode,&payer_email,&receiver_email,&payer_name,&receiver_name,&due,&task_id,&property_id]).await.map_err(database_error)?;
    let saved=client.query_opt("SELECT payer_email,receiver_email,amount_cents,description,due_date::text,task_id FROM account_payment_requests WHERE id=$1::text::uuid AND creator_id=$2 AND test_mode=$3", &[&id,&user.id,&mode]).await.map_err(database_error)?.ok_or((StatusCode::CONFLICT,"Bill reference already used.".into()))?;
    if saved.get::<_, String>(0) != payer_email
        || saved.get::<_, String>(1) != receiver_email
        || saved.get::<_, i64>(2) != amount
        || saved.get::<_, String>(3) != description
        || saved.get::<_, String>(4) != due
        || saved.get::<_, Option<i64>>(5) != Some(task_id)
    {
        return Err((
            StatusCode::CONFLICT,
            "This bill has already been created. Start a new bill to change its details.".into(),
        ));
    }
    client.commit().await.map_err(database_error)?;
    Ok(Json(json!({"id":id})))
}
pub async fn checkout(
    auth: AuthSession,
    State(db): State<Database>,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    let user = actor(&auth)?;
    let stripe = Stripe::configured().ok_or_else(unavailable)?;
    let publishable_key = stripe.publishable_key.clone().ok_or_else(unavailable)?;
    uuid::Uuid::parse_str(&id).map_err(|_| (StatusCode::NOT_FOUND, "Payment not found.".into()))?;
    let mut client = db.lock().await;
    let tx = client.transaction().await.map_err(database_error)?;
    let row=tx.query_opt("SELECT COALESCE(receiver_id,(SELECT id FROM users WHERE lower(email)=lower(receiver_email) AND active LIMIT 1)),amount_cents,description,status,session_id,account_id,bill_number FROM account_payment_requests WHERE id=$1::text::uuid AND (payer_id=$2 OR (payer_id IS NULL AND lower(payer_email)=lower($4))) AND test_mode=$3 AND task_id IS NOT NULL FOR UPDATE",&[&id,&user.id,&stripe.test_mode(),&user.email]).await.map_err(database_error)?.ok_or((StatusCode::NOT_FOUND,"Payment not found.".into()))?;
    if row.get::<_, String>(3) != "requested" {
        return Err((
            StatusCode::CONFLICT,
            "This payment is already processing or complete.".into(),
        ));
    }
    let receiver = row.get::<_, Option<i64>>(0).ok_or((
        StatusCode::CONFLICT,
        "The recipient needs to set up their Buildry payout account.".into(),
    ))?;
    let account = for_user(&tx, receiver, stripe.test_mode())
        .await?
        .filter(|a| a.ready)
        .ok_or((
            StatusCode::CONFLICT,
            "The recipient needs to finish payout setup.".into(),
        ))?;
    if !crate::stripe_connect::refresh_account(&tx, &stripe, &account.id).await? {
        return Err((
            StatusCode::CONFLICT,
            "The recipient needs to finish payout setup.".into(),
        ));
    }
    let scoped = stripe.for_account(Some(account.id.clone()));
    let session = if let Some(session) = row.get::<_, Option<String>>(4) {
        if row.get::<_, Option<String>>(5).as_deref() != Some(&account.id) {
            return Err((
                StatusCode::CONFLICT,
                "The recipient’s payout account changed.".into(),
            ));
        }
        scoped
            .request(
                Method::GET,
                &format!("checkout/sessions/{session}"),
                &[],
                None,
            )
            .await?
    } else {
        let result = scoped
            .request(
                Method::POST,
                "checkout/sessions",
                &[
                    ("mode", "payment".into()),
                    ("ui_mode", "elements".into()),
                    ("allowed_payment_method_types[0]", "card".into()),
                    ("line_items[0][price_data][currency]", "cad".into()),
                    (
                        "line_items[0][price_data][unit_amount]",
                        row.get::<_, i64>(1).to_string(),
                    ),
                    ("line_items[0][price_data][product_data][name]", row.get(2)),
                    ("line_items[0][quantity]", "1".into()),
                    ("customer_email", user.email.clone()),
                    ("return_url", format!("{}/payments", stripe.origin)),
                    ("metadata[buildry_request_id]", id.clone()),
                    (
                        "payment_intent_data[metadata][buildry_request_id]",
                        id.clone(),
                    ),
                ],
                Some(&format!("account-bill-v2-{id}")),
            )
            .await?;
        let session = result["id"].as_str().ok_or_else(unavailable)?;
        tx.execute("UPDATE account_payment_requests SET session_id=$2,account_id=$3,payer_id=COALESCE(payer_id,$4),receiver_id=COALESCE(receiver_id,$5) WHERE id=$1::text::uuid",&[&id,&session,&account.id,&user.id,&receiver]).await.map_err(database_error)?;
        result
    };
    if session["status"] == "expired" {
        return Err((
            StatusCode::CONFLICT,
            "Payment session expired. Create a new payment.".into(),
        ));
    }
    let secret = session["client_secret"].as_str().ok_or_else(unavailable)?;
    tx.commit().await.map_err(database_error)?;
    Ok(Json(
        json!({"client_secret":secret,"publishable_key":publishable_key,"stripe_account":account.id}),
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn task_bill_payload_cannot_override_source_or_parties() {
        let value = json!({"key":"a","email":"worker@example.test","amount":"25.00","due_date":"2026-11-07"});
        assert!(serde_json::from_value::<NewRequest>(value.clone()).is_ok());
        for field in [
            "direction",
            "description",
            "task_id",
            "payer_id",
            "stripe_account",
            "lease_id",
        ] {
            let mut extra = value.clone();
            extra[field] = json!("injected");
            assert!(serde_json::from_value::<NewRequest>(extra).is_err());
        }
        assert!(valid_bill_email("worker@example.test"));
        assert!(!valid_bill_email("bad@<example.test>"));
        assert!(!valid_bill_email("two words@example.test"));
    }
    #[test]
    fn checkout_does_not_treat_submission_as_settlement() {
        assert_eq!(
            checkout_status(
                &json!({"status":"complete","payment_status":"unpaid","payment_intent":{"status":"processing"}})
            ),
            "processing"
        );
        assert_eq!(
            checkout_status(&json!({"status":"complete","payment_status":"unpaid"})),
            "requested"
        );
        assert_eq!(checkout_status(&json!({"payment_status":"paid"})), "paid");
    }
}

pub async fn cancel(
    auth: AuthSession,
    State(db): State<Database>,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    let user = actor(&auth)?;
    let stripe = Stripe::configured().ok_or_else(unavailable)?;
    uuid::Uuid::parse_str(&id).map_err(|_| (StatusCode::NOT_FOUND, "Request not found.".into()))?;
    let mut client = db.lock().await;
    let tx = client.transaction().await.map_err(database_error)?;
    let row=tx.query_opt("SELECT status,session_id,account_id FROM account_payment_requests WHERE id=$1::text::uuid AND (payer_id=$2 OR receiver_id=$2 OR creator_id=$2 OR (payer_id IS NULL AND lower(payer_email)=lower($4)) OR (receiver_id IS NULL AND lower(receiver_email)=lower($4))) AND test_mode=$3 FOR UPDATE",&[&id,&user.id,&stripe.test_mode(),&user.email]).await.map_err(database_error)?.ok_or((StatusCode::NOT_FOUND,"Request not found.".into()))?;
    if row.get::<_, String>(0) != "requested" {
        return Err((
            StatusCode::CONFLICT,
            "Only unpaid requests can be canceled.".into(),
        ));
    }
    if let Some(session) = row.get::<_, Option<String>>(1) {
        let scoped = stripe.for_account(row.get(2));
        let value = scoped
            .request(
                Method::GET,
                &format!("checkout/sessions/{session}"),
                &[],
                None,
            )
            .await?;
        if value["status"] == "complete" {
            return Err((
                StatusCode::CONFLICT,
                "Payment has already been submitted. Refresh to see its status.".into(),
            ));
        }
        if value["status"] == "open" {
            scoped
                .request(
                    Method::POST,
                    &format!("checkout/sessions/{session}/expire"),
                    &[],
                    None,
                )
                .await?;
        }
    }
    tx.execute(
        "UPDATE account_payment_requests SET status='canceled' WHERE id=$1::text::uuid",
        &[&id],
    )
    .await
    .map_err(database_error)?;
    tx.commit().await.map_err(database_error)?;
    Ok(StatusCode::NO_CONTENT)
}
