use serde::Serialize;

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


