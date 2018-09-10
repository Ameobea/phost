use super::schema::static_deployments;

#[derive(Queryable)]
pub struct StaticDeployment {
    pub id: i32,
    pub name: String,
    pub subdomain: String,
}

#[derive(Insertable)]
#[table_name = "static_deployments"]
pub struct NewStaticDeployment {
    pub name: String,
    pub subdomain: String,
}
