use diesel::{prelude::*};
use serde::{Serialize, Deserialize};
use crate::models::schema::users;
use uuid::Uuid;
use chrono::NaiveDateTime;

#[derive(Queryable, Serialize, Deserialize, Debug, Selectable)]
#[diesel(table_name = users)]
#[diesel(check_for_backend(diesel::pg::Pg))]
#[diesel(sql_type = Timestamp)]
pub struct Users {
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
    pub updated_at: NaiveDateTime
}