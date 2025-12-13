use chrono::NaiveDateTime;
use salvo::macros::Extractible;
use salvo_oapi::ToSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Serialize, Deserialize, Extractible, Debug, ToSchema)]
#[salvo(extract(default_source(from = "body")))]
pub struct UserCredentiel {
    pub username: String,
    pub password: String,
}

#[derive(Serialize, Deserialize, Extractible, Debug, ToSchema)]
#[salvo(extract(default_source(from = "body")))]
pub struct UserCreate {
    pub email: String,
    pub fullname: String,
    pub password: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct UserResponseModel {
    pub id: Uuid,
    pub email: String,
    pub full_name: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Serialize, Deserialize, Extractible, Debug)]
pub struct Token<'a> {
    pub access_token: &'a str,
    pub token_type: &'a str,
}

#[derive(Serialize, Deserialize, Extractible, Debug, ToSchema)]
#[salvo(extract(default_source(from = "body")))]
pub struct UserUpdate {
    pub fullname: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UserSuccessResponseModel{
    pub id: Uuid,
    pub email: String,
    pub full_name: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,

}
