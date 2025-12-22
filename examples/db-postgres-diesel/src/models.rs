pub mod posts;
pub mod users;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct JwtClaims {
    pub username: String,
    pub exp: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ResTokenBody {
    pub token_type: String,
    pub token: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ResErrorBody {
    pub detail: String,
}
