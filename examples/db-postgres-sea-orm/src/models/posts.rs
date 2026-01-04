use chrono::NaiveDateTime;
use sea_orm::prelude::*;
use serde::{Serialize, Deserialize};
use uuid::Uuid;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "posts")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub title: String,
    pub content: String,
    #[sea_orm(unique_key = "users")]
    pub user_id: Uuid,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    #[sea_orm(belongs_to, from = "user_id", to = "id")]
    pub user: Option<super::users::Entity>,

}
impl ActiveModelBehavior for ActiveModel {}