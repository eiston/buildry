use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://buildry:buildry_dev_only@127.0.0.1:5433/buildry".into());
    let (mut client, connection) = tokio_postgres::connect(&url, tokio_postgres::NoTls).await?;
    tokio::spawn(async move {
        let _ = connection.await;
    });
    let transaction = client.transaction().await?;
    let rows = transaction.query(
        "SELECT property_id, id, (data->>'current_monthly_rent')::int FROM property_spaces \
         WHERE data->>'kind' IN ('Bedroom', 'Suite') AND data->>'current_monthly_rent' IS NOT NULL \
         AND (data->>'rent_sep_dec' IS NULL OR data->>'rent_jan_apr' IS NULL OR data->>'rent_may_aug' IS NULL)",
        &[],
    ).await?;
    for row in &rows {
        let property_id: i64 = row.get(0);
        let space_id: i64 = row.get(1);
        let base: i32 = row.get(2);
        let difference = if base >= 1400 {
            150
        } else if base >= 1000 {
            125
        } else {
            100
        };
        let sep = base + difference;
        let may = (base - difference).max(1);
        transaction
            .execute(
                "UPDATE property_spaces SET data = jsonb_set(jsonb_set(jsonb_set(data, \
             '{rent_sep_dec}', COALESCE(data->'rent_sep_dec', to_jsonb($3::int))), \
             '{rent_jan_apr}', COALESCE(data->'rent_jan_apr', to_jsonb($4::int))), \
             '{rent_may_aug}', COALESCE(data->'rent_may_aug', to_jsonb($5::int))) \
             WHERE property_id = $1 AND id = $2",
                &[&property_id, &space_id, &sep, &base, &may],
            )
            .await?;
    }
    transaction.commit().await?;
    println!("Saved example seasonal rents for {} rooms.", rows.len());
    Ok(())
}
