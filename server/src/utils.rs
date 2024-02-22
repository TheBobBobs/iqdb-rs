use axum::extract::Multipart;
use iqdb_rs::Signature;

use crate::ApiError;

pub async fn get_signature(
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
