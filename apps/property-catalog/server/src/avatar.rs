use tokio_postgres::Client;

const COLORS: [&str; 8] = [
    "#245d55", "#367356", "#376b78", "#596a8b", "#765e7d", "#8a654e", "#39776c", "#526d56",
];

fn avatar_svg(id: i64, first_name: &str, last_name: &str) -> Vec<u8> {
    let initials: String = [first_name, last_name]
        .iter()
        .filter_map(|part| part.chars().find(|ch| ch.is_alphanumeric()))
        .flat_map(char::to_uppercase)
        .collect();
    let initials = if initials.is_empty() { "?" } else { &initials };
    let color = COLORS[(id.unsigned_abs() as usize).wrapping_mul(5) % COLORS.len()];
    format!("<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 64 64'><rect width='64' height='64' rx='32' fill='{color}'/><text x='32' y='34' fill='white' font-family='Arial,sans-serif' font-size='23' font-weight='700' text-anchor='middle' dominant-baseline='middle'>{initials}</text></svg>").into_bytes()
}

/// Store one generated portrait in the user's normal photo field, preserving uploads.
pub async fn ensure_user_avatar(client: &Client, id: i64) -> Result<(), tokio_postgres::Error> {
    let Some(user) = client
        .query_opt(
            "SELECT first_name,last_name,photo_url FROM users WHERE id=$1",
            &[&id],
        )
        .await?
    else {
        return Ok(());
    };
    let first_name: String = user.get(0);
    let last_name: String = user.get(1);
    let photo_url: String = user.get(2);
    let svg = avatar_svg(id, &first_name, &last_name);
    if photo_url.is_empty() {
        let image_id: i64 = client.query_one(
            "INSERT INTO property_images(content_type,data) VALUES('image/svg+xml',$1) RETURNING id",
            &[&svg],
        ).await?.get(0);
        client
            .execute(
                "UPDATE users SET photo_url=$2 WHERE id=$1 AND photo_url=''",
                &[&id, &format!("/images/{image_id}")],
            )
            .await?;
    } else if let Some(image_id) = photo_url
        .strip_prefix("/images/")
        .and_then(|value| value.parse::<i64>().ok())
    {
        client.execute(
            "UPDATE property_images SET data=$2 WHERE id=$1 AND content_type='image/svg+xml' AND data<>$2",
            &[&image_id,&svg],
        ).await?;
    }
    Ok(())
}

pub async fn backfill_user_avatars(client: &Client) -> Result<(), tokio_postgres::Error> {
    for row in client
        .query(
            "SELECT id FROM users WHERE role IN ('admin','staff','landlord')",
            &[],
        )
        .await?
    {
        ensure_user_avatar(client, row.get(0)).await?;
    }
    Ok(())
}
