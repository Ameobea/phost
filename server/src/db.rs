use actix::prelude::*;
use actix_web::error::{Error as ActixError, ErrorInternalServerError};
use diesel;
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, Pool};

use models;
use schema;

pub struct DbExecutor(pub Pool<ConnectionManager<MysqlConnection>>);

pub struct CreateStaticDeployment {
    pub name: String,
    pub subdomain: String,
}

impl Message for CreateStaticDeployment {
    type Result = Result<models::StaticDeployment, ActixError>;
}

impl Actor for DbExecutor {
    type Context = SyncContext<Self>;
}

impl Handler<CreateStaticDeployment> for DbExecutor {
    type Result = Result<models::StaticDeployment, ActixError>;

    fn handle(&mut self, msg: CreateStaticDeployment, _: &mut Self::Context) -> Self::Result {
        use self::schema::static_deployments::dsl::*;

        let new_user = models::NewStaticDeployment {
            name: msg.name,
            subdomain: msg.subdomain.clone(),
        };

        let conn: &MysqlConnection = &self.0.get().unwrap();

        diesel::insert_into(static_deployments)
            .values(&new_user)
            .execute(conn)
            .map_err(|_| ErrorInternalServerError("Error inserting person"))?;

        let mut items = static_deployments
            .filter(subdomain.eq(&msg.subdomain))
            .load::<models::StaticDeployment>(conn)
            .map_err(|_| ErrorInternalServerError("Error loading person"))?;

        Ok(items.pop().unwrap())
    }
}
