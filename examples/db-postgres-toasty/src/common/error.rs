use crate::common::result::{BaseResponse, PageResult};
use salvo::prelude::Json;
use salvo::{Depot, Request, Response, Writer, async_trait};
use thiserror::Error;
#[derive(Error, Debug)]
pub enum AppError {
    #[error("parse error: {0}")]
    ParseError(#[from] salvo::http::ParseError),

    #[error("db error: {0}")]
    DbError(#[from] toasty::Error),

    #[error("business error: {0}")]
    BusinessError(String),
}
pub type AppResult<T> = Result<Json<BaseResponse<T>>, AppError>;
pub type AppResultPage<T> = Result<Json<BaseResponse<PageResult<T>>>, AppError>;

#[async_trait]
impl Writer for AppError {
    async fn write(mut self, _req: &mut Request, _: &mut Depot, res: &mut Response) {
        res.render(Json(BaseResponse {
            msg: self.to_string(),
            code: 1,
            data: Some("None".to_string()),
        }))
    }
}
