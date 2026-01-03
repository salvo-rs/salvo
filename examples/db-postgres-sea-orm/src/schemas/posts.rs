use salvo_oapi::ToSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, ToSchema)]
#[salvo(extract(default_source(from = "body")))]
pub struct PostCreate {
    pub title: String,
    pub content: String,
}
