use chrono::NaiveDateTime;
use diesel::prelude::*;
use salvo::macros::Extractible;
use salvo_oapi::ToSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::schema::*;

#[derive(Queryable, Serialize, Deserialize, Debug, Selectable)]
#[diesel(table_name = users)]
#[diesel(check_for_backend(diesel::pg::Pg))]
#[diesel(sql_type = Timestamp)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub password: String,
    pub full_name: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

// Insertable: represents new data to insert (no id)
#[derive(Insertable, Serialize, Deserialize, Debug)]
#[diesel(table_name = users)]
pub struct NewUser {
    pub id: Uuid,
    pub username: String,
    pub password: String,
    pub full_name: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

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
pub struct ResUserBody {
    pub id: Uuid,
    pub email: String,
    pub full_name: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}
