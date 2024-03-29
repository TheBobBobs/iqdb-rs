use std::sync::Arc;

use axum::{http::StatusCode, Extension, Json};
use iqdb_rs::DB;
use serde::Serialize;
use tokio::sync::RwLock;

use crate::ApiResponse;

#[derive(Serialize)]
pub struct GetStatusResponse {
    pub images: u32,
}

pub async fn get(
    Extension(db): Extension<Arc<RwLock<DB>>>,
) -> (StatusCode, Json<ApiResponse<GetStatusResponse>>) {
    let images = {
        let db = db.read().await;
        db.image_count() as u32
    };

    let response = GetStatusResponse { images };
    ApiResponse::ok(response)
}
