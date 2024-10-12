use axum::{http::StatusCode, Json};
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiError {
    MissingFile,
    MissingFileOrHash,

    InvalidFile,
    InvalidHash,
    InvalidImage,

    NotFound,

    Sqlite {
        code: Option<isize>,
        message: Option<String>,
    },
}

impl From<sqlite::Error> for ApiError {
    fn from(value: sqlite::Error) -> Self {
        Self::Sqlite {
            code: value.code,
            message: value.message,
        }
    }
}

#[derive(Serialize)]
#[serde(untagged)]
pub enum ApiResponse<T, E = ApiError> {
    Ok(T),
    Err { error: E },
}

impl<T, E> ApiResponse<T, E> {
    pub fn ok(t: T) -> (StatusCode, Json<Self>) {
        (StatusCode::OK, Json(Self::Ok(t)))
    }

    pub fn err(error: E, status_code: StatusCode) -> (StatusCode, Json<Self>) {
        (status_code, Json(Self::Err { error }))
    }
}

#[derive(Serialize)]
pub struct SignatureResponse {
    pub avglf: (f64, f64, f64),
    pub sig: Vec<i16>,
}
