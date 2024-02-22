use std::{collections::HashMap, sync::Arc};

use axum::{
    extract::{Multipart, Path, Query},
    routing::{get, post},
    Extension, Json, Router,
};
use rustiq_db::{ImageData, Signature, DB};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock};

#[tokio::main]
async fn main() {
    let sql_db = sqlite::open("iqdb.sqlite").unwrap();
    let db = {
        let query = "SELECT * FROM images";
        let parsed = sql_db.prepare(query).unwrap().into_iter().map(|row| {
            let values: Vec<sqlite::Value> = row.unwrap().into();
            ImageData::try_from(values).unwrap()
        });
        DB::new(parsed)
    };
    let db = Arc::new(RwLock::new(db));
    // TODO rwlock?
    let sql_db = Arc::new(Mutex::new(sql_db));

    let app = Router::new()
        .route("/query", get(query).post(query))
        .route("/images/:post_id", post(post_image).delete(delete_image))
        .route("/status", get(get_status))
        .layer(Extension(db))
        .layer(Extension(sql_db));
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3002").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

const fn query_default_limit() -> usize {
    20
}

#[derive(Deserialize)]
pub struct GetQuery {
    #[serde(alias = "l", default = "query_default_limit")]
    limit: usize,
    #[serde(alias = "h")]
    hash: Option<String>,
}

#[derive(Serialize)]
pub struct GetQueryResponse {
    posts: Vec<GetQueryResponsePost>,
}

#[derive(Serialize)]
pub struct GetQueryResponsePost {
    post_id: u32,
    score: f32,
    hash: String,
    signature: GetQueryResponseSig,
}

#[derive(Serialize)]
pub struct GetQueryResponseSig {
    avglf: (f64, f64, f64),
    sig: Vec<i16>,
}

async fn query(
    Extension(sql_db): Extension<Arc<Mutex<sqlite::Connection>>>,
    Extension(db): Extension<Arc<RwLock<DB>>>,
    Query(GetQuery { limit, hash }): Query<GetQuery>,
    form: Option<Multipart>,
) -> Json<GetQueryResponse> {
    let looking_for = if let Some(hash) = hash {
        hash.parse().unwrap()
    } else if let Some(mut form) = form {
        let Ok(Some(field)) = form.next_field().await else {
            panic!()
        };
        if field.name() != Some("file") {
            panic!();
        }
        let Ok(bytes) = field.bytes().await else {
            panic!();
        };
        let img = image::load_from_memory(&bytes).unwrap();
        Signature::from_image(&img)
    } else {
        panic!();
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
                signature: GetQueryResponseSig {
                    avglf: sig.avgl,
                    sig: sig.sig,
                },
            }
        })
        .collect();
    posts.sort_by(|a, b| a.score.partial_cmp(&b.score).unwrap().reverse());

    Json(GetQueryResponse { posts })
}

#[derive(Serialize)]
pub struct PostImageResponse {
    id: u32,
}

async fn post_image(
    Extension(sql_db): Extension<Arc<Mutex<sqlite::Connection>>>,
    Extension(db): Extension<Arc<RwLock<DB>>>,
    Path(post_id): Path<u32>,
    mut form: Multipart,
) -> Json<PostImageResponse> {
    let Ok(Some(field)) = form.next_field().await else {
        panic!()
    };
    if field.name() != Some("file") {
        panic!();
    }
    let Ok(bytes) = field.bytes().await else {
        panic!();
    };
    let img = image::load_from_memory(&bytes).unwrap();
    let sig = Signature::from_image(&img);
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
        let row = statement.into_iter().next().unwrap().unwrap();
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

    Json(PostImageResponse { id })
}

#[derive(Serialize)]
pub struct DeleteImageResponse {
    post_id: u32,
}

async fn delete_image(
    Extension(sql_db): Extension<Arc<Mutex<sqlite::Connection>>>,
    Extension(db): Extension<Arc<RwLock<DB>>>,
    Path(post_id): Path<u32>,
) -> Json<DeleteImageResponse> {
    let images: Vec<_> = {
        let sql_db = sql_db.lock().await;
        let query = "DELETE FROM images WHERE post_id = ?";
        let mut statement = sql_db.prepare(query).unwrap();
        statement.bind((1, post_id as i64)).unwrap();
        let data = statement.into_iter().map(|row| {
            let values: Vec<sqlite::Value> = row.unwrap().into();
            ImageData::try_from(values).unwrap()
        });
        data.collect()
    };

    let mut db = db.write().await;
    for image in images {
        db.delete(image.id, image);
    }
    drop(db);

    Json(DeleteImageResponse { post_id })
}

#[derive(Serialize)]
pub struct GetStatusResponse {
    images: u32,
}

async fn get_status(Extension(db): Extension<Arc<RwLock<DB>>>) -> Json<GetStatusResponse> {
    let images = {
        let db = db.read().await;
        db.image_count() as u32
    };

    Json(GetStatusResponse { images })
}
