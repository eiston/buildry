use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct PricePoint {
    pub month: u8,
    pub percent: f64,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct MonthlyPriceOverride {
    pub year: i32,
    pub month: u8,
    pub price: u32,
}

pub fn starter_price_curve() -> Vec<PricePoint> {
    [(1, 90), (3, 95), (5, 105), (7, 115), (9, 125), (11, 105)]
        .into_iter()
        .map(|(month, percent)| PricePoint {
            month,
            percent: percent as f64,
        })
        .collect()
}

pub fn effective_price_curve(
    points: &[PricePoint],
    base: Option<u32>,
    jan: Option<u32>,
    may: Option<u32>,
    sep: Option<u32>,
) -> Vec<PricePoint> {
    if !points.is_empty() {
        return points.to_vec();
    }
    if let (Some(base), Some(jan), Some(may), Some(sep)) = (base, jan, may, sep) {
        if base > 0 {
            return [(2, jan), (6, may), (10, sep)]
                .into_iter()
                .map(|(month, rent)| PricePoint {
                    month,
                    percent: ((rent as f64 * 100.0 / base as f64).round()).clamp(1.0, 500.0),
                })
                .collect();
        }
    }
    vec![
        PricePoint {
            month: 1,
            percent: 100.0,
        },
        PricePoint {
            month: 7,
            percent: 100.0,
        },
    ]
}

pub fn percent_for_month(points: &[PricePoint], month: u8) -> f64 {
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
    let result = if span == 0 {
        previous.percent
    } else {
        (previous.percent * (next_month - target_month) as f64
            + next.percent * (target_month - previous_month) as f64)
            / span as f64
    };
    if curve.iter().all(|point| point.percent.fract() == 0.0) {
        result.round()
    } else {
        result
    }
}

pub fn price_for_month(base: u32, points: &[PricePoint], month: u8) -> u32 {
    (base as f64 * percent_for_month(points, month) / 100.0)
        .round()
        .clamp(0.0, u32::MAX as f64) as u32
}

#[cfg(test)]
mod price_tests {
    use super::{price_for_month, starter_price_curve, Space};

    #[test]
    fn interpolates_annual_curve() {
        let points = starter_price_curve();
        assert_eq!(price_for_month(1000, &points, 1), 900);
        assert_eq!(price_for_month(1000, &points, 9), 1250);
        assert_eq!(price_for_month(1000, &points, 4), 1000);
        assert_eq!(price_for_month(1000, &points, 12), 980);
    }

    #[test]
    fn base_price_is_flat_without_seasonal_pricing() {
        let mut room = Space::new(1);
        room.current_monthly_rent = Some(1000);
        assert_eq!(room.price_for_calendar_month(2026, 1), Some(1000));
        assert_eq!(room.price_for_calendar_month(2026, 9), Some(1000));
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Space {
    pub id: u32,
    pub name: String,
    #[serde(default)]
    pub external_name: String,
    pub kind: String,
    #[serde(default)]
    pub parent_id: Option<u32>,
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
    pub monthly_price_overrides: Vec<MonthlyPriceOverride>,
}

impl Space {
    pub fn base_rent(&self) -> Option<u32> {
        self.current_monthly_rent
            .or(self.rent_jan_apr)
            .or(self.rent_sep_dec)
            .or(self.rent_may_aug)
    }

    pub fn effective_curve(&self) -> Vec<PricePoint> {
        effective_price_curve(
            &self.price_curve,
            self.base_rent(),
            self.rent_jan_apr,
            self.rent_may_aug,
            self.rent_sep_dec,
        )
    }

    pub fn price_for_calendar_month(&self, year: i32, month: u8) -> Option<u32> {
        self.monthly_price_overrides
            .iter()
            .find(|item| item.year == year && item.month == month)
            .map(|item| item.price)
            .or_else(|| {
                self.base_rent()
                    .map(|base| price_for_month(base, &self.effective_curve(), month))
            })
    }

    pub fn new(id: u32) -> Self {
        Self {
            id,
            name: String::new(),
            external_name: String::new(),
            kind: "Bedroom".into(),
            parent_id: None,
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
            monthly_price_overrides: Vec::new(),
        }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Property {
    pub id: u32,
    pub name: String,
    #[serde(default)]
    pub external_name: String,
    pub address: String,
    #[serde(default)]
    pub owner_id: Option<i64>,
    #[serde(default)]
    pub owner_name: String,
    #[serde(default)]
    pub owner_email: String,
    #[serde(default = "default_owner_manages_tours")]
    pub owner_manages_tours: bool,
    #[serde(default)]
    pub manager_staff_ids: Vec<i64>,
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

fn default_owner_manages_tours() -> bool {
    true
}

impl Property {
    pub fn ensure_external_names(&mut self) -> bool {
        let mut changed = false;
        let city = self
            .address
            .split(',')
            .nth(1)
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .unwrap_or("Waterloo");
        let descriptors = [
            "Inviting",
            "Welcoming",
            "Charming",
            "Comfortable",
            "Relaxed",
            "Cozy",
        ];
        if self.external_name.trim().is_empty() {
            let bedrooms = self
                .spaces
                .iter()
                .filter(|space| space.kind == "Bedroom")
                .count();
            let home = if bedrooms == 0 {
                "home".to_string()
            } else {
                format!("{bedrooms}-bedroom home")
            };
            self.external_name = format!(
                "{} {} in {city}",
                descriptors[self.id as usize % descriptors.len()],
                home
            );
            changed = true;
        }
        for space in self
            .spaces
            .iter_mut()
            .filter(|space| space.kind == "Bedroom" || space.kind == "Suite")
        {
            if space.external_name.trim().is_empty() {
                let descriptor =
                    descriptors[(self.id as usize + space.id as usize) % descriptors.len()];
                let internal = space.name.to_ascii_lowercase();
                let unit = if space.kind == "Suite" {
                    if internal.contains("basement") {
                        "private basement suite"
                    } else {
                        "private suite"
                    }
                } else if internal.contains("basement") {
                    "basement room"
                } else if internal.contains("master") || internal.contains("primary") {
                    "primary room"
                } else {
                    "private room"
                };
                let bath = if space.bathroom_access == "Ensuite" {
                    " with ensuite"
                } else {
                    ""
                };
                space.external_name = format!("{descriptor} {unit}{bath} in {city}");
                changed = true;
            }
        }
        changed
    }

    pub fn location_label(&self) -> String {
        let parts: Vec<&str> = self
            .address
            .split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .collect();
        if parts.len() > 1 {
            parts[1..].join(", ")
        } else {
            "Waterloo, ON".into()
        }
    }

    pub fn new(id: u32) -> Self {
        Self {
            id,
            name: String::new(),
            external_name: String::new(),
            address: String::new(),
            owner_id: None,
            owner_name: String::new(),
            owner_email: String::new(),
            owner_manages_tours: true,
            manager_staff_ids: Vec::new(),
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
                    external_name: String::new(),
                    kind: kind.into(),
                    parent_id: None,
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
                    monthly_price_overrides: Vec::new(),
                });
                next_id += 1;
            }
        }
    }
}

#[cfg(test)]
mod external_name_tests {
    use super::{Property, Space};

    #[test]
    fn generates_guest_names_without_street_addresses_and_preserves_edits() {
        let mut property = Property::new(4);
        property.name = "315 Dearborn Blvd".into();
        property.address = "315 Dearborn Blvd, Waterloo, ON".into();
        let mut room = Space::new(1);
        room.name = "Bedroom 1".into();
        room.bathroom_access = "Ensuite".into();
        property.spaces.push(room);
        assert!(property.ensure_external_names());
        assert!(property
            .external_name
            .contains("1-bedroom home in Waterloo"));
        assert!(property.spaces[0]
            .external_name
            .contains("private room with ensuite in Waterloo"));
        assert!(!property.external_name.contains("Dearborn"));
        property.external_name = "My custom home".into();
        assert!(!property.ensure_external_names());
        assert_eq!(property.external_name, "My custom home");
    }
}
