use super::StaticDeployment;
use util::get_deployment_url;

#[derive(Serialize)]
pub struct DeploymentDescriptor {
    pub name: String,
    pub url: String,
}

impl From<StaticDeployment> for DeploymentDescriptor {
    fn from(other: StaticDeployment) -> Self {
        DeploymentDescriptor {
            name: other.name,
            url: get_deployment_url(&other.subdomain),
        }
    }
}
