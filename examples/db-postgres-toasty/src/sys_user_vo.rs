use crate::common::result::serialize_datetime;
use crate::common::result::serialize_datetime_opt;
use jiff::Timestamp;
use serde::{Deserialize, Serialize};
///删除用户信息请求参数
#[derive(Debug, Serialize, Deserialize)]
pub struct DeleteUserReq {
    pub ids: Vec<i64>, //主键
}

///用户信息请求参数
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserReq {
    pub id: Option<i64>,   //主键
    pub mobile: String,    //手机号码
    pub user_name: String, //用户账号
    pub nick_name: String, //用户昵称
    pub user_type: String, //用户类型（00系统用户）
    pub avatar: String,    //头像路径
    pub email: String,     //用户邮箱
    pub password: String,  //密码
    pub status: i8,        //状态(1:正常，0:禁用)
    pub dept_id: i64,      //部门ID
    pub remark: String,    //备注
}

///更新用户信息状态请求参数
#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateUserStatusReq {
    pub ids: Vec<i64>, //主键
    pub status: i8,    //状态
}
///查询用户信息详情请求参数
#[derive(Debug, Serialize, Deserialize)]
pub struct QueryUserDetailReq {
    pub id: i64, //主键
}

///查询用户信息列表请求参数
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct QueryUserListReq {
    pub page_no: usize,
    pub page_size: usize,
    pub mobile: Option<String>,    //手机号码
    pub user_name: Option<String>, //用户账号
    pub nick_name: Option<String>, //用户昵称
    pub email: Option<String>,     //用户邮箱
    pub status: Option<i8>,        //状态(1:正常，0:禁用)
    pub dept_id: Option<i64>,      //部门ID
}

///查询用户信息响应参数
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserResp {
    pub id: i64,           //主键
    pub mobile: String,    //手机号码
    pub user_name: String, //用户账号
    pub nick_name: String, //用户昵称
    pub user_type: String, //用户类型（00系统用户）
    pub avatar: String,    //头像路径
    pub email: String,     //用户邮箱
    pub password: String,  //密码
    pub status: i8,        //状态(1:正常，0:禁用)
    pub dept_id: i64,      //部门ID
    pub remark: String,    //备注
    pub create_by: String, //创建者
    #[serde(serialize_with = "serialize_datetime")]
    pub create_time: Timestamp, //创建时间
    pub update_by: Option<String>, //更新者
    #[serde(serialize_with = "serialize_datetime_opt")]
    pub update_time: Option<Timestamp>, //更新时间
}
