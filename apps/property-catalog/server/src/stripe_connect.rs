use crate::{
    auth::{AuthSession, Database},
    billing::{stripe_url, unavailable, Stripe},
    database_error, ApiResult,
};
use axum::{extract::State, http::StatusCode, response::Redirect, Json};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use tokio_postgres::{Client, GenericClient, Transaction};

pub(crate) async fn migrate(db: &Client) -> Result<(), tokio_postgres::Error> {
    db.batch_execute(
        "CREATE TABLE IF NOT EXISTS landlord_stripe_accounts (
            landlord_id BIGINT NOT NULL REFERENCES landlords(id),
            test_mode BOOLEAN NOT NULL,
            request_key TEXT NOT NULL,
            account_id TEXT UNIQUE,
            ready BOOLEAN NOT NULL DEFAULT FALSE,
            portal_configuration TEXT,
            PRIMARY KEY (landlord_id, test_mode)
        );
        CREATE TABLE IF NOT EXISTS user_stripe_accounts (
            user_id BIGINT NOT NULL REFERENCES users(id), test_mode BOOLEAN NOT NULL,
            request_key TEXT NOT NULL, account_id TEXT, ready BOOLEAN NOT NULL DEFAULT FALSE,
            portal_configuration TEXT, PRIMARY KEY(user_id,test_mode)
        );
        INSERT INTO user_stripe_accounts(user_id,test_mode,request_key,account_id,ready,portal_configuration)
        SELECT u.id,a.test_mode,a.request_key,a.account_id,a.ready,a.portal_configuration
        FROM users u JOIN landlord_stripe_accounts a ON a.landlord_id=u.landlord_id WHERE u.landlord_id IS NOT NULL
        ON CONFLICT DO NOTHING",
    )
    .await
}

pub(crate) struct ConnectedAccount {
    pub(crate) id: String,
    pub(crate) ready: bool,
}

pub(crate) async fn for_landlord(
    db: &impl GenericClient,
    landlord: Option<String>,
    test_mode: bool,
) -> ApiResult<Option<ConnectedAccount>> {
    let Some(landlord) = landlord.and_then(|value| value.parse::<i64>().ok()) else {
        return Ok(None);
    };
    let row = db
        .query_opt(
            "SELECT account_id,ready FROM (
          SELECT a.account_id,a.ready,0 AS priority FROM user_stripe_accounts a JOIN users u ON u.id=a.user_id
          WHERE u.landlord_id=$1 AND a.test_mode=$2 AND a.account_id IS NOT NULL
          UNION ALL SELECT account_id,ready,1 FROM landlord_stripe_accounts
          WHERE landlord_id=$1 AND test_mode=$2 AND account_id IS NOT NULL
          UNION ALL SELECT a.account_id,a.ready,2 FROM user_stripe_accounts a JOIN resource_owners o ON o.resource_type='landlord' AND o.resource_id=$1 AND o.user_id=a.user_id WHERE a.test_mode=$2 AND a.account_id IS NOT NULL
         ) accounts ORDER BY priority,account_id LIMIT 1",
            &[&landlord, &test_mode],
        )
        .await
        .map_err(database_error)?;
    Ok(row.map(|row| ConnectedAccount {
        id: row.get(0),
        ready: row.get(1),
    }))
}

pub(crate) async fn refresh_account(
    db: &impl GenericClient,
    stripe: &Stripe,
    id: &str,
) -> ApiResult<bool> {
    let account = stripe
        .request(Method::GET, &format!("accounts/{id}"), &[], None)
        .await?;
    let ready = account["charges_enabled"] == true
        && account["payouts_enabled"] == true
        && (account["capabilities"]["card_payments"] == "active" || account["capabilities"]["acss_debit_payments"] == "active");
    db.execute(
        "UPDATE landlord_stripe_accounts SET ready=$2 WHERE account_id=$1 AND ready<>$2",
        &[&id, &ready],
    )
    .await
    .map_err(database_error)?;
    db.execute("UPDATE user_stripe_accounts SET ready=$2 WHERE account_id=$1 AND ready<>$2", &[&id,&ready]).await.map_err(database_error)?;
    Ok(ready)
}

async fn landlord_id(db: &Client, user_id: i64) -> ApiResult<Option<i64>> {
    db.query_opt(
        "SELECT landlord_id FROM users WHERE id=$1 AND active",
        &[&user_id],
    )
    .await
    .map_err(database_error)
    .map(|row| row.and_then(|row| row.get(0)))
}

#[derive(Serialize)]
pub struct Payouts {
    status: &'static str,
    test_mode: bool,
}

pub async fn status(
    auth: AuthSession,
    State(db): State<Database>,
) -> ApiResult<Json<Option<Payouts>>> {
    let Some(user) = auth.user.filter(|user| matches!(user.role.as_str(), "user" | "admin")) else {
        return Ok(Json(None));
    };
    let Some(stripe) = Stripe::configured() else {
        return Ok(Json(Some(Payouts {
            status: "unavailable",
            test_mode: false,
        })));
    };
    let client = db.lock().await;
    let Some(id) = landlord_id(&client, user.id).await? else {
        return Ok(Json(Some(Payouts {
            status: "unavailable",
            test_mode: stripe.test_mode(),
        })));
    };
    let account = for_landlord(&*client, Some(id.to_string()), stripe.test_mode()).await?;
    let status = match account {
        Some(account) => match refresh_account(&*client, &stripe, &account.id).await {
            Ok(true) => "ready",
            Ok(false) => "pending",
            Err(_) => "unavailable",
        },
        None => "new",
    };
    Ok(Json(Some(Payouts {
        status,
        test_mode: stripe.test_mode(),
    })))
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PayoutAction {
    Onboard,
}

#[derive(Serialize)]
pub struct Onboarding {
    url: String,
}

pub async fn onboard(
    auth: AuthSession,
    State(db): State<Database>,
    Json(_): Json<PayoutAction>,
) -> ApiResult<Json<Onboarding>> {
    if let Some(user)=auth.user.as_ref().filter(|u|matches!(u.role.as_str(),"user"|"admin")) {
        let stripe=Stripe::configured().ok_or_else(unavailable)?;
        let account=crate::payout_setup::for_user(&*db.lock().await,user.id,stripe.test_mode()).await?;
        if let Some(account)=account {
            let result=stripe.request(Method::POST,"account_links",&[
                ("account",account.id),("type","account_onboarding".into()),
                ("refresh_url",format!("{}/api/stripe/connect/refresh",stripe.origin)),
                ("return_url",format!("{}/payments/settings/payouts",stripe.origin))],None).await?;
            return Ok(Json(Onboarding{url:stripe_url(&result,"connect.stripe.com")?}));
        }
    }
    let user = auth
        .user
        .filter(|user| matches!(user.role.as_str(), "user" | "admin"))
        .ok_or((StatusCode::FORBIDDEN, "Landlord account required.".into()))?;
    let stripe = Stripe::configured().ok_or_else(unavailable)?;
    let mode = stripe.test_mode();
    let mut client = db.lock().await;
    let id = landlord_id(&client, user.id).await?.ok_or((
        StatusCode::FORBIDDEN,
        "Link a landlord profile to continue.".into(),
    ))?;
    let request_key = uuid::Uuid::new_v4().to_string();
    client.execute(
        "INSERT INTO landlord_stripe_accounts(landlord_id,test_mode,request_key) VALUES($1,$2,$3)
         ON CONFLICT (landlord_id,test_mode) DO NOTHING", &[&id,&mode,&request_key],
    ).await.map_err(database_error)?;
    let tx = client.transaction().await.map_err(database_error)?;
    let row = tx
        .query_one(
            "SELECT account_id,request_key FROM landlord_stripe_accounts
         WHERE landlord_id=$1 AND test_mode=$2 FOR UPDATE",
            &[&id, &mode],
        )
        .await
        .map_err(database_error)?;
    let account = match row.get::<_, Option<String>>(0) {
        Some(account) => account,
        None => {
            let key: String = row.get(1);
            let result = stripe
                .request(
                    Method::POST,
                    "accounts",
                    &[
                        ("country", "CA".into()),
                        ("controller[stripe_dashboard][type]", "full".into()),
                        ("controller[fees][payer]", "account".into()),
                        ("controller[losses][payments]", "stripe".into()),
                        ("controller[requirement_collection]", "stripe".into()),
                        ("capabilities[card_payments][requested]", "true".into()),
                        ("capabilities[transfers][requested]", "true".into()),
                        ("capabilities[acss_debit_payments][requested]", "true".into()),
                        ("metadata[buildry_landlord_id]", id.to_string()),
                    ],
                    Some(&key),
                )
                .await?;
            let account = result["id"]
                .as_str()
                .filter(|id| id.starts_with("acct_"))
                .ok_or_else(unavailable)?
                .to_string();
            tx.execute(
                "UPDATE landlord_stripe_accounts SET account_id=$3 WHERE landlord_id=$1 AND test_mode=$2",
                &[&id,&mode,&account],
            ).await.map_err(database_error)?;
            account
        }
    };
    tx.commit().await.map_err(database_error)?;
    drop(client);
    let return_url = format!("{}/payments/settings/payouts", stripe.origin);
    let result = stripe
        .request(
            Method::POST,
            "account_links",
            &[
                ("account", account),
                ("type", "account_onboarding".into()),
                ("refresh_url", format!("{}/api/stripe/connect/refresh", stripe.origin)),
                ("return_url", return_url),
            ],
            None,
        )
        .await?;
    Ok(Json(Onboarding {
        url: stripe_url(&result, "connect.stripe.com")?,
    }))
}

pub async fn refresh(
    auth: AuthSession,
    db: State<Database>,
) -> ApiResult<Redirect> {
    let Json(result) = onboard(auth, db, Json(PayoutAction::Onboard)).await?;
    Ok(Redirect::to(&result.url))
}

pub(crate) async fn portal_configuration(
    db: &Transaction<'_>,
    stripe: &Stripe,
    account: &str,
) -> ApiResult<String> {
    let row = db.query_one(
        "SELECT portal_configuration FROM user_stripe_accounts WHERE account_id=$1 LIMIT 1 FOR UPDATE",
        &[&account],
    ).await.map_err(database_error)?;
    if let Some(id) = row.get::<_, Option<String>>(0) {
        return Ok(id);
    }
    let configuration = stripe
        .request(
            Method::POST,
            "billing_portal/configurations",
            &[
                ("features[invoice_history][enabled]", "true".into()),
                ("features[payment_method_update][enabled]", "true".into()),
                ("features[subscription_cancel][enabled]", "false".into()),
                ("features[subscription_update][enabled]", "false".into()),
            ],
            Some("buildry-rent-portal-v1"),
        )
        .await?;
    let id = configuration["id"].as_str().ok_or_else(unavailable)?;
    db.execute(
        "UPDATE user_stripe_accounts SET portal_configuration=$2 WHERE account_id=$1",
        &[&account, &id],
    )
    .await
    .map_err(database_error)?;
    Ok(id.to_string())
}
