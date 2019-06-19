use chrono::NaiveDateTime;

#[derive(Serialize, Deserialize, Queryable, Clone, Debug)]
pub struct ProxyDeployment {
    pub id: String,
    pub name: String,
    pub subdomain: String,
    pub use_cors_headers: bool,
    pub destination_address: String,
    pub created_on: NaiveDateTime,
}
