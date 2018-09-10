use std::hint::unreachable_unchecked;
use std::io::Write;

use actix_web::{
    dev, error as actix_error,
    error::ErrorBadRequest,
    http::header::DispositionParam,
    multipart::{self, MultipartItem},
    Error as ActixWebError, HttpMessage, HttpRequest, Json, State,
};
use futures::{future, Future, Stream};

use super::AppState;
use db::{CreateStaticDeployment, ListAllStaticDeployments};
use models::{internal::DeploymentDescriptor, NewStaticDeployment};
use upload::process_file_payload;

pub fn index(_: State<AppState>) -> &'static str {
    "Project hoster up and running!  Try `GET /deployments`."
}

pub fn list_deployments(
    state: State<AppState>,
) -> impl Future<Item = Json<Vec<DeploymentDescriptor>>, Error = ActixWebError> {
    state
        .db
        .send(ListAllStaticDeployments)
        .flatten()
        .map(Json)
        .map_err(actix_error::ErrorInternalServerError)
}

#[derive(Default)]
struct IncompleteNewDeployment {
    name: Option<String>,
    subdomain: Option<String>,
    file: Option<multipart::Field<dev::Payload>>,
}

enum ParsedMultipartItem {
    Tag(String, String),
    File(multipart::Field<dev::Payload>),
}

// TODO: Look into switching away from the `Box` once we implement the `unimplemented!()`s; it's
// causing a recursion error atm.
fn handle_static_deployment_multipart_item(
    item: multipart::MultipartItem<dev::Payload>,
) -> Result<Box<Stream<Item = ParsedMultipartItem, Error = ActixWebError>>, ActixWebError> {
    match item {
        MultipartItem::Field(field) => match field.content_disposition() {
            Some(disposition) => {
                let field_name = disposition
                    .parameters
                    .into_iter()
                    .find(|param| match param {
                        DispositionParam::Name(_) => true,
                        _ => {
                            warn!("Unhandled content disposition param: {:?}", param);
                            false
                        }
                    }).map(|opt| match opt {
                        DispositionParam::Name(name) => name,
                        _ => unsafe { unreachable_unchecked() },
                    }).ok_or(ErrorBadRequest(
                        "Invalid request - no `name` header sent with multipart data!",
                    ))?;
                println!("field name: {}", field_name);

                if field_name == "file" {
                    Ok(box future::ok(ParsedMultipartItem::File(field)).into_stream())
                } else {
                    Ok(box field
                        .fold(Vec::new(), move |mut acc, bytes| {
                            let fut = acc.write_all(bytes.as_ref()).map(|_| acc).map_err(|err| {
                                error!("Failed to write header to buffer");
                                actix_error::MultipartError::Payload(actix_error::PayloadError::Io(
                                    err,
                                ))
                            });
                            future::result(fut)
                        }).and_then(|buf| {
                            String::from_utf8(buf).map_err(|err| {
                                actix_error::MultipartError::Parse(actix_error::ParseError::Utf8(
                                    err.utf8_error(),
                                ))
                            })
                        }).from_err()
                        .map(move |val| ParsedMultipartItem::Tag(field_name.clone(), val))
                        .into_stream())
                }
            }
            None => Err(ErrorBadRequest(
                "Invalid request - no content disposition info sent with multipart data!",
            )),
        },
        MultipartItem::Nested(mp) => Ok(box mp
            .map_err(actix_error::ErrorInternalServerError)
            .and_then(handle_static_deployment_multipart_item)
            .flatten()),
    }
}

pub fn create_static_deployment(
    req: HttpRequest<AppState>,
) -> impl Future<Item = Json<DeploymentDescriptor>, Error = ActixWebError> {
    req.multipart()
        .map_err(actix_error::ErrorInternalServerError)
        .and_then(handle_static_deployment_multipart_item)
        .flatten()
        .fold(
            IncompleteNewDeployment::default(),
            |mut acc, field| -> Result<IncompleteNewDeployment, ActixWebError> {
                println!("Parsing parsed field...");
                match field {
                    ParsedMultipartItem::Tag(key, val) => match key.as_str() {
                        "name" => acc.name = Some(val),
                        "subdomain" => acc.subdomain = Some(val),
                        _ => warn!(
                        "Invallid multipart field name supplied to `create_static_deployment`: {}",
                        key
                    ),
                    },
                    ParsedMultipartItem::File(stream) => acc.file = Some(stream),
                };

                Ok(acc)
            },
        ).map_err(|err| {
            println!("Static deployment creation failed: {:?}", err);
            err
        }).and_then(|deployment| {
            println!("Uhh mapping parsed keys",);
            let missing_multipart_key = || {
                actix_error::ErrorBadRequest("Invalid request; missing key in multipart payload")
            };

            let name = deployment.name.ok_or(missing_multipart_key())?.clone();
            let subdomain = deployment.subdomain.ok_or(missing_multipart_key())?;
            let file = deployment.file.ok_or(missing_multipart_key())?;

            Ok((file, NewStaticDeployment { name, subdomain }))
        }).and_then(|(file, NewStaticDeployment { name, subdomain })| {
            // Write the provided archive file to disk and extract it into the hosting directory at
            // the appropriate point in the fs tree
            println!("Processing file upload");
            process_file_payload(name, subdomain, file)
        }).and_then(move |deployment_descriptor| {
            req.state()
                .db
                .send(CreateStaticDeployment(deployment_descriptor))
                .map_err(actix_error::ErrorInternalServerError)
                .and_then(|res| res)
        }).map(|deployment| Json(deployment.into()))
}
