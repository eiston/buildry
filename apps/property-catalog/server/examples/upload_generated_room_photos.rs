//! Upload the project's illustrative room images into the catalog database.
//! Run from server/: cargo run --release --example upload_generated_room_photos

use std::{env, path::PathBuf};
use tokio_postgres::NoTls;

const ROOMS: &[(i64, i64)] = &[
    (1, 2),
    (1, 3),
    (1, 4),
    (1, 5),
    (1, 6),
    (2, 1),
    (2, 2),
    (2, 3),
    (2, 4),
    (3, 1),
    (3, 2),
    (3, 3),
    (3, 4),
    (3, 5),
    (4, 1),
    (4, 2),
    (4, 3),
    (4, 4),
    (4, 5),
    (4, 6),
    (5, 1),
    (5, 3),
    (6, 1),
    (6, 2),
    (6, 3),
    (6, 4),
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

    let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets/generated-rooms");
    let transaction = client.transaction().await?;
    let mut uploaded = 0;
    for &(property_id, room_id) in ROOMS {
        let filename = format!("house-{property_id}-room-{room_id}.jpg");
        let bytes = tokio::fs::read(assets.join(&filename)).await?;
        if bytes.is_empty() || bytes.len() >= 5_000_000 || !bytes.starts_with(&[0xff, 0xd8]) {
            return Err(format!("Invalid or oversized JPEG: {filename}").into());
        }

        let row = transaction
            .query_one(
                "SELECT data::text FROM property_spaces WHERE property_id = $1 AND id = $2 FOR UPDATE",
                &[&property_id, &room_id],
            )
            .await?;
        let data: serde_json::Value = serde_json::from_str(&row.get::<_, String>(0))?;
        let photos = data["photos"].as_array();
        if photos.is_some_and(|items| !items.is_empty()) {
            if photos.is_some_and(|items| {
                items.len() == 1
                    && items[0]
                        .as_str()
                        .is_some_and(|url| url.ends_with("?illustrative=ai"))
            }) {
                println!("Already uploaded: {filename}");
                continue;
            }
            return Err(format!(
                "Room {property_id}/{room_id} already has photos; leaving them intact"
            )
            .into());
        }

        let image_id: i64 = transaction
            .query_one(
                "INSERT INTO property_images (content_type, data) VALUES ('image/jpeg', $1) RETURNING id",
                &[&bytes],
            )
            .await?
            .get(0);
        let image_url = format!("/images/{image_id}?illustrative=ai");
        let photos_json = serde_json::to_string(&vec![image_url.clone()])?;
        transaction
            .execute(
                "UPDATE property_spaces SET data = jsonb_set(data, '{photos}', $3::text::jsonb, true) WHERE property_id = $1 AND id = $2",
                &[&property_id, &room_id, &photos_json],
            )
            .await?;
        uploaded += 1;
        println!("{property_id}/{room_id}: {image_url}");
    }
    transaction.commit().await?;
    println!("Uploaded {uploaded} illustrative room photos.");
    Ok(())
}
