use crate::model::{effective_price_curve, price_for_month, MonthlyPriceOverride, PricePoint};
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct OccupiedPeriod {
    pub start_date: String,
    pub end_date: String,
}

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
    pub monthly_price_overrides: Vec<MonthlyPriceOverride>,
    #[serde(default)]
    pub amenities: Vec<String>,
    #[serde(default)]
    pub occupied_periods: Vec<OccupiedPeriod>,
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
        if period_keys(year, start, months).is_none() {
            return false;
        }
        let month = match start {
            "jan" => 1,
            "may" => 5,
            "sep" => 9,
            _ => return false,
        };
        self.is_month_range_available(year, month, months)
    }

    pub fn is_month_range_available(&self, year: i32, month: u32, months: u8) -> bool {
        if !(1..=12).contains(&month) || months == 0 {
            return false;
        }
        let start = format!("{year:04}-{month:02}-01");
        let end_absolute = year * 12 + month as i32 - 1 + months as i32;
        let end = format!(
            "{:04}-{:02}-01",
            end_absolute.div_euclid(12),
            end_absolute.rem_euclid(12) + 1
        );
        self.occupied_periods.iter().all(|period| {
            period.end_date.as_str() < start.as_str() || period.start_date.as_str() >= end.as_str()
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
            let absolute = year * 12 + start_month as i32 - 1 + offset as i32;
            let month = (absolute.rem_euclid(12) + 1) as u8;
            let month_year = absolute.div_euclid(12);
            let price = self
                .monthly_price_overrides
                .iter()
                .find(|item| item.year == month_year && item.month == month)
                .map(|item| item.price)
                .unwrap_or_else(|| price_for_month(base, &curve, month));
            total += price as u64;
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
    use super::{period_keys, rental_period_label, term_label, GuestRoom, OccupiedPeriod};
    use crate::model::{starter_price_curve, MonthlyPriceOverride};

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
    fn multi_year_stay_uses_the_price_curve() {
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
            monthly_price_overrides: Vec::new(),
            amenities: Vec::new(),
            occupied_periods: Vec::new(),
        };
        assert!(room.is_available(2027, "jan", 12));
        assert!(room.is_available(2027, "sep", 24));
        assert!(room.is_available(2027, "may", 4));
        assert_eq!(room.curve_monthly_rent(2027, "sep", 4), Some(970));
        let mut occupied = room.clone();
        occupied.occupied_periods.push(OccupiedPeriod {
            start_date: "2027-06-15".into(),
            end_date: "2027-10-31".into(),
        });
        assert!(!occupied.is_available(2027, "may", 4));
        assert!(!occupied.is_available(2027, "sep", 4));
        assert!(occupied.is_available(2028, "jan", 4));
        assert!(occupied.is_month_range_available(2027, 5, 1));
        assert!(!occupied.is_month_range_available(2027, 6, 1));
        let mut higher_base = room.clone();
        higher_base.monthly_price = Some(1200);
        higher_base.price_curve = starter_price_curve();
        assert_eq!(higher_base.curve_monthly_rent(2027, "sep", 4), Some(1329));
    }

    #[test]
    fn quote_uses_calendar_overrides_across_years() {
        let mut room = GuestRoom {
            id: 1,
            name: "Room".into(),
            kind: "Bedroom".into(),
            bathroom_access: String::new(),
            photos: Vec::new(),
            monthly_price: Some(1000),
            price_is_estimate: false,
            rent_sep_dec: None,
            rent_jan_apr: None,
            rent_may_aug: None,
            price_curve: Vec::new(),
            monthly_price_overrides: vec![
                MonthlyPriceOverride {
                    year: 2026,
                    month: 12,
                    price: 1200,
                },
                MonthlyPriceOverride {
                    year: 2027,
                    month: 1,
                    price: 1400,
                },
            ],
            amenities: Vec::new(),
            occupied_periods: Vec::new(),
        };
        assert_eq!(room.curve_monthly_rent(2026, "sep", 8), Some(1075));
        room.monthly_price_overrides.clear();
        assert_eq!(room.curve_monthly_rent(2026, "sep", 8), Some(1000));
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct GuestListing {
    pub id: u32,
    pub name: String,
    pub area: String,
    #[serde(default)]
    pub nearby_intersection: Option<String>,
    #[serde(default)]
    pub photos: Vec<String>,
    pub rooms: Vec<GuestRoom>,
    #[serde(default)]
    pub amenities: Vec<String>,
    pub tours_available: bool,
}
