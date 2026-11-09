use crate::{
    auth::{AuthSession, Database},
    database_error,
    leasing::LeaseDetails,
    tenants::tenant_id,
    ApiResult,
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::{Datelike, NaiveDate, Utc};
use reqwest::{Method, Url};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{env, sync::LazyLock, time::Duration};
use tokio_postgres::{GenericClient, Row};

const UNAVAILABLE: &str = "Payments are temporarily unavailable.";
const RETURN_PATH: &str = "/my-home/payments";
static HTTP: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("Stripe HTTP client")
});

#[derive(Clone)]
pub(crate) struct Stripe {
    key: String,
    pub(crate) origin: String,
    account: Option<String>,
    portal: Option<String>,
    pub(crate) publishable_key: Option<String>,
}

impl Stripe {
    pub(crate) fn configured() -> Option<Self> {
        let key = env::var("STRIPE_SECRET_KEY").ok()?;
        let origin = Url::parse(&env::var("APP_PUBLIC_URL").ok()?).ok()?;
        let local_test = key.starts_with("sk_test_")
            && origin.scheme() == "http"
            && matches!(origin.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
        if !(key.starts_with("sk_test_") || key.starts_with("sk_live_"))
            || !(origin.scheme() == "https" || local_test)
            || !origin.username().is_empty()
            || origin.password().is_some()
            || origin.path() != "/"
            || origin.query().is_some()
            || origin.fragment().is_some()
        {
            return None;
        }
        let prefix = if local_test || key.starts_with("sk_test_") {
            "pk_test_"
        } else {
            "pk_live_"
        };
        let publishable_key = env::var("STRIPE_PUBLISHABLE_KEY")
            .ok()
            .filter(|value| value.starts_with(prefix));
        Some(Self {
            key,
            publishable_key,
            origin: origin.origin().ascii_serialization(),
            account: None,
            portal: env::var("STRIPE_PORTAL_CONFIGURATION")
                .ok()
                .filter(|id| id.starts_with("bpc_")),
        })
    }

    pub(crate) fn test_mode(&self) -> bool {
        self.key.starts_with("sk_test_")
    }

    pub(crate) fn for_account(&self, account: Option<String>) -> Self {
        Self {
            account,
            ..self.clone()
        }
    }

    pub(crate) async fn request(
        &self,
        method: Method,
        path: &str,
        form: &[(&str, String)],
        idempotency: Option<&str>,
    ) -> ApiResult<Value> {
        let mut request = HTTP
            .request(method, format!("https://api.stripe.com/v1/{path}"))
            .bearer_auth(&self.key)
            .header("Stripe-Version", "2026-10-28.endive");
        if let Some(account) = &self.account {
            request = request.header("Stripe-Account", account);
        }
        if !form.is_empty() {
            request = request.form(form);
        }
        if let Some(key) = idempotency {
            request = request.header("Idempotency-Key", key);
        }
        let response = request.send().await.map_err(|_| unavailable())?;
        let status = response.status();
        let value: Value = response.json().await.map_err(|_| unavailable())?;
        if !status.is_success() {
            eprintln!(
                "Stripe request failed ({status}): {}",
                value["error"]["code"]
                    .as_str()
                    .or_else(|| value["error"]["param"].as_str())
                    .unwrap_or("unknown error")
            );
            if status == StatusCode::BAD_REQUEST
                && (path == "accounts" || path.starts_with("accounts/"))
            {
                let field = value["error"]["param"].as_str().unwrap_or("payout details");
                let field: String = field
                    .chars()
                    .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '[' | ']' | '_' | ' '))
                    .take(100)
                    .collect();
                return Err((
                    StatusCode::BAD_REQUEST,
                    format!("Stripe could not accept {field}. Check the details and try again."),
                ));
            }
            return Err(unavailable());
        }
        Ok(value)
    }

    async fn session(&self, id: &str) -> ApiResult<Value> {
        if id.starts_with("sub_") {
            let mut subscription = self.request(Method::GET,
                &format!("subscriptions/{id}?expand[]=pending_setup_intent&expand[]=default_payment_method"), &[], None).await?;
            if let Some(invoice) = subscription["latest_invoice"].as_str() {
                subscription["latest_invoice"] = self
                    .request(
                        Method::GET,
                        &format!(
                            "invoices/{invoice}?expand[]=payments.data.payment.payment_intent"
                        ),
                        &[],
                        None,
                    )
                    .await?;
            }
            return Ok(serde_json::json!({
                "id": id, "status": "complete", "pad": true,
                "customer": subscription["customer"],
                "subscription": subscription,
            }));
        }
        self.request(
            Method::GET,
            &format!("checkout/sessions/{id}?expand[]=subscription"),
            &[],
            None,
        )
        .await
    }
}

pub(crate) fn unavailable() -> (StatusCode, String) {
    (StatusCode::SERVICE_UNAVAILABLE, UNAVAILABLE.into())
}

pub async fn migrate(db: &tokio_postgres::Client) -> Result<(), tokio_postgres::Error> {
    db.batch_execute(
        "CREATE TABLE IF NOT EXISTS lease_stripe_checkout (
            lease_id BIGINT PRIMARY KEY REFERENCES leases(id),
            attempt TEXT NOT NULL,
            form JSONB NOT NULL,
            expires_at BIGINT NOT NULL,
            session_id TEXT UNIQUE,
            test_mode BOOLEAN NOT NULL
        );
        ALTER TABLE lease_stripe_checkout ADD COLUMN IF NOT EXISTS connected_account TEXT;
        CREATE TABLE IF NOT EXISTS lease_stripe_past_subscriptions (
            lease_id BIGINT NOT NULL REFERENCES leases(id),
            subscription_id TEXT NOT NULL,
            connected_account TEXT,
            test_mode BOOLEAN NOT NULL,
            PRIMARY KEY (lease_id, subscription_id)
        );
        ALTER TABLE leases ADD COLUMN IF NOT EXISTS rent_autopay_paused BOOLEAN NOT NULL DEFAULT FALSE;
        CREATE TABLE IF NOT EXISTS lease_deposit_receipts (
            lease_id BIGINT NOT NULL REFERENCES leases(id),
            kind TEXT NOT NULL CHECK (kind IN ('rent','key')),
            received_on DATE NOT NULL,
            recorded_by BIGINT NOT NULL REFERENCES users(id),
            PRIMARY KEY (lease_id,kind)
        )",
    )
    .await
}

const LEASE_QUERY: &str = "SELECT l.details, l.tenant_email,
    l.landlord_snapshot #>> '{landlord,id}', l.tenancy_ended_at IS NOT NULL, (l.is_demo OR l.is_seeded),
    c.attempt,c.form,c.expires_at,c.session_id,c.test_mode,l.rent_autopay_paused,c.connected_account
    FROM leases l JOIN users u ON lower(u.email)=lower(l.tenant_email)
    LEFT JOIN lease_stripe_checkout c ON c.lease_id=l.id
    WHERE l.id=$1 AND u.id=$2 AND l.status='confirmed'";

pub(crate) fn cents(value: &str) -> Option<u32> {
    let (whole, fractional) = value.trim().split_once('.').unwrap_or((value.trim(), ""));
    let whole = whole.parse::<u32>().ok()?;
    let fractional = match fractional.len() {
        0 => 0,
        1 => fractional.parse::<u32>().ok()? * 10,
        2 => fractional.parse::<u32>().ok()?,
        _ => return None,
    };
    whole.checked_mul(100)?.checked_add(fractional)
}

fn rent_terms(row: &Row) -> Option<(u32, u32, NaiveDate)> {
    if row.get::<_, bool>(3) || row.get::<_, bool>(4) {
        return None;
    }
    let details: LeaseDetails = serde_json::from_value(row.get(0)).ok()?;
    if !matches!(details.term_type.as_str(), "fixed" | "month-to-month")
        || [&details.rent_discount, &details.partial_rent]
            .iter()
            .any(|value| !value.trim().is_empty() && cents(value) != Some(0))
    {
        return None;
    }
    let amount =
        cents(&details.monthly_rent).filter(|amount| (50..=99_999_999).contains(amount))?;
    let day = details
        .rent_due_day
        .parse::<u32>()
        .ok()
        .filter(|day| (1..=28).contains(day))?;
    let start = NaiveDate::parse_from_str(&details.start_date, "%Y-%m-%d").ok()?;
    Some((amount, day, start))
}

fn next_due(after: chrono::DateTime<Utc>, day: u32) -> i64 {
    let date = after
        .with_timezone(&chrono_tz::America::Toronto)
        .date_naive();
    let candidate = NaiveDate::from_ymd_opt(date.year(), date.month(), day).unwrap();
    let noon_utc = |date: NaiveDate| date.and_hms_opt(16, 0, 0).unwrap().and_utc();
    let due = if noon_utc(candidate) > after {
        candidate
    } else {
        candidate
            .checked_add_months(chrono::Months::new(1))
            .unwrap()
    };
    noon_utc(due).timestamp()
}

fn enrollment(row: &Row) -> Option<(u32, u32, i64, i64)> {
    let (amount, day, start) = rent_terms(row)?;
    let now = Utc::now();
    // Give Checkout 40 minutes, and leave a buffer before the billing anchor.
    let expiry = now + chrono::TimeDelta::minutes(40);
    let due = next_due(now, day);
    let due_date = chrono::DateTime::from_timestamp(due, 0)?.date_naive();
    (due > expiry.timestamp() + 60 && due_date >= start).then_some((
        amount,
        day,
        due,
        expiry.timestamp(),
    ))
}

#[derive(Serialize)]
pub struct Billing {
    state: &'static str,
    test_mode: bool,
    amount_cents: Option<u32>,
    first_charge: Option<String>,
    next_charge: Option<String>,
    can_enroll: bool,
    can_manage: bool,
    can_stop: bool,
    can_pad: bool,
    can_card: bool,
    method: Option<&'static str>,
    bank_last4: Option<String>,
}

fn is_stopped(subscription: &Value) -> bool {
    matches!(
        subscription["status"].as_str(),
        Some("canceled" | "incomplete_expired")
    )
}

pub(crate) fn subscription_state(session: &Value) -> &'static str {
    let subscription = &session["subscription"];
    if is_stopped(subscription) {
        return "off";
    }
    if session["pad"] == true {
        match subscription["pending_setup_intent"]["status"].as_str() {
            Some("requires_action") => return "verifying",
            Some("requires_payment_method" | "requires_confirmation" | "processing") => {
                return "pending"
            }
            Some("canceled") => return "attention",
            _ => {}
        }
        if subscription["default_payment_method"].is_null() {
            return "attention";
        }
        if invoice_processing(&subscription["latest_invoice"]) {
            return "processing";
        }
        if subscription["latest_invoice"]["status"] == "open"
            && subscription["latest_invoice"]["attempted"] == true
            && subscription["latest_invoice"]["amount_due"]
                .as_u64()
                .unwrap_or(0)
                > 0
        {
            return "attention";
        }
    }
    match subscription["status"].as_str() {
        Some("active" | "trialing") => "active",
        _ => "attention",
    }
}

fn invoice_processing(invoice: &Value) -> bool {
    invoice["payments"]["data"]
        .as_array()
        .is_some_and(|payments| {
            payments
                .iter()
                .any(|payment| payment["payment"]["payment_intent"]["status"] == "processing")
        })
}

fn session_matches_mode(row: &Row, stripe: &Stripe) -> ApiResult<()> {
    if row
        .get::<_, Option<bool>>(9)
        .is_some_and(|mode| mode != stripe.key.starts_with("sk_test_"))
    {
        return Err(unavailable());
    }
    Ok(())
}

pub async fn status(
    auth: AuthSession,
    State(db): State<Database>,
    Path(id): Path<i64>,
) -> ApiResult<Json<Option<Billing>>> {
    let user = tenant_id(auth)?;
    let Some(stripe) = Stripe::configured() else {
        return Ok(Json(None));
    };
    let (row, connected) = {
        let client = db.lock().await;
        let row = client
            .query_opt(LEASE_QUERY, &[&id, &user])
            .await
            .map_err(database_error)?
            .ok_or((StatusCode::NOT_FOUND, "Tenancy not found.".into()))?;
        let connected =
            crate::stripe_connect::for_landlord(&*client, row.get(2), stripe.test_mode()).await?;
        (row, connected)
    };
    let stripe = stripe.for_account(row.get(11));
    let paused: bool = row.get(10);
    let plan = connected
        .as_ref()
        .filter(|account| account.ready)
        .and_then(|_| enrollment(&row));
    let attempt: Option<String> = row.get(5);
    if attempt.is_none() && plan.is_none() && !paused {
        return Ok(Json(None));
    }
    session_matches_mode(&row, &stripe)?;
    let mut billing = Billing {
        state: if paused { "paused" } else { "off" },
        test_mode: stripe.key.starts_with("sk_test_"),
        amount_cents: rent_terms(&row).map(|(amount, _, _)| amount),
        first_charge: plan
            .and_then(|(_, _, due, _)| chrono::DateTime::from_timestamp(due, 0))
            .map(|date| date.format("%b %-d, %Y").to_string()),
        next_charge: None,
        can_enroll: plan.is_some() && !paused,
        can_manage: false,
        can_stop: false,
        can_pad: false,
        can_card: plan.is_some(),
        method: None,
        bank_last4: None,
    };
    if let Some(account) = connected.as_ref().filter(|_| plan.is_some()) {
        if let Ok(value) = stripe
            .for_account(None)
            .request(Method::GET, &format!("accounts/{}", account.id), &[], None)
            .await
        {
            billing.can_pad = value["capabilities"]["acss_debit_payments"] == "active";
            billing.can_card = value["capabilities"]["card_payments"] == "active";
        }
    }
    if let Some(id) = row.get::<_, Option<String>>(8) {
        let session = stripe.session(&id).await?;
        let subscription = &session["subscription"];
        billing.bank_last4 = subscription["default_payment_method"]["acss_debit"]["last4"]
            .as_str()
            .map(str::to_string);
        billing.method = Some(if session["pad"] == true {
            "acss_debit"
        } else {
            "card"
        });
        billing.can_manage = session["pad"] != true
            && (stripe.account.is_some() || stripe.portal.is_some())
            && session["customer"].is_string();
        if session["status"] == "complete" {
            if !is_stopped(subscription) {
                billing.state = subscription_state(&session);
                billing.can_stop = true;
                billing.can_enroll = matches!(billing.state, "pending" | "verifying");
                if matches!(billing.state, "active" | "processing") {
                    billing.next_charge = subscription["items"]["data"][0]["current_period_end"]
                        .as_i64()
                        .and_then(|value| chrono::DateTime::from_timestamp(value, 0))
                        .map(|date| {
                            date.with_timezone(&chrono_tz::America::Toronto)
                                .format("%b %-d, %Y")
                                .to_string()
                        });
                }
            }
        } else if session["status"] == "open" && !row.get::<_, bool>(3) && !paused {
            billing.state = "pending";
            billing.can_enroll = true;
            billing.can_stop = true;
        }
        if matches!(
            billing.state,
            "active" | "pending" | "attention" | "verifying" | "processing"
        ) {
            let form: Value = row.get(6);
            billing.amount_cents = form["line_items[0][price_data][unit_amount]"]
                .as_str()
                .and_then(|v| v.parse().ok());
            billing.first_charge = form["subscription_data[billing_cycle_anchor]"]
                .as_str()
                .and_then(|v| chrono::DateTime::from_timestamp(v.parse().ok()?, 0))
                .map(|date| date.format("%b %-d, %Y").to_string());
        }
    } else if attempt.is_some() && !row.get::<_, bool>(3) && !paused {
        billing.state = "pending";
        billing.can_enroll = true;
        billing.can_stop = true;
    }
    Ok(Json(Some(billing)))
}

#[derive(Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Enroll,
    EnrollPad,
    Manage,
    Stop,
}

#[derive(Serialize)]
pub struct BillingAction {
    url: Option<String>,
    elements: Option<CheckoutElements>,
}

#[derive(Serialize)]
struct CheckoutElements {
    client_secret: String,
    publishable_key: String,
    stripe_account: Option<String>,
    kind: &'static str,
}

impl BillingAction {
    fn checkout(stripe: &Stripe, session: &Value) -> ApiResult<Self> {
        if session["pad"] == true {
            let setup = &session["subscription"]["pending_setup_intent"];
            let kind = if setup["status"] == "requires_action" {
                "pad_verify"
            } else {
                "pad"
            };
            return Ok(Self {
                url: None,
                elements: Some(CheckoutElements {
                    client_secret: setup["client_secret"]
                        .as_str()
                        .ok_or_else(unavailable)?
                        .into(),
                    publishable_key: stripe.publishable_key.clone().ok_or_else(unavailable)?,
                    stripe_account: stripe.account.clone(),
                    kind,
                }),
            });
        }

        if session["ui_mode"] == "elements" {
            return Ok(Self {
                url: None,
                elements: Some(CheckoutElements {
                    client_secret: session["client_secret"]
                        .as_str()
                        .ok_or_else(unavailable)?
                        .into(),
                    publishable_key: stripe.publishable_key.clone().ok_or_else(unavailable)?,
                    stripe_account: stripe.account.clone(),
                    kind: "card",
                }),
            });
        }
        Ok(Self {
            url: Some(stripe_url(session, "checkout.stripe.com")?),
            elements: None,
        })
    }
}

type CheckoutForm = std::collections::BTreeMap<String, String>;

fn checkout_form(
    row: &Row,
    stripe: &Stripe,
    id: i64,
    amount: u32,
    day: u32,
    due: i64,
    expiry: i64,
) -> CheckoutForm {
    let email: String = row.get(1);
    let return_url = format!("{}{RETURN_PATH}?lease={id}", stripe.origin);
    let mut fields: CheckoutForm = [
        ("mode", "subscription".into()),
        ("success_url", return_url.clone()),
        ("cancel_url", return_url),
        ("customer_email", email),
        ("client_reference_id", id.to_string()),
        ("allowed_payment_method_types[0]", "card".into()),
        ("payment_method_collection", "always".into()),
        ("adaptive_pricing[enabled]", "false".into()),
        ("automatic_tax[enabled]", "false".into()),
        ("line_items[0][quantity]", "1".into()),
        ("line_items[0][price_data][currency]", "cad".into()),
        ("line_items[0][price_data][unit_amount]", amount.to_string()),
        (
            "line_items[0][price_data][recurring][interval]",
            "month".into(),
        ),
        (
            "line_items[0][price_data][product_data][name]",
            "Monthly residential rent".into(),
        ),
        ("subscription_data[billing_cycle_anchor]", due.to_string()),
        ("subscription_data[proration_behavior]", "none".into()),
        (
            "subscription_data[metadata][buildry_lease_id]",
            id.to_string(),
        ),
        ("expires_at", expiry.to_string()),
        ("branding_settings[display_name]", "Buildry".into()),
        ("branding_settings[button_color]", "#236c4b".into()),
        ("branding_settings[background_color]", "#f6f8f7".into()),
        ("branding_settings[border_style]", "rounded".into()),
        (
            "custom_text[submit][message]",
            format!(
                "Authorize monthly rent charges on day {day}. Residential rent is GST/HST-exempt."
            ),
        ),
    ]
    .into_iter()
    .map(|(key, value)| (key.into(), value))
    .collect();
    if stripe.publishable_key.is_some() {
        for key in [
            "success_url",
            "cancel_url",
            "branding_settings[display_name]",
            "branding_settings[button_color]",
            "branding_settings[background_color]",
            "branding_settings[border_style]",
            "custom_text[submit][message]",
        ] {
            fields.remove(key);
        }
        fields.insert("ui_mode".into(), "elements".into());
        fields.insert(
            "return_url".into(),
            format!("{}{RETURN_PATH}?lease={id}", stripe.origin),
        );
    }
    fields
}

async fn lock_tenancy(tx: &tokio_postgres::Transaction<'_>, id: i64, user: i64) -> ApiResult<Row> {
    // Read the Checkout attempt after acquiring the lease lock so a waiting
    // transaction observes any attempt committed by the previous holder.
    let missing = || (StatusCode::NOT_FOUND, "Tenancy not found.".into());
    tx.query_opt(
        "SELECT l.id FROM leases l JOIN users u ON lower(u.email)=lower(l.tenant_email)
         WHERE l.id=$1 AND u.id=$2 AND l.status='confirmed' FOR UPDATE OF l",
        &[&id, &user],
    )
    .await
    .map_err(database_error)?
    .ok_or_else(missing)?;
    tx.query_opt(LEASE_QUERY, &[&id, &user])
        .await
        .map_err(database_error)?
        .ok_or_else(missing)
}

pub async fn change(
    auth: AuthSession,
    State(db): State<Database>,
    Path(id): Path<i64>,
    Json(action): Json<Action>,
) -> ApiResult<Json<BillingAction>> {
    let user = tenant_id(auth)?;
    let platform = Stripe::configured().ok_or_else(unavailable)?;
    let mut db = db.lock().await;
    let tx = db.transaction().await.map_err(database_error)?;
    let row = lock_tenancy(&tx, id, user).await?;
    let stripe = platform.for_account(row.get(11));
    session_matches_mode(&row, &stripe)?;
    let session_id: Option<String> = row.get(8);
    let session = match &session_id {
        Some(id) => stripe.session(id).await?,
        None => Value::Null,
    };
    if action == Action::Stop {
        stop_session(&stripe, &session).await?;
        if session_id.is_none() {
            tx.execute(
                "DELETE FROM lease_stripe_checkout WHERE lease_id=$1",
                &[&id],
            )
            .await
            .map_err(database_error)?;
        }
        tx.commit().await.map_err(database_error)?;
        return Ok(Json(BillingAction {
            url: None,
            elements: None,
        }));
    }
    if action == Action::Manage {
        let customer = session["customer"].as_str().ok_or_else(unavailable)?;
        let portal = match &stripe.account {
            Some(account) => {
                crate::stripe_connect::portal_configuration(&tx, &stripe, account).await?
            }
            None => stripe.portal.clone().ok_or_else(unavailable)?,
        };
        let result = stripe
            .request(
                Method::POST,
                "billing_portal/sessions",
                &[
                    ("customer", customer.into()),
                    ("configuration", portal),
                    (
                        "return_url",
                        format!("{}{RETURN_PATH}?lease={id}", stripe.origin),
                    ),
                ],
                None,
            )
            .await?;
        let url = stripe_url(&result, "billing.stripe.com")?;
        tx.commit().await.map_err(database_error)?;
        return Ok(Json(BillingAction {
            url: Some(url),
            elements: None,
        }));
    }
    if row.get::<_, bool>(3) || row.get::<_, bool>(10) {
        return Err((
            StatusCode::CONFLICT,
            "Autopay setup is closed for this lease.".into(),
        ));
    }
    if session["pad"] == true
        && !is_stopped(&session["subscription"])
        && matches!(subscription_state(&session), "pending" | "verifying")
    {
        return BillingAction::checkout(&stripe, &session).map(Json);
    }
    if session["status"] == "open" {
        return BillingAction::checkout(&stripe, &session).map(Json);
    }
    if session["status"] == "complete" && !is_stopped(&session["subscription"]) {
        return Err((StatusCode::CONFLICT, "Autopay is already set up.".into()));
    }
    let attempt: Option<String> = row.get(5);
    let pending_attempt = attempt.filter(|_| {
        session_id.is_none() && row.get::<_, i64>(7) > Utc::now().timestamp() + 30 * 60
    });
    let attempt = if let Some(attempt) = pending_attempt {
        attempt
    } else {
        let (amount, day, due, expiry) = enrollment(&row).ok_or((
            StatusCode::CONFLICT,
            "Autopay setup is unavailable for this rent cycle.".into(),
        ))?;
        let account = crate::stripe_connect::for_landlord(&tx, row.get(2), platform.test_mode())
            .await?
            .ok_or((
                StatusCode::CONFLICT,
                "The landlord hasn’t set up Stripe payouts.".into(),
            ))?;
        if !crate::stripe_connect::refresh_account(&tx, &platform, &account.id).await? {
            tx.commit().await.map_err(database_error)?;
            return Err((
                StatusCode::CONFLICT,
                "The landlord’s Stripe account is not ready for payments.".into(),
            ));
        }
        let target = platform.for_account(Some(account.id));
        if action == Action::EnrollPad {
            crate::pad::ensure_capability(
                &platform,
                target.account.as_deref().ok_or_else(unavailable)?,
            )
            .await?;
        }
        if let Some(subscription) = session["subscription"]["id"].as_str() {
            tx.execute(
                "INSERT INTO lease_stripe_past_subscriptions(lease_id,subscription_id,connected_account,test_mode)
                 VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING",
                &[&id, &subscription, &stripe.account, &platform.test_mode()],
            ).await.map_err(database_error)?;
        }
        let attempt = uuid::Uuid::new_v4().to_string();
        let mut fields = checkout_form(&row, &target, id, amount, day, due, expiry);
        if action == Action::EnrollPad {
            fields.insert("buildry_payment_method".into(), "acss_debit".into());
        }
        if action != Action::EnrollPad && stripe.account == target.account {
            if let Some(customer) = session["customer"].as_str() {
                fields.remove("customer_email");
                fields.insert("customer".into(), customer.into());
            }
        }
        let form = serde_json::json!(fields);
        tx.execute(
            "INSERT INTO lease_stripe_checkout(lease_id,attempt,form,expires_at,test_mode,connected_account)
             VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(lease_id) DO UPDATE SET
             attempt=$2,form=$3,expires_at=$4,session_id=NULL,test_mode=$5,connected_account=$6",
            &[
                &id,
                &attempt,
                &form,
                &expiry,
                &platform.test_mode(),
                &target.account,
            ],
        )
        .await
        .map_err(database_error)?;
        attempt
    };
    tx.commit().await.map_err(database_error)?;
    let tx = db.transaction().await.map_err(database_error)?;
    let row = lock_tenancy(&tx, id, user).await?;
    if row.get::<_, bool>(3)
        || row.get::<_, bool>(10)
        || row.get::<_, Option<String>>(5).as_deref() != Some(attempt.as_str())
    {
        return Err((
            StatusCode::CONFLICT,
            "Autopay setup is no longer available.".into(),
        ));
    }
    let stripe = platform.for_account(row.get(11));
    if let Some(existing) = row.get::<_, Option<String>>(8) {
        let session = stripe.session(&existing).await?;
        return BillingAction::checkout(&stripe, &session).map(Json);
    }
    let expiry: i64 = row.get(7);
    // Stripe requires an expiration at least 30 minutes out. A new attempt is
    // safe because no Checkout URL was disclosed before committing the session ID.
    if expiry <= Utc::now().timestamp() + 30 * 60 {
        tx.execute(
            "DELETE FROM lease_stripe_checkout WHERE lease_id=$1",
            &[&id],
        )
        .await
        .map_err(database_error)?;
        tx.commit().await.map_err(database_error)?;
        return Err((StatusCode::CONFLICT, "Setup expired. Try again.".into()));
    }
    let form: CheckoutForm = serde_json::from_value(row.get(6)).map_err(|_| unavailable())?;
    let fields: Vec<_> = form
        .iter()
        .map(|(key, value)| (key.as_str(), value.clone()))
        .collect();
    let result = if form
        .get("buildry_payment_method")
        .is_some_and(|value| value == "acss_debit")
    {
        crate::pad::create_subscription(&stripe, &form, id, &attempt).await?
    } else {
        stripe
            .request(Method::POST, "checkout/sessions", &fields, Some(&attempt))
            .await?
    };
    let session = result["id"].as_str().ok_or_else(unavailable)?;
    let action = BillingAction::checkout(&stripe, &result)?;
    tx.execute(
        "UPDATE lease_stripe_checkout SET session_id=$2 WHERE lease_id=$1",
        &[&id, &session],
    )
    .await
    .map_err(database_error)?;
    tx.commit().await.map_err(database_error)?;
    Ok(Json(action))
}

pub(crate) fn stripe_url(value: &Value, host: &str) -> ApiResult<String> {
    let url = value["url"].as_str().ok_or_else(unavailable)?;
    let parsed = Url::parse(url).map_err(|_| unavailable())?;
    if parsed.scheme() != "https" || parsed.host_str() != Some(host) {
        return Err(unavailable());
    }
    Ok(url.into())
}

async fn stop_session(stripe: &Stripe, session: &Value) -> ApiResult<()> {
    if session["pad"] == true {
        // Cancel the subscription first so bank verification cannot leave a future rent charge behind.
        cancel_subscription(stripe, session).await?;
        let setup = &session["subscription"]["pending_setup_intent"];
        if let Some(id) = setup["id"]
            .as_str()
            .filter(|_| !matches!(setup["status"].as_str(), Some("canceled" | "succeeded")))
        {
            let result = stripe
                .request(
                    Method::POST,
                    &format!("setup_intents/{id}/cancel"),
                    &[],
                    None,
                )
                .await;
            if result.is_err() {
                let current = stripe
                    .request(Method::GET, &format!("setup_intents/{id}"), &[], None)
                    .await?;
                if !matches!(current["status"].as_str(), Some("canceled" | "succeeded")) {
                    return Err(unavailable());
                }
            }
        }
        return Ok(());
    }

    if session["status"] == "open" {
        let id = session["id"].as_str().ok_or_else(unavailable)?;
        if stripe
            .request(
                Method::POST,
                &format!("checkout/sessions/{id}/expire"),
                &[],
                None,
            )
            .await
            .is_ok()
        {
            return Ok(());
        }
        // Checkout may have completed while the tenant was turning autopay off.
        let current = stripe.session(id).await?;
        return cancel_subscription(stripe, &current).await;
    }
    cancel_subscription(stripe, session).await
}

async fn cancel_subscription(stripe: &Stripe, session: &Value) -> ApiResult<()> {
    if session["status"] == "open" {
        return Err(unavailable());
    }
    if session["status"] == "complete" && !is_stopped(&session["subscription"]) {
        let id = session["subscription"]["id"]
            .as_str()
            .ok_or_else(unavailable)?;
        stripe
            .request(
                Method::DELETE,
                &format!("subscriptions/{id}?invoice_now=false&prorate=false"),
                &[],
                None,
            )
            .await?;
    }
    Ok(())
}

pub async fn stop_for_ended_tenancy(db: &impl GenericClient, id: i64) -> ApiResult<()> {
    let row = db
        .query_opt(
            "SELECT session_id,test_mode,connected_account FROM lease_stripe_checkout WHERE lease_id=$1",
            &[&id],
        )
        .await
        .map_err(database_error)?;
    if let Some(row) = row {
        if let Some(session) = row.get::<_, Option<String>>(0) {
            let stripe = Stripe::configured().ok_or_else(unavailable)?;
            if row.get::<_, bool>(1) != stripe.key.starts_with("sk_test_") {
                return Err(unavailable());
            }
            let stripe = stripe.for_account(row.get(2));
            stop_session(&stripe, &stripe.session(&session).await?).await?;
        }
    }
    Ok(())
}

#[derive(Serialize)]
pub struct LeasePaymentOverview {
    autopay: &'static str,
    test_mode: bool,
    rent_received_on: Option<String>,
    key_received_on: Option<String>,
    can_close: bool,
    can_reopen: bool,
}

pub async fn lease_payments(
    State(db): State<Database>,
    Path(id): Path<i64>,
) -> ApiResult<Json<LeasePaymentOverview>> {
    let row = db.lock().await.query_opt(
        "SELECT l.landlord_snapshot #>> '{landlord,id}', c.session_id, c.test_mode,
         (SELECT received_on::text FROM lease_deposit_receipts WHERE lease_id=l.id AND kind='rent'),
         (SELECT received_on::text FROM lease_deposit_receipts WHERE lease_id=l.id AND kind='key'),
         l.rent_autopay_paused, l.status='confirmed' AND l.tenancy_ended_at IS NULL, c.connected_account
         FROM leases l LEFT JOIN lease_stripe_checkout c ON c.lease_id=l.id
         WHERE l.id=$1 AND l.status IN ('signed','confirmed') AND NOT l.is_demo AND NOT l.is_seeded",
        &[&id],
    ).await.map_err(database_error)?.ok_or((StatusCode::NOT_FOUND, "Lease not found.".into()))?;
    let session_id = row.get::<_, Option<String>>(1);
    let test_mode = row.get::<_, Option<bool>>(2);
    let stripe = Stripe::configured();
    let paused: bool = row.get(5);
    let current: bool = row.get(6);
    let matching_stripe = if let Some(platform) = stripe.as_ref() {
        crate::stripe_connect::for_landlord(&*db.lock().await, row.get(0), platform.test_mode())
            .await?
            .is_some_and(|account| account.ready)
    } else {
        false
    };
    let mut overview = LeasePaymentOverview {
        autopay: "unavailable",
        test_mode: test_mode.unwrap_or_else(|| {
            stripe
                .as_ref()
                .is_some_and(|s| s.key.starts_with("sk_test_"))
        }),
        rent_received_on: row.get(3),
        key_received_on: row.get(4),
        can_close: current && !paused && (matching_stripe || session_id.is_some()),
        can_reopen: current && paused && matching_stripe,
    };
    if let Some(stripe) = stripe
        .filter(|_| matching_stripe || session_id.is_some())
        .map(|platform| platform.for_account(row.get(7)))
    {
        overview.autopay =
            if test_mode.is_some_and(|mode| mode != stripe.key.starts_with("sk_test_")) {
                "unavailable"
            } else if let Some(id) = session_id {
                match stripe.session(&id).await {
                    Ok(session) => match session["status"].as_str() {
                        Some("open") => "pending",
                        Some("complete") if !is_stopped(&session["subscription"]) => {
                            subscription_state(&session)
                        }
                        _ => "off",
                    },
                    Err(_) => "unavailable",
                }
            } else {
                "off"
            };
    }
    if paused {
        overview.autopay = "paused";
    }
    Ok(Json(overview))
}

#[derive(Deserialize)]
pub struct DepositReceipt {
    kind: String,
    received_on: Option<String>,
}

pub async fn record_deposit(
    auth: AuthSession,
    State(db): State<Database>,
    Path(id): Path<i64>,
    Json(input): Json<DepositReceipt>,
) -> ApiResult<StatusCode> {
    let user = crate::scope::user(&auth)?;
    let field = match input.kind.as_str() {
        "rent" => "rent_deposit",
        "key" => "key_deposit",
        _ => return Err((StatusCode::BAD_REQUEST, "Choose a deposit.".into())),
    };
    let received_on = input
        .received_on
        .map(|value| {
            NaiveDate::parse_from_str(&value, "%Y-%m-%d")
                .ok()
                .filter(|date| {
                    *date
                        <= Utc::now()
                            .with_timezone(&chrono_tz::America::Toronto)
                            .date_naive()
                })
                .map(|date| date.to_string())
                .ok_or((
                    StatusCode::BAD_REQUEST,
                    "Enter a receipt date on or before today.".into(),
                ))
        })
        .transpose()?;
    let mut client = db.lock().await;
    let tx = client.transaction().await.map_err(database_error)?;
    let row = tx
        .query_opt(
            "SELECT details->>$2 FROM leases WHERE id=$1 AND status IN ('signed','confirmed')
         AND NOT is_demo AND NOT is_seeded FOR UPDATE",
            &[&id, &field],
        )
        .await
        .map_err(database_error)?
        .ok_or((StatusCode::NOT_FOUND, "Lease not found.".into()))?;
    if let Some(date) = received_on {
        if row
            .get::<_, Option<String>>(0)
            .and_then(|amount| cents(&amount))
            .unwrap_or(0)
            == 0
        {
            return Err((
                StatusCode::CONFLICT,
                "This lease has no deposit of that type.".into(),
            ));
        }
        tx.execute(
            "INSERT INTO lease_deposit_receipts (lease_id,kind,received_on,recorded_by)
             VALUES ($1,$2,$3::text::date,$4) ON CONFLICT (lease_id,kind)
             DO UPDATE SET received_on=EXCLUDED.received_on,recorded_by=EXCLUDED.recorded_by",
            &[&id, &input.kind, &date, &user.id],
        )
        .await
        .map_err(database_error)?;
    } else {
        tx.execute(
            "DELETE FROM lease_deposit_receipts WHERE lease_id=$1 AND kind=$2",
            &[&id, &input.kind],
        )
        .await
        .map_err(database_error)?;
    }
    tx.commit().await.map_err(database_error)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn set_lease_autopay_closed(
    State(db): State<Database>,
    Path(id): Path<i64>,
    Json(closed): Json<bool>,
) -> ApiResult<StatusCode> {
    let mut client = db.lock().await;
    let tx = client.transaction().await.map_err(database_error)?;
    let count = tx
        .execute(
            "UPDATE leases SET rent_autopay_paused=$2 WHERE id=$1 AND status='confirmed'
         AND tenancy_ended_at IS NULL AND NOT is_demo AND NOT is_seeded",
            &[&id, &closed],
        )
        .await
        .map_err(database_error)?;
    if count == 0 {
        return Err((
            StatusCode::CONFLICT,
            "This tenancy cannot change autopay.".into(),
        ));
    }
    if closed {
        stop_for_ended_tenancy(&tx, id).await?;
    }
    tx.commit().await.map_err(database_error)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize)]
pub struct PaymentHistory {
    pub(crate) rows: Vec<PaymentRecord>,
    pub(crate) test_mode: bool,
    pub(crate) stripe_unavailable: bool,
}

#[derive(Serialize)]
pub(crate) struct PaymentRecord {
    pub(crate) id: String,
    pub(crate) kind: &'static str,
    pub(crate) date: String,
    pub(crate) amount_cents: u64,
    pub(crate) status: &'static str,
    pub(crate) invoice_url: Option<String>,
}

impl Stripe {
    async fn subscription_history(&self, subscription: &str) -> ApiResult<Vec<PaymentRecord>> {
        let mut rows = Vec::new();
        let mut cursor = None::<String>;
        loop {
            let query = reqwest::Url::parse_with_params(
                "https://api.stripe.com/v1/invoices",
                [("subscription", subscription), ("limit", "100")]
                    .into_iter()
                    .chain(cursor.as_deref().map(|value| ("starting_after", value))),
            )
            .map_err(|_| unavailable())?;
            let result = self
                .request(
                    Method::GET,
                    &format!("invoices?{}", query.query().unwrap_or_default()),
                    &[],
                    None,
                )
                .await?;
            let invoices = result["data"].as_array().ok_or_else(unavailable)?;
            for invoice in invoices {
                let detailed;
                let invoice = if invoice["status"] == "open" && invoice["attempted"] == true {
                    let id = invoice["id"].as_str().ok_or_else(unavailable)?;
                    detailed = self
                        .request(
                            Method::GET,
                            &format!("invoices/{id}?expand[]=payments.data.payment.payment_intent"),
                            &[],
                            None,
                        )
                        .await?;
                    &detailed
                } else {
                    invoice
                };
                rows.extend(invoice_record(invoice));
            }
            if result["has_more"] != true {
                return Ok(rows);
            }
            let next = invoices
                .last()
                .and_then(|invoice| invoice["id"].as_str())
                .ok_or_else(unavailable)?;
            if cursor.as_deref() == Some(next) {
                return Err(unavailable());
            }
            cursor = Some(next.into());
        }
    }
}

fn invoice_record(invoice: &Value) -> Option<PaymentRecord> {
    let status = match invoice["status"].as_str()? {
        "paid" => "paid",
        "open" if invoice_processing(invoice) => "processing",
        "open" if invoice["attempted"] == true => "attention",
        "open" => "open",
        "uncollectible" => "uncollectible",
        "void" => "void",
        _ => return None,
    };
    if invoice["currency"] != "cad" {
        return None;
    }
    let amount_cents = invoice[if status == "paid" {
        "amount_paid"
    } else {
        "amount_due"
    }]
    .as_u64()?;
    if amount_cents == 0 {
        return None;
    }
    let timestamp = invoice["status_transitions"]["paid_at"]
        .as_i64()
        .or_else(|| invoice["created"].as_i64())?;
    let date = chrono::DateTime::from_timestamp(timestamp, 0)?
        .with_timezone(&chrono_tz::America::Toronto)
        .date_naive()
        .to_string();
    let invoice_url = invoice["invoice_pdf"]
        .as_str()
        .filter(|value| {
            Url::parse(value).is_ok_and(|url| {
                url.scheme() == "https" && url.host_str() == Some("pay.stripe.com")
            })
        })
        .map(str::to_owned);
    Some(PaymentRecord {
        id: invoice["id"].as_str()?.into(),
        kind: "rent",
        date,
        amount_cents,
        status,
        invoice_url,
    })
}

pub async fn tenant_history(
    auth: AuthSession,
    State(db): State<Database>,
    Path(id): Path<i64>,
) -> ApiResult<Json<PaymentHistory>> {
    let user = tenant_id(auth)?;
    payment_history(&db, id, user, true).await.map(Json)
}

pub async fn lease_history(
    auth: AuthSession,
    State(db): State<Database>,
    Path(id): Path<i64>,
) -> ApiResult<Json<PaymentHistory>> {
    let user = crate::scope::user(&auth)?;
    crate::scope::require_entity(&db, user.id, "leases", id).await?;
    payment_history(&db, id, user.id, false).await.map(Json)
}

pub(crate) async fn payment_history(
    db: &Database,
    id: i64,
    user: i64,
    tenant: bool,
) -> ApiResult<PaymentHistory> {
    let (row, prior) = {
        let client = db.lock().await;
        let row = client.query_opt(
            "SELECT l.details->>'rent_deposit',l.details->>'key_deposit',
             (SELECT received_on::text FROM lease_deposit_receipts WHERE lease_id=l.id AND kind='rent'),
             (SELECT received_on::text FROM lease_deposit_receipts WHERE lease_id=l.id AND kind='key'),
             c.session_id,c.connected_account,c.test_mode
             FROM leases l JOIN users u ON u.id=$2 AND u.active
             LEFT JOIN lease_stripe_checkout c ON c.lease_id=l.id
             WHERE l.id=$1 AND NOT l.is_demo AND
             (CASE WHEN $3 THEN l.status='confirmed' AND lower(l.tenant_email)=lower(u.email) AND u.role='tenant'
              ELSE l.status IN ('signed','confirmed') AND
                   (u.role<>'landlord' OR u.landlord_id::text=l.landlord_snapshot #>> '{landlord,id}') END)",
            &[&id,&user,&tenant],
        ).await.map_err(database_error)?.ok_or((StatusCode::NOT_FOUND, "Lease not found.".into()))?;
        let prior = client
            .query(
                "SELECT subscription_id,connected_account,test_mode
             FROM lease_stripe_past_subscriptions WHERE lease_id=$1",
                &[&id],
            )
            .await
            .map_err(database_error)?;
        (row, prior)
    };
    let mut history = PaymentHistory {
        rows: Vec::new(),
        test_mode: false,
        stripe_unavailable: false,
    };
    for (amount, date, kind) in [(0, 2, "rent_deposit"), (1, 3, "key_deposit")] {
        let amount = row
            .get::<_, Option<String>>(amount)
            .and_then(|value| cents(&value));
        if let (Some(amount_cents), Some(date)) = (amount, row.get::<_, Option<String>>(date)) {
            history.rows.push(PaymentRecord {
                id: kind.into(),
                kind,
                date,
                amount_cents: amount_cents.into(),
                status: "recorded",
                invoice_url: None,
            });
        }
    }
    let session: Option<String> = row.get(4);
    let Some(platform) = Stripe::configured() else {
        history.stripe_unavailable = session.is_some() || !prior.is_empty();
        return Ok(history);
    };
    history.test_mode = platform.test_mode();
    let mut subscriptions: Vec<(String, Option<String>)> = prior
        .into_iter()
        .filter_map(|row| {
            (row.get::<_, bool>(2) == history.test_mode).then(|| (row.get(0), row.get(1)))
        })
        .collect();
    if let Some(id) = session.filter(|_| row.get::<_, Option<bool>>(6) == Some(history.test_mode)) {
        let account: Option<String> = row.get(5);
        match platform.for_account(account.clone()).session(&id).await {
            Ok(session) => {
                if let Some(subscription) = session["subscription"]["id"].as_str() {
                    subscriptions.push((subscription.into(), account));
                }
            }
            Err(_) => history.stripe_unavailable = true,
        }
    }
    subscriptions.sort_unstable();
    subscriptions.dedup();
    for (subscription, account) in subscriptions {
        match platform
            .for_account(account)
            .subscription_history(&subscription)
            .await
        {
            Ok(rows) => history.rows.extend(rows),
            Err(_) => history.stripe_unavailable = true,
        }
    }
    history
        .rows
        .sort_by(|a, b| b.date.cmp(&a.date).then_with(|| a.id.cmp(&b.id)));
    Ok(history)
}

#[cfg(test)]
mod pad_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_trial_subscription_does_not_mean_the_bank_is_verified() {
        let mut session = json!({"pad":true,"subscription":{"status":"trialing","pending_setup_intent":{"status":"requires_payment_method"}}});
        assert_eq!(subscription_state(&session), "pending");
        session["subscription"]["pending_setup_intent"]["status"] = json!("requires_action");
        assert_eq!(subscription_state(&session), "verifying");
        session["subscription"]["pending_setup_intent"] = Value::Null;
        assert_eq!(subscription_state(&session), "attention");
        session["subscription"]["default_payment_method"] = json!("pm_verified");
        assert_eq!(subscription_state(&session), "active");
        session["subscription"]["status"] = json!("canceled");
        assert_eq!(subscription_state(&session), "off");
    }

    #[test]
    fn submitted_bank_debits_are_not_reported_as_paid_or_failed() {
        let mut invoice = json!({"id":"in_pad","status":"open","attempted":true,"currency":"cad","amount_due":95000,"amount_paid":0,"created":1793520000,"payments":{"data":[{"payment":{"payment_intent":{"status":"processing"}}}]}});
        assert_eq!(invoice_record(&invoice).unwrap().status, "processing");
        invoice["payments"]["data"][0]["payment"]["payment_intent"]["status"] =
            json!("requires_payment_method");
        assert_eq!(invoice_record(&invoice).unwrap().status, "attention");
        invoice["status"] = json!("paid");
        invoice["amount_paid"] = json!(95000);
        assert_eq!(invoice_record(&invoice).unwrap().status, "paid");
    }

    #[test]
    fn existing_card_trials_keep_their_current_status() {
        assert_eq!(
            subscription_state(&json!({"subscription":{"status":"trialing"}})),
            "active"
        );
        assert_eq!(
            subscription_state(&json!({"subscription":{"status":"past_due"}})),
            "attention"
        );
    }
}
