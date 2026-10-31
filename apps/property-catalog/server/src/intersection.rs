use quick_xml::{
    events::{BytesStart, Event},
    Reader,
};
use std::{
    collections::HashMap,
    sync::OnceLock,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

type Cache = Mutex<HashMap<(i64, i64), (Instant, Option<String>)>>;
static CACHE: OnceLock<Cache> = OnceLock::new();

fn distance_km(a_lat: f64, a_lon: f64, b_lat: f64, b_lon: f64) -> f64 {
    let lat = (b_lat - a_lat).to_radians();
    let lon = (b_lon - a_lon).to_radians();
    let a = (lat / 2.0).sin().powi(2)
        + a_lat.to_radians().cos() * b_lat.to_radians().cos() * (lon / 2.0).sin().powi(2);
    6371.0 * 2.0 * a.sqrt().asin()
}

fn road_rank(kind: &str) -> u8 {
    match kind {
        "trunk" | "primary" | "secondary" => 0,
        "tertiary" => 1,
        "residential" | "living_street" | "unclassified" => 2,
        _ => 3,
    }
}

fn attr(element: &BytesStart<'_>, key: &[u8]) -> Option<String> {
    element
        .attributes()
        .flatten()
        .find(|item| item.key.as_ref() == key)
        .and_then(|item| item.unescape_value().ok().map(|value| value.into_owned()))
}

fn parse_map(xml: &str, latitude: f64, longitude: f64) -> Option<String> {
    let mut reader = Reader::from_str(xml);
    let mut nodes = HashMap::new();
    let mut ways = Vec::new();
    let mut current: Option<(Vec<i64>, Option<String>, Option<String>)> = None;
    loop {
        match reader.read_event() {
            Ok(Event::Empty(element)) if element.name().as_ref() == b"node" => {
                if let (Some(id), Some(lat), Some(lon)) = (
                    attr(&element, b"id").and_then(|v| v.parse::<i64>().ok()),
                    attr(&element, b"lat").and_then(|v| v.parse::<f64>().ok()),
                    attr(&element, b"lon").and_then(|v| v.parse::<f64>().ok()),
                ) {
                    nodes.insert(id, (lat, lon));
                }
            }
            Ok(Event::Start(element)) if element.name().as_ref() == b"way" => {
                current = Some((Vec::new(), None, None))
            }
            Ok(Event::Empty(element)) if element.name().as_ref() == b"nd" => {
                if let Some((ids, _, _)) = current.as_mut() {
                    if let Some(id) = attr(&element, b"ref").and_then(|v| v.parse::<i64>().ok()) {
                        ids.push(id);
                    }
                }
            }
            Ok(Event::Empty(element)) if element.name().as_ref() == b"tag" => {
                if let Some((_, name, kind)) = current.as_mut() {
                    match attr(&element, b"k").as_deref() {
                        Some("name") => *name = attr(&element, b"v"),
                        Some("highway") => *kind = attr(&element, b"v"),
                        _ => {}
                    }
                }
            }
            Ok(Event::End(element)) if element.name().as_ref() == b"way" => {
                if let Some((ids, Some(name), Some(kind))) = current.take() {
                    if road_rank(&kind) < 3 {
                        ways.push((ids, name, road_rank(&kind)));
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => return None,
            _ => {}
        }
    }
    let mut roads: HashMap<i64, Vec<(String, u8)>> = HashMap::new();
    for (ids, name, rank) in ways {
        for id in ids {
            let entry = roads.entry(id).or_default();
            if !entry.iter().any(|(road, _)| road == &name) {
                entry.push((name.clone(), rank));
            }
        }
    }
    let mut candidates = Vec::new();
    for (id, names) in roads {
        if names.len() < 2 {
            continue;
        }
        let Some((lat, lon)) = nodes.get(&id) else {
            continue;
        };
        let distance = distance_km(latitude, longitude, *lat, *lon);
        if distance > 1.2 {
            continue;
        }
        for (index, first) in names.iter().enumerate() {
            for second in names.iter().skip(index + 1) {
                let rank = first.1.min(second.1);
                let score = distance + f64::from(rank) * 2.0;
                candidates.push((score, format!("{} & {}", first.0, second.0)));
            }
        }
    }
    candidates.sort_by(|a, b| a.0.total_cmp(&b.0));
    candidates.into_iter().next().map(|(_, label)| label)
}

async fn lookup(latitude: f64, longitude: f64) -> Option<String> {
    let bbox = format!(
        "{},{},{},{}",
        longitude - 0.008,
        latitude - 0.006,
        longitude + 0.008,
        latitude + 0.006
    );
    let url = format!("https://api.openstreetmap.org/api/0.6/map?bbox={bbox}");
    let response = reqwest::Client::builder()
        .timeout(Duration::from_secs(7))
        .build()
        .ok()?
        .get(url)
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let xml = response.text().await.ok()?;
    parse_map(&xml, latitude, longitude)
}

pub async fn nearest(latitude: f64, longitude: f64) -> Option<String> {
    if !latitude.is_finite() || !longitude.is_finite() {
        return None;
    }
    let key = (
        (latitude * 100_000.0).round() as i64,
        (longitude * 100_000.0).round() as i64,
    );
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some((time, value)) = cache.lock().await.get(&key) {
        if time.elapsed() < Duration::from_secs(3600) {
            return value.clone();
        }
    }
    let value = lookup(latitude, longitude).await;
    cache
        .lock()
        .await
        .insert(key, (Instant::now(), value.clone()));
    value
}

#[cfg(test)]
mod tests {
    use super::parse_map;

    #[test]
    fn finds_shared_road_node_without_exposing_a_house_number() {
        let map = r#"<osm>
            <node id="1" lat="43.4900" lon="-80.5180"/>
            <node id="2" lat="43.4910" lon="-80.5180"/>
            <way id="10"><nd ref="1"/><nd ref="2"/><tag k="highway" v="primary"/><tag k="name" v="King Street"/></way>
            <way id="11"><nd ref="1"/><tag k="highway" v="residential"/><tag k="name" v="Dearborn Boulevard"/></way>
        </osm>"#;
        assert_eq!(
            parse_map(map, 43.4901, -80.5180).as_deref(),
            Some("King Street & Dearborn Boulevard")
        );
    }
}
