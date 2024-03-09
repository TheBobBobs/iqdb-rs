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
    pub id: i64,
    pub hash: String,
    pub signature: SignatureResponse,
}

#[derive(Serialize)]
pub struct DeleteImageResponse {
    pub id: i64,
}

pub async fn post(
    Extension(sql_db): Extension<Arc<Mutex<sqlite::Connection>>>,
    Extension(db): Extension<Arc<RwLock<DB>>>,
    Path(id): Path<i64>,
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

    {
        let sql_db = sql_db.lock().await;
        let query = "
        INSERT INTO images (id, avglf1, avglf2, avglf3, sig)
        VALUES (:id, :avglf1, :avglf2, :avglf3, :sig)";
        let mut statement = sql_db.prepare(query).unwrap();
        statement
            .bind::<&[(_, sqlite::Value)]>(
                &[
                    (":id", id.into()),
                    (":avglf1", sig.avgl.0.into()),
                    (":avglf2", sig.avgl.1.into()),
                    (":avglf3", sig.avgl.2.into()),
                    (":sig", sig_bytes.into()),
                ][..],
            )
            .unwrap();
        if let Some(Err(error)) = statement.into_iter().next() {
            let error = ApiError::Sqlite {
                code: error.code,
                message: error.message,
            };
            return ApiResponse::err(error, StatusCode::INTERNAL_SERVER_ERROR);
        };
    }

    db.insert(ImageData {
        id,
        avgl: sig.avgl,
        sig: sig.sig.clone(),
    });

    let response = PostImageResponse {
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
    Path(id): Path<i64>,
) -> (StatusCode, Json<ApiResponse<DeleteImageResponse>>) {
    let mut db = db.write().await;

    let image = {
        let sql_db = sql_db.lock().await;
        let query = "DELETE FROM images WHERE id = ? RETURNING *";
        let mut statement = sql_db.prepare(query).unwrap();
        statement.bind((1, id)).unwrap();
        let Some(result) = statement.into_iter().next() else {
            return ApiResponse::err(ApiError::NotFound, StatusCode::NOT_FOUND);
        };
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
        ImageData::try_from(values).unwrap()
    };

    db.delete(image);

    let response = DeleteImageResponse { id };
    ApiResponse::ok(response)
}
