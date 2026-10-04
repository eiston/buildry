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
        "SELECT property_id, id, data->>'name', data->>'kind' FROM property_spaces WHERE data->>'kind' IN ('Bedroom', 'Suite') AND (data->>'current_monthly_rent') IS NULL ORDER BY property_id, id",
        &[],
    ).await?;
    for row in &rows {
        let property_id: i64 = row.get(0);
        let space_id: i64 = row.get(1);
        let name: String = row.get(2);
        let kind: String = row.get(3);
        let lower = name.to_lowercase();
        let rent: i32 = if kind == "Suite" {
            1550
        } else if lower.contains("master") {
            1100
        } else if lower.contains("ensuite") {
            950
        } else {
            850
        };
        transaction.execute(
            "UPDATE property_spaces SET data = jsonb_set(jsonb_set(data, '{current_monthly_rent}', to_jsonb($3::int)), '{rent_is_estimate}', 'true'::jsonb) WHERE property_id = $1 AND id = $2 AND (data->>'current_monthly_rent') IS NULL",
            &[&property_id, &space_id, &rent],
        ).await?;
        println!("Property {property_id}, room {space_id} ({name}): C${rent}/month (example)");
    }
    transaction.commit().await?;
    println!("Saved {} example room rents.", rows.len());
    Ok(())
}
