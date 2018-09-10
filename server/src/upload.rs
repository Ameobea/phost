//! Contains functions dealing with the upload process for deploying new projects

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use actix_web::{
    dev,
    error::{self as actix_error, Error as ActixWebError},
    multipart,
};
use futures::{future, Future, Stream};
use tempfile::tempdir;
use tokio_fs;
use tokio_process::CommandExt;
use uuid::Uuid;

use config::CONF;
use models::NewStaticDeployment;

fn setup_dir_structure(
    name: &str,
    version: &str,
) -> impl Future<Item = impl AsRef<Path>, Error = ActixWebError> {
    let mut target_path = PathBuf::new();
    target_path.push(&CONF.hosting_dir);
    target_path.push(name);
    target_path.push(version);

    tokio_fs::create_dir_all(target_path.clone())
        .map(|()| target_path)
        .map_err(|err| {
            error!("Error creating hosting directory: {:?}", err);
            actix_error::ErrorInternalServerError("Error creating hosting directory")
        })
}

/// Writes the supplied file (which is a .tgz archive) to a temporary file.  Then, it extracts
/// it to the host directory in the proper place so the application knows where to look for it.
pub fn process_file_payload(
    name: String,
    subdomain: String,
    field: multipart::Field<dev::Payload>,
) -> impl Future<Item = NewStaticDeployment, Error = ActixWebError> {
    let tmpfile_future = future::result(tempdir())
        .and_then(|temp_dir| {
            let tempdir_path = temp_dir.path();
            let mut tempfile_path = PathBuf::new();
            tempfile_path.push(tempdir_path);
            tempfile_path.push(Uuid::new_v4().to_string());
            tempfile_path.set_extension("tgz");

            tokio_fs::File::create(tempfile_path.clone()).map(|tmpfile| (tempfile_path, tmpfile))
        }).map_err(|e| -> ActixWebError {
            println!("process_file_payload failed, {:?}", e);
            actix_error::ErrorInternalServerError(e)
        }).and_then(|(tempfile_path, file)| {
            // Write the file from the request into the tempfile
            field
                .from_err()
                .fold(file, move |mut file, bytes| {
                    let rt = file.write_all(bytes.as_ref()).map(|_| file);
                    future::result(rt).map_err(|e| -> ActixWebError {
                        println!("file.write_all failed: {:?}", e);
                        actix_error::MultipartError::Payload(actix_error::PayloadError::Io(e))
                            .into()
                    })
                }).map(|_| tempfile_path)
        });

    let dir_structure_future = setup_dir_structure(&name, "0.1.0"); // TODO: get version from somewhere

    fn extract_error<T>(_: T) -> ActixWebError {
        actix_error::ErrorBadRequest("There was an issue extracting the supplied archive.")
    }

    tmpfile_future
        .join(dir_structure_future)
        .and_then(|(tempfile_path, host_dir)| {
            // Extract the tempfile into the host directory
            Command::new("tar")
                .arg("-xf")
                .arg(tempfile_path.as_os_str())
                .arg("-C")
                .arg(host_dir.as_ref().as_os_str())
                .status_async()
                .map_err(extract_error)
                .map(|exit_status| exit_status.map_err(extract_error))
        }).and_then(|exit_status| exit_status)
        .and_then(|exit_status| {
            if exit_status.success() {
                // TODO: Double check that the tempdir gets deallocated successfully
                Ok(NewStaticDeployment { name, subdomain })
            } else {
                Err(extract_error(()))
            }
        })
}
