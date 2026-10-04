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
    let rows = transaction
        .query(
            "SELECT property_id, id, row_number() OVER (PARTITION BY property_id ORDER BY id) \
         FROM property_spaces WHERE data->>'kind' IN ('Bedroom', 'Suite') \
         AND data->'unavailable_periods' IS NULL ORDER BY property_id, id",
            &[],
        )
        .await?;
    for row in &rows {
        let property_id: i64 = row.get(0);
        let room_id: i64 = row.get(1);
        let rank: i64 = row.get(2);
        let periods: Vec<&str> = match rank {
            1 => vec!["2027-jan", "2027-may"],
            2 if property_id % 2 == 0 => vec!["2027-sep", "2028-jan"],
            3 if property_id % 2 == 1 => vec!["2028-may"],
            _ => Vec::new(),
        };
        let encoded = serde_json::to_string(&periods)?;
        transaction.execute(
            "UPDATE property_spaces SET data = jsonb_set(data, '{unavailable_periods}', $3::text::jsonb) \
             WHERE property_id = $1 AND id = $2 AND data->'unavailable_periods' IS NULL",
            &[&property_id, &room_id, &encoded],
        ).await?;
    }
    transaction.commit().await?;
    println!("Saved example availability for {} rooms.", rows.len());
    Ok(())
}
