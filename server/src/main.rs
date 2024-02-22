use std::{collections::HashMap, sync::Arc};

use axum::{
    extract::{Multipart, Path, Query},
    routing::{get, post},
    Extension, Json, Router,
};
use clap::Parser;
use iqdb_rs::{ImageData, Signature, DB};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock};

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiError {
    MissingFile,
    MissingFileOrHash,

    InvalidFile,
    InvalidHash,
    InvalidImage,

    Sqlite {
        code: Option<isize>,
        message: Option<String>,
    },
}

#[derive(Serialize)]
#[serde(untagged)]
pub enum ApiResponse<T, E = ApiError> {
    Ok(T),
    Err { error: E },
}
#[derive(Parser)]
struct Args {
    /// The address to bind to
    #[arg(short = 'h', long = "host", default_value = "0.0.0.0")]
    host: String,
    /// The port to listen on
    #[arg(short = 'p', long = "port", default_value_t = 5588)]
    port: u16,
    /// The path to the sqlite db
    #[arg(short = 'd', long = "database", default_value = "iqdb.sqlite")]
    db_path: std::path::PathBuf,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let sql_db = sqlite::open(args.db_path).unwrap();
    let db = {
        let query = "SELECT * FROM images";
        let parsed = sql_db.prepare(query).unwrap().into_iter().map(|row| {
            let values: Vec<sqlite::Value> = row.unwrap().into();
            ImageData::try_from(values).unwrap()
        });
        DB::new(parsed)
    };
    let db = Arc::new(RwLock::new(db));
    let sql_db = Arc::new(Mutex::new(sql_db));

    let app = Router::new()
        .route("/query", get(query).post(query))
        .route("/images/:post_id", post(post_image).delete(delete_image))
        .route("/status", get(get_status))
        .layer(Extension(db))
        .layer(Extension(sql_db));
    let addr = format!("{}:{}", args.host, args.port);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn get_signature(
    hash: Option<String>,
    form: Option<Multipart>,
) -> Result<Signature, ApiError> {
    if let Some(hash) = hash {
        hash.parse().map_err(|_| ApiError::InvalidHash)
    } else if let Some(mut form) = form {
        let maybe_field = form.next_field().await.map_err(|_| ApiError::InvalidFile)?;
        let Some(field) = maybe_field else {
            return Err(ApiError::MissingFileOrHash);
        };
        if field.name() != Some("file") {
            return Err(ApiError::InvalidFile);
        }
        let bytes = field.bytes().await.map_err(|_| ApiError::InvalidFile)?;
        let img = image::load_from_memory(&bytes).map_err(|_| ApiError::InvalidImage)?;
        Ok(Signature::from_image(&img))
    } else {
        Err(ApiError::MissingFileOrHash)
    }
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
) -> Json<ApiResponse<GetQueryResponse>> {
    let looking_for = match get_signature(hash, form).await {
        Ok(s) => s,
        Err(error) => return Json(ApiResponse::Err { error }),
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

    let response = GetQueryResponse { posts };
    Json(ApiResponse::Ok(response))
}

#[derive(Serialize)]
pub struct PostImageResponse {
    id: u32,
}

async fn post_image(
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

#[derive(Serialize)]
pub struct DeleteImageResponse {
    post_id: u32,
    ids: Vec<u32>,
}

async fn delete_image(
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

#[derive(Serialize)]
pub struct GetStatusResponse {
    images: u32,
}

async fn get_status(
    Extension(db): Extension<Arc<RwLock<DB>>>,
) -> Json<ApiResponse<GetStatusResponse>> {
    let images = {
        let db = db.read().await;
        db.image_count() as u32
    };

    let response = GetStatusResponse { images };
    Json(ApiResponse::Ok(response))
}
