//! Add the supplementary illustrative gallery images to every house and rentable room.
//! Run from server/: cargo run --release --example upload_generated_gallery

use std::{env, path::PathBuf};
use tokio_postgres::NoTls;

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
    let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets/generated-gallery");
    let mut entries = std::fs::read_dir(&assets)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    let transaction = client.transaction().await?;
    let mut uploaded = 0;
    for entry in entries {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "png") {
            continue;
        }
        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
        let parts: Vec<&str> = stem.split('-').collect();
        if parts.first() != Some(&"house") {
            continue;
        }
        let property_id: i64 = parts.get(1).ok_or("Missing house ID")?.parse()?;
        let room_id = if parts.get(2) == Some(&"room") {
            Some(parts.get(3).ok_or("Missing room ID")?.parse::<i64>()?)
        } else {
            None
        };
        let data: String = if let Some(room_id) = room_id {
            transaction.query_one(
                "SELECT data::text FROM property_spaces WHERE property_id=$1 AND id=$2 FOR UPDATE",
                &[&property_id, &room_id],
            ).await?.get(0)
        } else {
            transaction
                .query_one(
                    "SELECT data::text FROM properties WHERE id=$1 FOR UPDATE",
                    &[&property_id],
                )
                .await?
                .get(0)
        };
        let value: serde_json::Value = serde_json::from_str(&data)?;
        let mut photos: Vec<String> = value["photos"]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        let marker = format!("&asset={stem}");
        if photos.iter().any(|photo| photo.ends_with(&marker)) {
            println!("Already uploaded: {stem}");
            continue;
        }
        if photos.len() >= 20 {
            return Err(format!("Gallery full: {stem}").into());
        }
        let bytes = tokio::fs::read(&path).await?;
        if bytes.is_empty() || bytes.len() > 5_000_000 || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            return Err(format!("Invalid or oversized PNG: {stem}").into());
        }
        let image_id: i64 = transaction.query_one(
            "INSERT INTO property_images (content_type,data) VALUES ('image/png',$1) RETURNING id",
            &[&bytes],
        ).await?.get(0);
        photos.push(format!("/images/{image_id}?illustrative=ai{marker}"));
        let photos_json = serde_json::to_string(&photos)?;
        if let Some(room_id) = room_id {
            transaction.execute(
                "UPDATE property_spaces SET data=jsonb_set(data,'{photos}',$3::text::jsonb,true) WHERE property_id=$1 AND id=$2",
                &[&property_id, &room_id, &photos_json],
            ).await?;
        } else {
            transaction.execute(
                "UPDATE properties SET data=jsonb_set(data,'{photos}',$2::text::jsonb,true),updated_at=NOW() WHERE id=$1",
                &[&property_id, &photos_json],
            ).await?;
        }
        uploaded += 1;
        println!("{stem}: /images/{image_id}");
    }
    transaction.commit().await?;
    println!("Uploaded {uploaded} supplementary illustrative gallery images.");
    Ok(())
}
