use std::sync::Arc;

use axum::{
    extract::{Multipart, Path},
    http::StatusCode,
    Extension, Json,
};
use iqdb_rs::{ImageData, DB};
use serde::Serialize;
use tokio::sync::{Mutex, RwLock};

use crate::{response::SignatureResponse, utils::get_signature, ApiError, ApiResponse, SqlDB};

#[derive(Serialize)]
pub struct PostImageResponse {
    #[serde(rename = "post_id")]
    pub id: i64,
    pub hash: String,
    pub signature: SignatureResponse,
}

#[derive(Serialize)]
pub struct DeleteImageResponse {
    #[serde(rename = "post_id")]
    pub id: i64,
}

pub async fn post(
    Extension(sql_db): Extension<Arc<Mutex<SqlDB>>>,
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

    let mut db = db.write().await;

    if db.contains(id) {
        let mut sql_db = sql_db.lock().await;
        match sql_db.delete(id) {
            Ok(Some(image)) => db.delete(image),
            Ok(None) => unreachable!(),
            Err(e) => return ApiResponse::err(e.into(), StatusCode::INTERNAL_SERVER_ERROR),
        };
    }

    {
        let sql_db = sql_db.lock().await;
        if let Err(error) = sql_db.insert(id, &sig) {
            return ApiResponse::err(error.into(), StatusCode::INTERNAL_SERVER_ERROR);
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
    Extension(sql_db): Extension<Arc<Mutex<SqlDB>>>,
    Extension(db): Extension<Arc<RwLock<DB>>>,
    Path(id): Path<i64>,
) -> (StatusCode, Json<ApiResponse<DeleteImageResponse>>) {
    let mut db = db.write().await;

    let image = {
        let mut sql_db = sql_db.lock().await;
        match sql_db.delete(id) {
            Ok(Some(image)) => image,
            Ok(None) => return ApiResponse::err(ApiError::NotFound, StatusCode::NOT_FOUND),
            Err(e) => return ApiResponse::err(e.into(), StatusCode::INTERNAL_SERVER_ERROR),
        }
    };

    db.delete(image);

    let response = DeleteImageResponse { id };
    ApiResponse::ok(response)
}
