use actix::prelude::{Actor, Handler, Message, SyncContext};
use actix_web::error::{Error as ActixError, ErrorInternalServerError};
use diesel;
use diesel::prelude::{ExpressionMethods, MysqlConnection, QueryDsl, RunQueryDsl};
use diesel::r2d2::{ConnectionManager, Pool};

use models::{self, internal::DeploymentDescriptor, NewStaticDeployment};
use schema;
use util::get_deployment_url;

pub struct DbExecutor(pub Pool<ConnectionManager<MysqlConnection>>);

pub struct CreateStaticDeployment(pub NewStaticDeployment);

impl Message for CreateStaticDeployment {
    type Result = Result<models::StaticDeployment, ActixError>;
}

impl Actor for DbExecutor {
    type Context = SyncContext<Self>;
}

impl Handler<CreateStaticDeployment> for DbExecutor {
    type Result = Result<models::StaticDeployment, ActixError>;

    fn handle(&mut self, msg: CreateStaticDeployment, _: &mut Self::Context) -> Self::Result {
        use self::schema::static_deployments::dsl::{static_deployments, subdomain};

        let new_user = models::NewStaticDeployment {
            name: msg.0.name,
            subdomain: msg.0.subdomain.clone(),
        };

        let conn: &MysqlConnection = &self.0.get().unwrap();

        diesel::insert_into(static_deployments)
            .values(&new_user)
            .execute(conn)
            .map_err(|_| ErrorInternalServerError("Error inserting person"))?;

        let mut items = static_deployments
            .filter(subdomain.eq(&msg.0.subdomain))
            .load::<models::StaticDeployment>(conn)
            .map_err(|_| ErrorInternalServerError("Error loading person"))?;

        Ok(items.pop().unwrap())
    }
}

pub struct ListAllStaticDeployments;

impl Message for ListAllStaticDeployments {
    type Result = Result<Vec<models::internal::DeploymentDescriptor>, ActixError>;
}

impl Handler<ListAllStaticDeployments> for DbExecutor {
    type Result = Result<Vec<models::internal::DeploymentDescriptor>, ActixError>;

    fn handle(&mut self, _: ListAllStaticDeployments, _: &mut Self::Context) -> Self::Result {
        use self::schema::static_deployments::dsl::static_deployments;

        let conn: &MysqlConnection = &self.0.get().unwrap();

        static_deployments
            .load::<models::StaticDeployment>(conn)
            .map(|deployments| {
                deployments
                    .into_iter()
                    .map(|deployment| DeploymentDescriptor {
                        name: deployment.name,
                        url: get_deployment_url(&deployment.subdomain),
                    }).collect()
            }).map_err(|_| ErrorInternalServerError("Error retrieving "))
    }
}
