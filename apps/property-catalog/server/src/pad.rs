use crate::{
    billing::{unavailable, Stripe},
    ApiResult,
};
use axum::http::StatusCode;
use reqwest::Method;
use serde_json::Value;
use std::collections::BTreeMap;

pub(crate) async fn ensure_capability(platform: &Stripe, account: &str) -> ApiResult<()> {
    let path = format!("accounts/{account}/capabilities/acss_debit_payments");
    let capability = platform.request(Method::GET, &path, &[], None).await?;
    if capability["status"] != "active" {
        return Err((
            StatusCode::CONFLICT,
            "The landlord needs to finish enabling Canadian bank payments in Deposit setup.".into(),
        ));
    }
    Ok(())
}

pub(crate) async fn create_subscription(
    stripe: &Stripe,
    form: &BTreeMap<String, String>,
    lease: i64,
    attempt: &str,
) -> ApiResult<Value> {
    let get = |key: &str| form.get(key).cloned().ok_or_else(unavailable);
    let amount = get("line_items[0][price_data][unit_amount]")?;
    let due = get("subscription_data[billing_cycle_anchor]")?;
    let customer = stripe
        .request(
            Method::POST,
            "customers",
            &[
                ("email", get("customer_email")?),
                ("metadata[buildry_lease_id]", lease.to_string()),
            ],
            Some(&format!("{attempt}-customer")),
        )
        .await?;
    let product = stripe
        .request(
            Method::POST,
            "products",
            &[("name", "Monthly residential rent".into())],
            Some(&format!("{attempt}-product")),
        )
        .await?;
    let mut subscription = stripe
        .request(
            Method::POST,
            "subscriptions",
            &[
                (
                    "customer",
                    customer["id"].as_str().ok_or_else(unavailable)?.into(),
                ),
                (
                    "items[0][price_data][product]",
                    product["id"].as_str().ok_or_else(unavailable)?.into(),
                ),
                ("items[0][price_data][currency]", "cad".into()),
                ("items[0][price_data][unit_amount]", amount.clone()),
                ("items[0][price_data][recurring][interval]", "month".into()),
                ("trial_end", due.clone()),
                (
                    "trial_settings[end_behavior][missing_payment_method]",
                    "cancel".into(),
                ),
                ("proration_behavior", "none".into()),
                ("payment_behavior", "default_incomplete".into()),
                (
                    "payment_settings[payment_method_types][0]",
                    "acss_debit".into(),
                ),
                (
                    "payment_settings[save_default_payment_method]",
                    "on_subscription".into(),
                ),
                ("automatic_tax[enabled]", "false".into()),
                ("metadata[buildry_lease_id]", lease.to_string()),
                ("metadata[buildry_payment_method]", "acss_debit".into()),
                ("expand[0]", "pending_setup_intent".into()),
            ],
            Some(&format!("{attempt}-subscription")),
        )
        .await?;
    let setup_id = subscription["pending_setup_intent"]["id"]
        .as_str()
        .ok_or_else(unavailable)?;
    let setup = stripe
        .request(
            Method::POST,
            &format!("setup_intents/{setup_id}"),
            &[
                ("payment_method_options[acss_debit][currency]", "cad".into()),
                (
                    "payment_method_options[acss_debit][verification_method]",
                    "automatic".into(),
                ),
                (
                    "payment_method_options[acss_debit][mandate_options][transaction_type]",
                    "personal".into(),
                ),
                (
                    "payment_method_options[acss_debit][mandate_options][default_for][0]",
                    "invoice".into(),
                ),
                (
                    "payment_method_options[acss_debit][mandate_options][default_for][1]",
                    "subscription".into(),
                ),
            ],
            Some(&format!("{attempt}-mandate")),
        )
        .await?;
    subscription["pending_setup_intent"] = setup;
    Ok(
        serde_json::json!({"id":subscription["id"],"status":"complete","pad":true,"customer":subscription["customer"],"subscription":subscription}),
    )
}
