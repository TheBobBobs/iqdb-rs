use std::{collections::HashMap, sync::Arc};

use axum::{
    extract::{Multipart, Query},
    http::StatusCode,
    Extension, Json,
};
use iqdb_rs::{Signature, DB};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock};

use crate::{response::SignatureResponse, utils::get_signature, ApiResponse, SqlDB};

const fn query_default_limit() -> usize {
    20
}

#[derive(Deserialize)]
pub struct GetQuery {
    #[serde(alias = "l", default = "query_default_limit")]
    pub limit: usize,
    #[serde(alias = "h")]
    pub hash: Option<String>,
}

pub type GetQueryResponse = Vec<GetQueryResponseImage>;

#[derive(Serialize)]
pub struct GetQueryResponseImage {
    #[serde(rename = "post_id")]
    pub id: i64,
    pub score: f32,
    pub hash: String,
    pub signature: SignatureResponse,
}

pub async fn get(
    Extension(sql_db): Extension<Arc<Mutex<SqlDB>>>,
    Extension(db): Extension<Arc<RwLock<DB>>>,
    Query(GetQuery { limit, hash }): Query<GetQuery>,
    form: Option<Multipart>,
) -> (StatusCode, Json<ApiResponse<GetQueryResponse>>) {
    let looking_for = match get_signature(hash, form).await {
        Ok(s) => s,
        Err(error) => return ApiResponse::err(error, StatusCode::BAD_REQUEST),
    };

    let result = {
        let db = db.read().await;
        db.query(&looking_for, limit)
    };

    let images: Vec<_> = {
        let sql_db = sql_db.lock().await;
        let ids = result.iter().map(|(_, i)| *i);
        sql_db.get_many(ids).collect()
    };

    let scores: HashMap<_, _> = result
        .iter()
        .copied()
        .map(|(score, id)| (id, score))
        .collect();

    let mut images: Vec<GetQueryResponseImage> = images
        .into_iter()
        .map(|data| {
            let sig = Signature {
                avgl: data.avgl,
                sig: data.sig,
            };
            GetQueryResponseImage {
                id: data.id,
                score: *scores.get(&data.id).unwrap(),
                hash: sig.to_string(),
                signature: SignatureResponse {
                    avglf: sig.avgl,
                    sig: sig.sig,
                },
            }
        })
        .collect();
    images.sort_by(|a, b| {
        a.score
            .total_cmp(&b.score)
            .then_with(|| a.id.cmp(&b.id))
            .reverse()
    });

    ApiResponse::ok(images)
}
