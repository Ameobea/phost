use chrono::NaiveDateTime;
use diesel::prelude::*;
use uuid::Uuid;

#[derive(Serialize, Deserialize, Queryable, Clone, Debug)]
pub struct ProxyDeployment {
    pub id: Uuid,
    pub name: String,
    pub subdomain: String,
    pub use_cors_headers: bool,
    pub destination_address: String,
    pub created_on: NaiveDateTime,
}
