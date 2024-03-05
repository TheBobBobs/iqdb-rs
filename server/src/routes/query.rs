use std::{collections::HashMap, sync::Arc};

use axum::{
    extract::{Multipart, Query},
    http::StatusCode,
    Extension, Json,
};
use iqdb_rs::{ImageData, Signature, DB};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock};

use crate::{response::SignatureResponse, utils::get_signature, ApiResponse};

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

pub type GetQueryResponse = Vec<GetQueryResponsePost>;

#[derive(Serialize)]
pub struct GetQueryResponsePost {
    pub post_id: u32,
    pub score: f32,
    pub hash: String,
    pub signature: SignatureResponse,
}

pub async fn get(
    Extension(sql_db): Extension<Arc<Mutex<sqlite::Connection>>>,
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
        let ids: Vec<String> = result.iter().map(|(_, i)| i.to_string()).collect();
        let query = format!("SELECT * FROM images WHERE id IN ({})", ids.join(", "));
        sql_db
            .prepare(query)
            .unwrap()
            .into_iter()
            .map(|row| {
                let values: Vec<sqlite::Value> = row.unwrap().into();
                ImageData::try_from(values).unwrap()
            })
            .collect()
    };

    let scores: HashMap<_, _> = result
        .iter()
        .copied()
        .map(|(score, id)| (id, score))
        .collect();

    let mut posts: Vec<GetQueryResponsePost> = images
        .into_iter()
        .map(|data| {
            let sig = Signature {
                avgl: data.avgl,
                sig: data.sig,
            };
            GetQueryResponsePost {
                post_id: data.post_id,
                score: *scores.get(&data.id).unwrap(),
                hash: sig.to_string(),
                signature: SignatureResponse {
                    avglf: sig.avgl,
                    sig: sig.sig,
                },
            }
        })
        .collect();
    posts.sort_by(|a, b| a.score.partial_cmp(&b.score).unwrap().reverse());

    let response = posts;
    ApiResponse::ok(response)
}
