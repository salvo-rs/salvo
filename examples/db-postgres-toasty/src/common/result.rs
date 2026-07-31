use crate::common::error::{AppResult, AppResultPage};
use salvo::writing::Json;
use serde::Serialize;
use std::fmt::Debug;
use jiff::Timestamp;

// 统一返回vo
#[derive(Serialize, Debug, Clone)]
pub struct BaseResponse<T> {
    pub code: i32,
    pub msg: String,
    pub data: Option<T>,
}

#[derive(Serialize, Debug, Clone)]
pub struct PageResult<T> {
    pub list: Vec<T>,
    pub total: u64,
}


pub fn ok_result() -> AppResult<String> {
    ok_result_msg("success".to_string())
}

pub fn ok_result_msg(msg: String) -> AppResult<String> {
    let response = BaseResponse {
        msg,
        code: 0,
        data: Some("None".to_string()),
    };
    Ok(Json(response))
}

pub fn ok_result_data<T: Serialize + Send>(data: T) -> AppResult<T> {
    let response = BaseResponse {
        msg: "success".to_string(),
        code: 0,
        data: Some(data),
    };
    Ok(Json(response))
}

pub fn err_result_msg(msg: String) -> AppResult<String> {
    let response = BaseResponse {
        msg,
        code: 1,
        data: Some("None".to_string()),
    };
    Ok(Json(response))
}

pub fn ok_result_page<T: Serialize + Send>(list: Vec<T>, total: u64) -> AppResultPage<T> {
    Ok(Json(BaseResponse {
        msg: "success".to_string(),
        code: 0,
        data: Some(PageResult { list, total }),
    }))
}

pub fn serialize_datetime<S>(dt: &Timestamp, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let formatted = dt.strftime("%Y-%m-%d %H:%M:%S").to_string();
    serializer.serialize_str(&formatted)
}

pub fn serialize_datetime_opt<S>(dt: &Option<Timestamp>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match dt {
        Some(datetime) => {
            let formatted = datetime.strftime("%Y-%m-%d %H:%M:%S").to_string();
            serializer.serialize_str(&formatted)
        }
        None => serializer.serialize_str(""),
    }
}
