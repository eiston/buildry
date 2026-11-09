use crate::{
    auth::{AuthSession, Database},
    billing::{unavailable, Stripe},
    database_error,
    stripe_connect::ConnectedAccount,
    ApiResult,
};
use axum::{
    extract::{ConnectInfo, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Serialize)]
pub struct Setup {
    pub test_mode: bool,
    pub defaults: BTreeMap<String, String>,
    pub publishable_key: String,
    pub status: &'static str,
    pub custom: bool,
    pub pad_enabled: bool,
    pub pad_requested: bool,
    pub business_type: String,
    pub requirements: Vec<String>,
    pub pending: bool,
    pub bank: Option<String>,
    pub people: Vec<PersonSummary>,
}

#[derive(Serialize)]
pub struct PersonSummary {
    id: String,
    name: String,
}

async fn owner(db: &tokio_postgres::Client, auth: &AuthSession) -> ApiResult<i64> {
    let user = auth
        .user
        .as_ref()
        .filter(|u| matches!(u.role.as_str(), "tenant" | "landlord"))
        .ok_or((StatusCode::FORBIDDEN, "Payment account required.".into()))?;
    db.query_opt("SELECT id FROM users WHERE id=$1 AND active", &[&user.id])
        .await
        .map_err(database_error)?
        .map(|r| r.get(0))
        .ok_or((StatusCode::FORBIDDEN, "Account unavailable.".into()))
}

pub(crate) async fn for_user(
    db: &impl tokio_postgres::GenericClient,
    user: i64,
    mode: bool,
) -> ApiResult<Option<ConnectedAccount>> {
    db.query_opt("SELECT account_id,ready FROM (SELECT account_id,ready,0 AS priority FROM user_stripe_accounts WHERE user_id=$1 AND test_mode=$2 AND account_id IS NOT NULL UNION ALL SELECT a.account_id,a.ready,1 FROM landlord_stripe_accounts a JOIN users u ON u.landlord_id=a.landlord_id AND u.role='landlord' WHERE u.id=$1 AND a.test_mode=$2 AND a.account_id IS NOT NULL) accounts ORDER BY priority LIMIT 1", &[&user,&mode])
        .await.map_err(database_error).map(|r| r.map(|r| ConnectedAccount { id:r.get(0), ready:r.get(1) }))
}

fn phone(value: &str) -> Option<String> {
    let digits: String = value.chars().filter(char::is_ascii_digit).collect();
    if digits.len() == 10 {
        Some(format!("+1{digits}"))
    } else if digits.len() == 11 && digits.starts_with('1') {
        Some(format!("+{digits}"))
    } else if value.starts_with('+') && (8..=15).contains(&digits.len()) {
        Some(format!("+{digits}"))
    } else {
        None
    }
}
async fn profile_defaults(
    db: &tokio_postgres::Client,
    user: i64,
) -> ApiResult<BTreeMap<String, String>> {
    let row=db.query_one("SELECT u.first_name,u.last_name,u.email,u.phone,COALESCE(l.legal_name,''),COALESCE(l.phone,''),COALESCE(l.signer_title,''),COALESCE(l.service_address,'') FROM users u LEFT JOIN landlords l ON l.id=u.landlord_id AND u.role='landlord' WHERE u.id=$1", &[&user]).await.map_err(database_error)?;
    let mut values = BTreeMap::new();
    for (path, index) in [
        ("account.individual.first_name", 0),
        ("account.individual.last_name", 1),
        ("account.individual.email", 2),
        ("account.company.name", 4),
        ("account.individual.relationship.title", 6),
    ] {
        let value: String = row.get(index);
        if !value.trim().is_empty() {
            values.insert(path.into(), value.trim().into());
        }
    }
    if let Some(value) = phone(&row.get::<_, String>(3)) {
        values.insert("account.individual.phone".into(), value.clone());
        values.insert("support_phone".into(), value);
    }
    if let Some(value) = phone(&row.get::<_, String>(5)).or_else(|| phone(&row.get::<_, String>(3)))
    {
        values.insert("account.company.phone".into(), value);
    }
    for field in ["first_name", "last_name", "email", "phone"] {
        if let Some(value) = values.get(&format!("account.individual.{field}")).cloned() {
            values.insert(format!("person.representative.{field}"), value);
        }
    }
    let address: String = row.get(7);
    let parts: Vec<&str> = address
        .split(',')
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .collect();
    if let Some(line) = parts.first() {
        values.insert("account.company.address.line1".into(), (*line).into());
    }
    if parts.len() >= 3 {
        values.insert("account.company.address.city".into(), parts[1].into());
    }
    let tokens: Vec<&str> = address
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|s| !s.is_empty())
        .collect();
    for token in &tokens {
        if [
            "AB", "BC", "MB", "NB", "NL", "NS", "NT", "NU", "ON", "PE", "QC", "SK", "YT",
        ]
        .contains(&token.to_uppercase().as_str())
        {
            values.insert("account.company.address.state".into(), token.to_uppercase());
        }
    }
    for candidate in tokens
        .iter()
        .map(|s| s.to_string())
        .chain(tokens.windows(2).map(|p| format!("{}{}", p[0], p[1])))
    {
        if candidate.len() == 6
            && candidate.bytes().enumerate().all(|(i, b)| {
                if i % 2 == 0 {
                    b.is_ascii_alphabetic()
                } else {
                    b.is_ascii_digit()
                }
            })
        {
            values.insert(
                "account.company.address.postal_code".into(),
                candidate.to_uppercase(),
            );
        }
    }
    let name = format!("{} {}", row.get::<_, String>(0), row.get::<_, String>(1))
        .trim()
        .to_string();
    if !name.is_empty() {
        values.insert("bank.account_holder_name".into(), name);
    }
    values.insert("profile_url".into(), "https://buildry.ca".into());
    values.insert(
        "product_description".into(),
        "Property payments managed through Buildry.".into(),
    );
    values.insert("mcc".into(), "6513".into());
    Ok(values)
}
async fn load(db: &tokio_postgres::Client, stripe: &Stripe, landlord: i64) -> ApiResult<Setup> {
    let mut setup = Setup {
        test_mode: stripe.test_mode(),
        defaults: profile_defaults(db, landlord).await?,
        publishable_key: stripe.publishable_key.clone().ok_or_else(unavailable)?,
        status: "new",
        custom: true,
        pad_enabled: false,
        pad_requested: false,
        business_type: String::new(),
        requirements: vec![],
        pending: false,
        bank: None,
        people: vec![],
    };
    let Some(account) = for_user(db, landlord, stripe.test_mode()).await? else {
        return Ok(setup);
    };
    let value = stripe
        .request(Method::GET, &format!("accounts/{}", account.id), &[], None)
        .await?;
    for (field, path) in [
        ("profile_url", "url"),
        ("product_description", "product_description"),
        ("mcc", "mcc"),
        ("support_phone", "support_phone"),
    ] {
        if value["business_profile"][path]
            .as_str()
            .is_some_and(|v| !v.is_empty())
        {
            setup.defaults.remove(field);
        }
    }
    let ready = value["charges_enabled"] == true
        && value["payouts_enabled"] == true
        && (value["capabilities"]["card_payments"] == "active"
            || value["capabilities"]["acss_debit_payments"] == "active");
    db.execute(
        "UPDATE user_stripe_accounts SET ready=$2 WHERE account_id=$1 AND ready<>$2",
        &[&account.id, &ready],
    )
    .await
    .map_err(database_error)?;
    db.execute(
        "UPDATE landlord_stripe_accounts SET ready=$2 WHERE account_id=$1 AND ready<>$2",
        &[&account.id, &ready],
    )
    .await
    .map_err(database_error)?;
    setup.status = if ready { "ready" } else { "pending" };
    setup.pad_enabled = value["capabilities"]["acss_debit_payments"] == "active";
    setup.pad_requested = value["capabilities"]["acss_debit_payments"].is_string();
    setup.custom = value["controller"]["requirement_collection"] == "application";
    setup.business_type = value["business_type"].as_str().unwrap_or("").into();
    for key in ["currently_due", "past_due"] {
        if let Some(values) = value["requirements"][key].as_array() {
            setup
                .requirements
                .extend(values.iter().filter_map(Value::as_str).map(str::to_string));
        }
    }
    setup.requirements.sort();
    setup.requirements.dedup();
    setup.pending = value["requirements"]["pending_verification"]
        .as_array()
        .is_some_and(|v| !v.is_empty());
    if value["external_accounts"]["data"]
        .as_array()
        .is_some_and(|banks| {
            banks.iter().any(|bank| {
                bank["default_for_currency"] == true
                    && bank["currency"] == "cad"
                    && matches!(
                        bank["status"].as_str(),
                        Some("errored" | "verification_failed")
                    )
            })
        })
        && !setup
            .requirements
            .iter()
            .any(|key| key == "external_account")
    {
        setup.requirements.push("external_account".into());
    }
    setup.bank = value["external_accounts"]["data"]
        .as_array()
        .and_then(|banks| {
            banks.iter().find(|b| {
                b["object"] == "bank_account"
                    && b["currency"] == "cad"
                    && b["default_for_currency"] == true
            })
        })
        .and_then(|bank| {
            bank["last4"].as_str().map(|last| {
                format!(
                    "{} · •••• {last}",
                    bank["bank_name"].as_str().unwrap_or("Bank account")
                )
            })
        });
    if let Some(holder) = value["external_accounts"]["data"]
        .as_array()
        .and_then(|banks| {
            banks
                .iter()
                .find(|bank| bank["currency"] == "cad" && bank["default_for_currency"] == true)
        })
        .and_then(|bank| bank["account_holder_name"].as_str())
        .filter(|name| !name.is_empty())
    {
        setup
            .defaults
            .insert("bank.account_holder_name".into(), holder.into());
    }
    if setup.custom
        && setup.business_type == "company"
        && setup
            .requirements
            .iter()
            .any(|key| key.starts_with("person_"))
    {
        let persons = stripe
            .request(
                Method::GET,
                &format!("accounts/{}/persons?limit=100", account.id),
                &[],
                None,
            )
            .await?;
        if let Some(values) = persons["data"].as_array() {
            setup.people = values
                .iter()
                .filter_map(|person| {
                    Some(PersonSummary {
                        id: person["id"].as_str()?.into(),
                        name: format!(
                            "{} {}",
                            person["first_name"].as_str().unwrap_or(""),
                            person["last_name"].as_str().unwrap_or("")
                        )
                        .trim()
                        .to_string(),
                    })
                })
                .collect();
        }
    }
    Ok(setup)
}

pub async fn status(auth: AuthSession, State(db): State<Database>) -> ApiResult<Json<Setup>> {
    let stripe = Stripe::configured().ok_or_else(unavailable)?;
    let client = db.lock().await;
    let landlord = owner(&client, &auth).await?;
    Ok(Json(load(&client, &stripe, landlord).await?))
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Change {
    EnablePad,
    Start {
        business_type: String,
    },
    Save {
        account_token: Option<String>,
        bank_token: Option<String>,
        #[serde(default)]
        persons: Vec<PersonToken>,
        #[serde(default)]
        registration_file: Option<String>,
        profile_url: Option<String>,
        product_description: Option<String>,
        mcc: Option<String>,
        support_phone: Option<String>,
        #[serde(default)]
        accept_terms: bool,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersonToken {
    id: Option<String>,
    key: String,
    token: String,
}

fn valid_token(value: &str, prefix: &str) -> bool {
    !value.is_empty()
        && value.starts_with(prefix)
        && value.len() < 255
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

pub async fn change(
    auth: AuthSession,
    State(db): State<Database>,
    ConnectInfo(peer): ConnectInfo<std::net::SocketAddr>,
    headers: HeaderMap,
    Json(input): Json<Change>,
) -> ApiResult<Json<Setup>> {
    let stripe = Stripe::configured().ok_or_else(unavailable)?;
    // Validate configuration before creating any account.
    stripe.publishable_key.as_ref().ok_or_else(unavailable)?;
    let mut client = db.lock().await;
    let landlord = owner(&client, &auth).await?;
    match input {
        Change::EnablePad => {
            let account = for_user(&*client, landlord, stripe.test_mode())
                .await?
                .ok_or((
                    StatusCode::CONFLICT,
                    "Set up your payout account first.".into(),
                ))?;
            stripe
                .request(
                    Method::POST,
                    &format!("accounts/{}/capabilities/acss_debit_payments", account.id),
                    &[("requested", "true".into())],
                    None,
                )
                .await?;
        }
        Change::Start { business_type } => {
            if !matches!(business_type.as_str(), "individual" | "company") {
                return Err((StatusCode::BAD_REQUEST, "Choose an account type.".into()));
            }
            client.execute("INSERT INTO user_stripe_accounts(user_id,test_mode,request_key) VALUES($1,$2,$3)
                ON CONFLICT (user_id,test_mode) DO NOTHING", &[&landlord,&stripe.test_mode(),&uuid::Uuid::new_v4().to_string()])
                .await.map_err(database_error)?;
            let tx = client.transaction().await.map_err(database_error)?;
            let profile:Option<i64>=tx.query_one("SELECT CASE WHEN role='landlord' THEN landlord_id ELSE NULL END FROM users WHERE id=$1", &[&landlord]).await.map_err(database_error)?.get(0);
            if let Some(profile) = profile {
                tx.execute("INSERT INTO landlord_stripe_accounts(landlord_id,test_mode,request_key) VALUES($1,$2,$3) ON CONFLICT DO NOTHING", &[&profile,&stripe.test_mode(),&uuid::Uuid::new_v4().to_string()]).await.map_err(database_error)?;
                tx.execute("UPDATE landlord_stripe_accounts a SET account_id=b.account_id,request_key=b.request_key FROM user_stripe_accounts b WHERE a.landlord_id=$1 AND a.test_mode=$2 AND a.account_id IS NULL AND b.user_id=$3 AND b.test_mode=$2 AND b.account_id IS NOT NULL", &[&profile,&stripe.test_mode(),&landlord]).await.map_err(database_error)?;
                let canonical=tx.query_one("SELECT account_id,request_key FROM landlord_stripe_accounts WHERE landlord_id=$1 AND test_mode=$2 FOR UPDATE", &[&profile,&stripe.test_mode()]).await.map_err(database_error)?;
                tx.execute("UPDATE user_stripe_accounts SET account_id=$3,request_key=$4 WHERE user_id=$1 AND test_mode=$2", &[&landlord,&stripe.test_mode(),&canonical.get::<_,Option<String>>(0),&canonical.get::<_,String>(1)]).await.map_err(database_error)?;
            }
            let row = tx.query_one("SELECT account_id,request_key FROM user_stripe_accounts WHERE user_id=$1 AND test_mode=$2 FOR UPDATE",
                &[&landlord,&stripe.test_mode()]).await.map_err(database_error)?;
            if row.get::<_, Option<String>>(0).is_none() {
                // The creation key always uses the same parameters, even if the browser retries with another business type.
                let key: String = row.get(1);
                let account = stripe
                    .request(
                        Method::POST,
                        "accounts",
                        &[
                            ("country", "CA".into()),
                            ("type", "custom".into()),
                            ("capabilities[card_payments][requested]", "true".into()),
                            ("capabilities[transfers][requested]", "true".into()),
                            (
                                "capabilities[acss_debit_payments][requested]",
                                "true".into(),
                            ),
                            (
                                if profile.is_some() {
                                    "metadata[buildry_landlord_id]"
                                } else {
                                    "metadata[buildry_user_id]"
                                },
                                profile.unwrap_or(landlord).to_string(),
                            ),
                        ],
                        Some(&format!("custom-{key}")),
                    )
                    .await?;
                let id = account["id"]
                    .as_str()
                    .filter(|s| valid_token(s, "acct_"))
                    .ok_or_else(unavailable)?;
                tx.execute("UPDATE user_stripe_accounts SET account_id=$3 WHERE user_id=$1 AND test_mode=$2",
                    &[&landlord,&stripe.test_mode(),&id]).await.map_err(database_error)?;
            }
            if let Some(profile) = profile {
                tx.execute("UPDATE landlord_stripe_accounts a SET account_id=b.account_id FROM user_stripe_accounts b WHERE a.landlord_id=$1 AND a.test_mode=$2 AND b.user_id=$3 AND b.test_mode=$2", &[&profile,&stripe.test_mode(),&landlord]).await.map_err(database_error)?;
            }
            tx.commit().await.map_err(database_error)?;
            let account = for_user(&*client, landlord, stripe.test_mode())
                .await?
                .ok_or_else(unavailable)?;
            let value = stripe
                .request(Method::GET, &format!("accounts/{}", account.id), &[], None)
                .await?;
            if value["controller"]["requirement_collection"] != "application" {
                return Err((
                    StatusCode::CONFLICT,
                    "This account uses Stripe-managed setup.".into(),
                ));
            }
            if value["business_type"].as_str() != Some(business_type.as_str()) {
                if !value["business_type"].is_null()
                    && (!value["tos_acceptance"]["date"].is_null()
                        || value["payouts_enabled"] == true)
                {
                    return Err((
                        StatusCode::CONFLICT,
                        "The account type has already been confirmed.".into(),
                    ));
                }
                stripe
                    .request(
                        Method::POST,
                        &format!("accounts/{}", account.id),
                        &[("business_type", business_type)],
                        None,
                    )
                    .await?;
            }
        }
        Change::Save {
            account_token,
            bank_token,
            persons,
            registration_file,
            profile_url,
            product_description,
            mcc,
            support_phone,
            accept_terms,
        } => {
            if registration_file
                .as_ref()
                .is_some_and(|f| !valid_token(f, "file_"))
                || account_token
                    .as_ref()
                    .is_some_and(|t| !valid_token(t, "ct_"))
                || bank_token
                    .as_ref()
                    .is_some_and(|t| !valid_token(t, "btok_"))
                || persons.len() > 30
                || persons.iter().any(|p| {
                    !valid_token(&p.token, "cpt_")
                        || !valid_token(&p.key, "")
                        || p.id.as_ref().is_some_and(|id| !valid_token(id, "person_"))
                })
            {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "Invalid setup token. Reload and try again.".into(),
                ));
            }
            let account = for_user(&*client, landlord, stripe.test_mode())
                .await?
                .ok_or((StatusCode::CONFLICT, "Start payout setup first.".into()))?;
            let value = stripe
                .request(Method::GET, &format!("accounts/{}", account.id), &[], None)
                .await?;
            if value["controller"]["requirement_collection"] != "application" {
                return Err((
                    StatusCode::CONFLICT,
                    "This account uses Stripe-managed setup.".into(),
                ));
            }
            let mut form = vec![];
            if let Some(token) = account_token.as_ref() {
                form.push(("account_token", token.clone()));
            }
            if let Some(token) = bank_token.as_ref() {
                form.push(("external_account", token.clone()));
            }
            if let Some(url) = profile_url.filter(|s| !s.is_empty()) {
                let valid = reqwest::Url::parse(&url).ok().is_some_and(|u| {
                    matches!(u.scheme(), "http" | "https")
                        && u.host_str().is_some()
                        && u.username().is_empty()
                        && u.password().is_none()
                });
                if !valid || url.len() > 1024 {
                    return Err((
                        StatusCode::BAD_REQUEST,
                        "Enter a valid business website.".into(),
                    ));
                }
                form.push(("business_profile[url]", url));
            }
            if let Some(description) = product_description.filter(|s| !s.is_empty()) {
                if description.len() > 2000 {
                    return Err((
                        StatusCode::BAD_REQUEST,
                        "Business description is too long.".into(),
                    ));
                }
                form.push(("business_profile[product_description]", description));
            }
            if let Some(phone) = support_phone.filter(|p| !p.is_empty()) {
                if phone.len() > 40 {
                    return Err((
                        StatusCode::BAD_REQUEST,
                        "Check your business phone number.".into(),
                    ));
                }
                form.push(("business_profile[support_phone]", phone));
            }
            if let Some(mcc) = mcc {
                if mcc.len() != 4 || !mcc.bytes().all(|b| b.is_ascii_digit()) {
                    return Err((
                        StatusCode::BAD_REQUEST,
                        "Enter a valid merchant category code.".into(),
                    ));
                }
                form.push(("business_profile[mcc]", mcc));
            }
            let mut terms = vec![];
            if accept_terms {
                let token = account_token.as_ref().ok_or((
                    StatusCode::BAD_REQUEST,
                    "Confirm the Stripe agreement.".into(),
                ))?;
                let details = stripe
                    .request(Method::GET, &format!("tokens/{token}"), &[], None)
                    .await?;
                let ip = details["client_ip"]
                    .as_str()
                    .and_then(|s| s.parse::<std::net::IpAddr>().ok())
                    .unwrap_or(peer.ip());
                terms.push(("tos_acceptance[ip]", ip.to_string()));
                terms.push((
                    "tos_acceptance[date]",
                    details["created"]
                        .as_i64()
                        .ok_or_else(unavailable)?
                        .to_string(),
                ));
                if let Some(agent) = headers.get("user-agent").and_then(|h| h.to_str().ok()) {
                    terms.push((
                        "tos_acceptance[user_agent]",
                        agent.chars().take(500).collect(),
                    ));
                }
            }
            if !persons.is_empty() {
                let existing = stripe
                    .request(
                        Method::GET,
                        &format!("accounts/{}/persons?limit=100", account.id),
                        &[],
                        None,
                    )
                    .await?;
                for person in persons {
                    let id = person.id.clone().or_else(|| {
                        existing["data"]
                            .as_array()
                            .and_then(|values| {
                                values
                                    .iter()
                                    .find(|v| v["metadata"]["buildry_setup_key"] == person.key)
                            })
                            .and_then(|v| v["id"].as_str())
                            .map(str::to_string)
                    });
                    let token = stripe
                        .request(Method::GET, &format!("tokens/{}", person.token), &[], None)
                        .await?;
                    if token["used"] == true && id.is_some() {
                        continue;
                    }
                    let path = match id {
                        Some(id) => format!("accounts/{}/persons/{id}", account.id),
                        None => format!("accounts/{}/persons", account.id),
                    };
                    stripe
                        .request(
                            Method::POST,
                            &path,
                            &[("person_token", person.token.clone())],
                            Some(&format!("setup-{}-{}", account.id, person.token)),
                        )
                        .await?;
                }
            }
            if !form.is_empty() {
                let key = account_token
                    .as_ref()
                    .or(bank_token.as_ref())
                    .map(|t| format!("setup-{}-{t}", account.id));
                stripe
                    .request(
                        Method::POST,
                        &format!("accounts/{}", account.id),
                        &form,
                        key.as_deref(),
                    )
                    .await?;
            }
            if let Some(file) = registration_file {
                stripe
                    .request(
                        Method::POST,
                        &format!("accounts/{}", account.id),
                        &[("documents[proof_of_registration][files][0]", file.clone())],
                        Some(&format!("setup-document-{}-{file}", account.id)),
                    )
                    .await?;
            }
            if !terms.is_empty() {
                stripe
                    .request(
                        Method::POST,
                        &format!("accounts/{}", account.id),
                        &terms,
                        Some(&format!(
                            "setup-terms-{}-{}",
                            account.id,
                            account_token.as_ref().unwrap()
                        )),
                    )
                    .await?;
            }
        }
    }
    Ok(Json(load(&client, &stripe, landlord).await?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_identifiers_cannot_change_stripe_request_paths() {
        assert!(valid_token("ct_test123", "ct_"));
        for value in [
            "",
            "ct_test/../accounts",
            "ct_test?expand[]=account",
            "ct_test%2faccount",
            "btok_test",
        ] {
            assert!(!valid_token(value, "ct_"));
        }
    }

    #[test]
    fn setup_accepts_tokens_and_rejects_raw_bank_details_or_account_selection() {
        assert!(serde_json::from_value::<Change>(
            serde_json::json!({"action":"save","bank_token":"btok_test"})
        )
        .is_ok());
        for field in [
            "account_number",
            "routing_number",
            "account_id",
            "landlord_id",
        ] {
            let mut value = serde_json::json!({"action":"save"});
            value[field] = serde_json::json!("untrusted");
            assert!(serde_json::from_value::<Change>(value).is_err());
        }
    }
}
