use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct PricePoint {
    pub month: u8,
    pub percent: u32,
}

pub fn starter_price_curve() -> Vec<PricePoint> {
    [(1, 90), (3, 95), (5, 105), (7, 115), (9, 125), (11, 105)]
        .into_iter()
        .map(|(month, percent)| PricePoint { month, percent })
        .collect()
}

pub fn price_for_month(base: u32, points: &[PricePoint], month: u8) -> u32 {
    let fallback = starter_price_curve();
    let mut curve = if points.is_empty() {
        fallback
    } else {
        points.to_vec()
    };
    curve.sort_by_key(|point| point.month);
    let previous = curve
        .iter()
        .rev()
        .find(|point| point.month <= month)
        .unwrap_or_else(|| curve.last().unwrap());
    let next = curve
        .iter()
        .find(|point| point.month > month)
        .unwrap_or(&curve[0]);
    let previous_month = previous.month as u32;
    let next_month = if next.month <= previous.month {
        next.month as u32 + 12
    } else {
        next.month as u32
    };
    let target_month = if month < previous.month {
        month as u32 + 12
    } else {
        month as u32
    };
    let span = next_month - previous_month;
    let percent = if span == 0 {
        previous.percent
    } else {
        ((previous.percent as u64 * (next_month - target_month) as u64
            + next.percent as u64 * (target_month - previous_month) as u64
            + span as u64 / 2)
            / span as u64) as u32
    };
    ((base as u64 * percent as u64 + 50) / 100).min(u32::MAX as u64) as u32
}

#[cfg(test)]
mod price_tests {
    use super::{price_for_month, starter_price_curve};

    #[test]
    fn interpolates_annual_curve() {
        let points = starter_price_curve();
        assert_eq!(price_for_month(1000, &points, 1), 900);
        assert_eq!(price_for_month(1000, &points, 9), 1250);
        assert_eq!(price_for_month(1000, &points, 4), 1000);
        assert_eq!(price_for_month(1000, &points, 12), 980);
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Space {
    pub id: u32,
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub parent_id: Option<u32>,
    #[serde(default)]
    pub level: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub amenities: Vec<String>,
    #[serde(default)]
    pub photos: Vec<String>,
    #[serde(default)]
    pub bathroom_access: String,
    #[serde(default)]
    pub current_monthly_rent: Option<u32>,
    #[serde(default)]
    pub rent_is_estimate: bool,
    #[serde(default)]
    pub rent_sep_dec: Option<u32>,
    #[serde(default)]
    pub rent_jan_apr: Option<u32>,
    #[serde(default)]
    pub rent_may_aug: Option<u32>,
    #[serde(default)]
    pub seasonal_prices_confirmed: bool,
    #[serde(default)]
    pub price_curve: Vec<PricePoint>,
    #[serde(default)]
    pub unavailable_periods: Vec<String>,
    #[serde(default)]
    pub availability_confirmed: bool,
}

impl Space {
    pub fn new(id: u32) -> Self {
        Self {
            id,
            name: String::new(),
            kind: "Bedroom".into(),
            parent_id: None,
            level: String::new(),
            notes: String::new(),
            amenities: Vec::new(),
            photos: Vec::new(),
            bathroom_access: String::new(),
            current_monthly_rent: None,
            rent_is_estimate: false,
            rent_sep_dec: None,
            rent_jan_apr: None,
            rent_may_aug: None,
            seasonal_prices_confirmed: false,
            price_curve: Vec::new(),
            unavailable_periods: Vec::new(),
            availability_confirmed: false,
        }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Property {
    pub id: u32,
    pub name: String,
    pub area: String,
    pub address: String,
    #[serde(default = "unset_kind")]
    pub kind: String,
    #[serde(default)]
    pub ensuite_rooms: u32,
    #[serde(default)]
    pub shared_rooms: u32,
    #[serde(default)]
    pub suites: u32,
    pub notes: String,
    #[serde(default)]
    pub amenities: Vec<String>,
    #[serde(default)]
    pub photos: Vec<String>,
    #[serde(default)]
    pub latitude: Option<f64>,
    #[serde(default)]
    pub longitude: Option<f64>,
    #[serde(default)]
    pub map_label: Option<String>,
    #[serde(default)]
    pub spaces: Vec<Space>,
}

fn unset_kind() -> String {
    "Not set".into()
}

impl Property {
    pub fn new(id: u32) -> Self {
        Self {
            id,
            name: String::new(),
            area: String::new(),
            address: String::new(),
            kind: unset_kind(),
            ensuite_rooms: 0,
            shared_rooms: 0,
            suites: 0,
            notes: String::new(),
            amenities: Vec::new(),
            photos: Vec::new(),
            latitude: None,
            longitude: None,
            map_label: None,
            spaces: Vec::new(),
        }
    }

    pub fn kind_count(&self, kind: &str) -> usize {
        self.spaces
            .iter()
            .filter(|space| space.kind == kind)
            .count()
    }

    pub fn inventory_label(&self) -> String {
        let bedrooms = self.kind_count("Bedroom");
        let suites = self.kind_count("Suite");
        if self.spaces.is_empty() {
            "Inventory pending".into()
        } else if suites == 0 {
            format!("{bedrooms} bedrooms")
        } else {
            let bed_label = if bedrooms == 1 { "bed" } else { "beds" };
            format!(
                "{bedrooms} {bed_label} · {suites} suite{}",
                if suites == 1 { "" } else { "s" }
            )
        }
    }

    // Existing browser catalogs used only aggregate counts. Preserve those
    // counts as basic physical records if they are encountered on import.
    pub fn migrate_legacy_counts(&mut self) {
        if !self.spaces.is_empty() {
            return;
        }
        let mut next_id = 1;
        for (count, prefix, kind) in [
            (self.ensuite_rooms, "Ensuite bedroom", "Bedroom"),
            (self.shared_rooms, "Shared-bath bedroom", "Bedroom"),
            (self.suites, "Suite", "Area"),
        ] {
            for index in 1..=count {
                self.spaces.push(Space {
                    id: next_id,
                    name: format!("{prefix} {index}"),
                    kind: kind.into(),
                    parent_id: None,
                    level: String::new(),
                    notes: "Imported from the earlier catalog count; details to confirm.".into(),
                    amenities: Vec::new(),
                    photos: Vec::new(),
                    bathroom_access: if prefix == "Ensuite bedroom" {
                        "Ensuite".into()
                    } else {
                        String::new()
                    },
                    current_monthly_rent: None,
                    rent_is_estimate: false,
                    rent_sep_dec: None,
                    rent_jan_apr: None,
                    rent_may_aug: None,
                    seasonal_prices_confirmed: false,
                    price_curve: Vec::new(),
                    unavailable_periods: Vec::new(),
                    availability_confirmed: false,
                });
                next_id += 1;
            }
        }
    }
}
