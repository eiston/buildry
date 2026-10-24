use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct HouseworkTicket {
    pub id: i64,
    pub creator_id: Option<i64>,
    pub title: String,
    pub description: String,
    pub property_id: Option<i64>,
    pub property_name: String,
    pub assignee: String,
    pub status: String,
    pub start_date: Option<String>,
    pub start_time: Option<String>,
    pub estimate_minutes: Option<i32>,
    #[serde(default)]
    pub photos: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub automation_run_id: Option<i64>,
    #[serde(default)]
    pub tenant_request: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct HouseworkInput {
    pub title: String,
    pub description: String,
    pub property_id: Option<i64>,
    pub assignee: String,
    pub status: String,
    pub start_date: Option<String>,
    pub start_time: Option<String>,
    pub estimate_minutes: Option<i32>,
    #[serde(default)]
    pub photos: Vec<String>,
}
