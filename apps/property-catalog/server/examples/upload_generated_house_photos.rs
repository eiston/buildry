//! Upload illustrative house photos to the catalog's PostgreSQL gallery.
//! Run from server/: cargo run --release --example upload_generated_house_photos

use std::{env, path::PathBuf};
use tokio_postgres::NoTls;

const HOUSES: &[(i64, &str)] = &[
    (1, "stay-house.png"),
    (2, "stay-house-brick.jpg"),
    (3, "stay-house-townhome.jpg"),
    (4, "stay-house-modern.jpg"),
    (5, "stay-house-red-brick.jpg"),
    (6, "stay-house-cream.jpg"),
];

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://buildry:buildry_dev_only@127.0.0.1:5433/buildry".into());
    let (mut client, connection) = tokio_postgres::connect(&url, NoTls).await?;
    tokio::spawn(async move {
        if let Err(error) = connection.await {
            eprintln!("PostgreSQL connection closed: {error}");
        }
    });
    let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets");
    let transaction = client.transaction().await?;
    let mut uploaded = 0;
    for &(property_id, exterior) in HOUSES {
        let row = transaction
            .query_one(
                "SELECT data::text FROM properties WHERE id=$1 FOR UPDATE",
                &[&property_id],
            )
            .await?;
        let data: serde_json::Value = serde_json::from_str(&row.get::<_, String>(0))?;
        if data["photos"]
            .as_array()
            .is_some_and(|items| !items.is_empty())
        {
            println!("House {property_id} already has photos; leaving them intact");
            continue;
        }
        let files: Vec<&str> = if property_id == 1 {
            vec![exterior, "generated-house-living-room.png"]
        } else {
            vec![exterior]
        };
        let mut photos = Vec::new();
        for filename in files {
            let bytes = tokio::fs::read(assets.join(filename)).await?;
            let content_type = if filename.ends_with(".png") {
                "image/png"
            } else {
                "image/jpeg"
            };
            if bytes.is_empty() || bytes.len() >= 5_000_000 {
                return Err(format!("Invalid or oversized image: {filename}").into());
            }
            let image_id: i64 = transaction
                .query_one(
                    "INSERT INTO property_images (content_type,data) VALUES ($1,$2) RETURNING id",
                    &[&content_type, &bytes],
                )
                .await?
                .get(0);
            photos.push(format!("/images/{image_id}?illustrative=ai"));
        }
        let photos_json = serde_json::to_string(&photos)?;
        transaction.execute(
            "UPDATE properties SET data=jsonb_set(data,'{photos}',$2::text::jsonb,true),updated_at=NOW() WHERE id=$1",
            &[&property_id, &photos_json],
        ).await?;
        uploaded += photos.len();
        println!("House {property_id}: {}", photos.join(", "));
    }
    transaction.commit().await?;
    println!("Uploaded {uploaded} illustrative house photos.");
    Ok(())
}
