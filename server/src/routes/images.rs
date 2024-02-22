use std::sync::Arc;

use axum::{
    extract::{Multipart, Path},
    Extension, Json,
};
use iqdb_rs::{ImageData, DB};
use serde::Serialize;
use tokio::sync::{Mutex, RwLock};

use crate::{utils::get_signature, ApiError, ApiResponse};

#[derive(Serialize)]
pub struct PostImageResponse {
    pub id: u32,
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
) -> Json<ApiResponse<PostImageResponse>> {
    let sig = match get_signature(None, Some(form)).await {
        Ok(sig) => sig,
        Err(mut error) => {
            if matches!(error, ApiError::MissingFileOrHash) {
                error = ApiError::MissingFile;
            }
            return Json(ApiResponse::Err { error });
        }
    };
    let sig_bytes: Vec<u8> = sig.sig.iter().flat_map(|i| i.to_le_bytes()).collect();

    let id = {
        let sql_db = sql_db.lock().await;
        let query = "
        INSERT INTO images (post_id, avglf1, avglf2, avglf3, sig)
        VALUES (:post_id, :avglf1, :avglf2, :avglf3, :sig)
        RETURNING id";
        let mut statement = sql_db.prepare(query).unwrap();
        statement
            .bind::<&[(_, sqlite::Value)]>(
                &[
                    (":post_id", (post_id as i64).into()),
                    (":avglf1", sig.avgl.0.into()),
                    (":avglf2", sig.avgl.1.into()),
                    (":avglf3", sig.avgl.2.into()),
                    (":sig", sig_bytes.into()),
                ][..],
            )
            .unwrap();
        let row = match statement.into_iter().next().unwrap() {
            Ok(row) => row,
            Err(error) => {
                let error = ApiError::Sqlite {
                    code: error.code,
                    message: error.message,
                };
                return Json(ApiResponse::Err { error });
            }
        };
        row.read::<i64, _>(0) as u32
    };

    {
        let mut db = db.write().await;
        db.insert(ImageData {
            id,
            post_id,
            avgl: sig.avgl,
            sig: sig.sig,
        });
    }

    let response = PostImageResponse { id };
    Json(ApiResponse::Ok(response))
}

pub async fn delete(
    Extension(sql_db): Extension<Arc<Mutex<sqlite::Connection>>>,
    Extension(db): Extension<Arc<RwLock<DB>>>,
    Path(post_id): Path<u32>,
) -> Json<ApiResponse<DeleteImageResponse>> {
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
                    return Json(ApiResponse::Err { error });
                }
            };
            let values: Vec<sqlite::Value> = row.into();
            let image = ImageData::try_from(values).unwrap();
            data.push(image);
        }
        data
    };

    let ids = images.iter().map(|i| i.id).collect();

    let mut db = db.write().await;
    for image in images {
        db.delete(image.id, image);
    }
    drop(db);

    let response = DeleteImageResponse { post_id, ids };
    Json(ApiResponse::Ok(response))
}
