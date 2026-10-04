use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct GuestRoom {
    pub id: u32,
    pub name: String,
    pub kind: String,
    pub bathroom_access: String,
    #[serde(default)]
    pub photos: Vec<String>,
    pub monthly_price: Option<u32>,
    pub price_is_estimate: bool,
    pub rent_sep_dec: Option<u32>,
    pub rent_jan_apr: Option<u32>,
    pub rent_may_aug: Option<u32>,
    #[serde(default)]
    pub amenities: Vec<String>,
    #[serde(default)]
    pub unavailable_periods: Vec<String>,
    #[serde(default)]
    pub availability_confirmed: bool,
}

pub fn term_label(start: &str, months: u8) -> Option<&'static str> {
    match (start, months) {
        ("sep", 4) => Some("September–December"),
        ("sep", 8) => Some("September–April"),
        ("sep", 12) => Some("September–August"),
        ("jan", 4) => Some("January–April"),
        ("jan", 8) => Some("January–August"),
        ("jan", 12) => Some("January–December"),
        ("may", 4) => Some("May–August"),
        ("may", 8) => Some("May–December"),
        ("may", 12) => Some("May–April"),
        _ => None,
    }
}

pub fn averaged_monthly_rent(sep: u32, jan: u32, may: u32, start: &str, months: u8) -> Option<u32> {
    let first = match start {
        "sep" => 0,
        "jan" => 1,
        "may" => 2,
        _ => return None,
    };
    let periods = match months {
        4 | 8 | 12 | 24 | 36 => (months / 4) as usize,
        _ => return None,
    };
    let prices = [sep, jan, may];
    let sum: u64 = (0..periods)
        .map(|offset| prices[(first + offset) % 3] as u64)
        .sum();
    Some(((sum + periods as u64 / 2) / periods as u64) as u32)
}

pub fn period_keys(year: i32, start: &str, months: u8) -> Option<Vec<String>> {
    if !(2020..=2100).contains(&year) || ![4, 8, 12, 24, 36].contains(&months) {
        return None;
    }
    let first = match start {
        "jan" => 0,
        "may" => 1,
        "sep" => 2,
        _ => return None,
    };
    Some(
        (0..(months / 4) as usize)
            .map(|offset| {
                let slot = first + offset;
                format!(
                    "{}-{}",
                    year + (slot / 3) as i32,
                    ["jan", "may", "sep"][slot % 3]
                )
            })
            .collect(),
    )
}

pub fn rental_period_label(year: i32, start: &str, months: u8) -> String {
    let Some(keys) = period_keys(year, start, months) else {
        return "Choose a rental period".into();
    };
    let last = keys.last().unwrap();
    let end_year = last.split('-').next().unwrap_or_default();
    let end_month = match last.split('-').nth(1) {
        Some("sep") => "Dec",
        Some("jan") => "Apr",
        _ => "Aug",
    };
    let start_month = match start {
        "sep" => "Sep",
        "jan" => "Jan",
        _ => "May",
    };
    format!("{start_month} {year} – {end_month} {end_year} · {months} months")
}

impl GuestRoom {
    pub fn is_available(&self, year: i32, start: &str, months: u8) -> bool {
        period_keys(year, start, months).is_some_and(|keys| {
            keys.iter()
                .all(|key| !self.unavailable_periods.contains(key))
        })
    }
    pub fn quoted_monthly_rent(&self, start: &str, months: u8) -> Option<u32> {
        averaged_monthly_rent(
            self.rent_sep_dec?,
            self.rent_jan_apr?,
            self.rent_may_aug?,
            start,
            months,
        )
    }

    pub fn lowest_monthly_rent(&self) -> Option<u32> {
        [self.rent_sep_dec, self.rent_jan_apr, self.rent_may_aug]
            .into_iter()
            .flatten()
            .min()
            .or(self.monthly_price)
    }
}

#[cfg(test)]
mod tests {
    use super::{averaged_monthly_rent, period_keys, rental_period_label, term_label, GuestRoom};

    #[test]
    fn averages_consecutive_seasons() {
        assert_eq!(averaged_monthly_rent(1200, 1000, 800, "sep", 4), Some(1200));
        assert_eq!(averaged_monthly_rent(1200, 1000, 800, "sep", 8), Some(1100));
        assert_eq!(averaged_monthly_rent(1200, 1000, 800, "jan", 8), Some(900));
        assert_eq!(averaged_monthly_rent(1200, 1000, 800, "may", 8), Some(1000));
        assert_eq!(
            averaged_monthly_rent(1200, 1000, 800, "sep", 12),
            Some(1000)
        );
        assert_eq!(averaged_monthly_rent(1200, 1000, 800, "sep", 6), None);
        assert_eq!(
            averaged_monthly_rent(1200, 1000, 800, "sep", 24),
            Some(1000)
        );
        assert_eq!(term_label("may", 8), Some("May–December"));
        assert_eq!(
            period_keys(2027, "sep", 8),
            Some(vec!["2027-sep".into(), "2028-jan".into()])
        );
        assert_eq!(
            period_keys(2027, "may", 12),
            Some(vec![
                "2027-may".into(),
                "2027-sep".into(),
                "2028-jan".into()
            ])
        );
        assert_eq!(
            rental_period_label(2027, "sep", 24),
            "Sep 2027 – Aug 2029 · 24 months"
        );
    }

    #[test]
    fn blocked_block_makes_multi_year_stay_unavailable() {
        let room = GuestRoom {
            id: 1,
            name: "Room".into(),
            kind: "Bedroom".into(),
            bathroom_access: String::new(),
            photos: Vec::new(),
            monthly_price: Some(900),
            price_is_estimate: true,
            rent_sep_dec: Some(1000),
            rent_jan_apr: Some(900),
            rent_may_aug: Some(800),
            amenities: Vec::new(),
            unavailable_periods: vec!["2028-jan".into()],
            availability_confirmed: false,
        };
        assert!(room.is_available(2027, "jan", 12));
        assert!(!room.is_available(2027, "sep", 24));
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct GuestListing {
    pub id: u32,
    pub name: String,
    pub address: String,
    pub area: String,
    #[serde(default)]
    pub photos: Vec<String>,
    pub rooms: Vec<GuestRoom>,
    #[serde(default)]
    pub amenities: Vec<String>,
    pub tours_available: bool,
}
