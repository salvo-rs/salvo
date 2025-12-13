use serde::{Serialize, Deserialize};
pub mod posts;
pub mod users;

#[derive(Debug, Serialize, Deserialize)]
pub struct JwtClaims {
    pub username: String,
    pub exp: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TokenResponseModel{
    pub token_type: String,
    pub token: String
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorResponseModel {
    pub detail: String
}