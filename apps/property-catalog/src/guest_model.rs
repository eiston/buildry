use crate::model::{effective_price_curve, price_for_month, PricePoint};
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
    pub price_curve: Vec<PricePoint>,
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
    pub fn curve_monthly_rent(&self, year: i32, start: &str, months: u8) -> Option<u32> {
        period_keys(year, start, months)?;
        let start_month = match start {
            "jan" => 1_u32,
            "may" => 5,
            "sep" => 9,
            _ => return None,
        };
        let base = self
            .monthly_price
            .or(self.rent_jan_apr)
            .or(self.rent_sep_dec)
            .or(self.rent_may_aug)?;
        let curve = effective_price_curve(
            &self.price_curve,
            Some(base),
            self.rent_jan_apr,
            self.rent_may_aug,
            self.rent_sep_dec,
        );
        let mut total = 0_u64;
        for offset in 0..months as u32 {
            let month = ((start_month - 1 + offset) % 12 + 1) as u8;
            total += price_for_month(base, &curve, month) as u64;
        }
        Some(((total + months as u64 / 2) / months as u64) as u32)
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
    use super::{period_keys, rental_period_label, term_label, GuestRoom};
    use crate::model::starter_price_curve;

    #[test]
    fn consecutive_periods_span_calendar_years() {
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
            price_curve: Vec::new(),
            amenities: Vec::new(),
            unavailable_periods: vec!["2028-jan".into()],
            availability_confirmed: false,
        };
        assert!(room.is_available(2027, "jan", 12));
        assert!(!room.is_available(2027, "sep", 24));
        assert_eq!(room.curve_monthly_rent(2027, "sep", 4), Some(970));
        let mut higher_base = room.clone();
        higher_base.monthly_price = Some(1200);
        higher_base.price_curve = starter_price_curve();
        assert_eq!(higher_base.curve_monthly_rent(2027, "sep", 4), Some(1329));
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
