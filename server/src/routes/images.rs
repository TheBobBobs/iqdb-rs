use std::sync::Arc;

use axum::{
    extract::{Multipart, Path},
    http::StatusCode,
    Extension, Json,
};
use iqdb_rs::{ImageData, DB};
use serde::Serialize;
use tokio::sync::{Mutex, RwLock};

use crate::{response::SignatureResponse, utils::get_signature, ApiError, ApiResponse};

#[derive(Serialize)]
pub struct PostImageResponse {
    pub id: u32,
    pub post_id: u32,
    pub hash: String,
    pub signature: SignatureResponse,
}

#[derive(Serialize)]
pub struct DeleteImageResponse {
    pub post_id: u32,
    pub ids: Vec<u32>,
}

pub async fn post(
    Extension(sql_db): Extension<Arc<Mutex<sqlite::Connection>>>,
    Extension(db): Extension<Arc<RwLock<DB>>>,
    Path(post_id): Path<u32>,
    form: Multipart,
) -> (StatusCode, Json<ApiResponse<PostImageResponse>>) {
    let sig = match get_signature(None, Some(form)).await {
        Ok(sig) => sig,
        Err(mut error) => {
            if matches!(error, ApiError::MissingFileOrHash) {
                error = ApiError::MissingFile;
            }
            return ApiResponse::err(error, StatusCode::BAD_REQUEST);
        }
    };
    let sig_bytes: Vec<u8> = sig.sig.iter().flat_map(|i| i.to_le_bytes()).collect();

    let mut db = db.write().await;

    let id = db.last_id().map(|i| i + 1).unwrap_or(0);

    {
        let sql_db = sql_db.lock().await;
        let query = "
        INSERT INTO images (id, post_id, avglf1, avglf2, avglf3, sig)
        VALUES (:id, :post_id, :avglf1, :avglf2, :avglf3, :sig)";
        let mut statement = sql_db.prepare(query).unwrap();
        statement
            .bind::<&[(_, sqlite::Value)]>(
                &[
                    (":id", (id as i64).into()),
                    (":post_id", (post_id as i64).into()),
                    (":avglf1", sig.avgl.0.into()),
                    (":avglf2", sig.avgl.1.into()),
                    (":avglf3", sig.avgl.2.into()),
                    (":sig", sig_bytes.into()),
                ][..],
            )
            .unwrap();
        match statement.next() {
            Ok(sqlite::State::Done) => {}
            Ok(_) => unreachable!(),
            Err(error) => {
                let error = ApiError::Sqlite {
                    code: error.code,
                    message: error.message,
                };
                return ApiResponse::err(error, StatusCode::INTERNAL_SERVER_ERROR);
            }
        };
    }

    db.insert(ImageData {
        id,
        post_id,
        avgl: sig.avgl,
        sig: sig.sig.clone(),
    });

    let response = PostImageResponse {
        post_id,
        id,
        hash: sig.to_string(),
        signature: SignatureResponse {
            avglf: sig.avgl,
            sig: sig.sig,
        },
    };
    ApiResponse::ok(response)
}

pub async fn delete(
    Extension(sql_db): Extension<Arc<Mutex<sqlite::Connection>>>,
    Extension(db): Extension<Arc<RwLock<DB>>>,
    Path(post_id): Path<u32>,
) -> (StatusCode, Json<ApiResponse<DeleteImageResponse>>) {
    let mut db = db.write().await;

    let images: Vec<_> = {
        let sql_db = sql_db.lock().await;
        let query = "DELETE FROM images WHERE post_id = ? RETURNING *";
        let mut statement = sql_db.prepare(query).unwrap();
        statement.bind((1, post_id as i64)).unwrap();
        let mut data = Vec::new();
        for result in statement.into_iter() {
            let row = match result {
                Ok(row) => row,
                Err(error) => {
                    let error = ApiError::Sqlite {
                        code: error.code,
                        message: error.message,
                    };
                    return ApiResponse::err(error, StatusCode::INTERNAL_SERVER_ERROR);
                }
            };
            let values: Vec<sqlite::Value> = row.into();
            let image = ImageData::try_from(values).unwrap();
            data.push(image);
        }
        data
    };
    let ids = images.iter().map(|i| i.id).collect();

    for image in images {
        db.delete(image.id, image);
    }

    let response = DeleteImageResponse { post_id, ids };
    ApiResponse::ok(response)
}
