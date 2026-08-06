use crate::common::error::{AppResult, AppResultPage};
use crate::common::result::{ok_result, ok_result_data, ok_result_page};
use crate::sys_user_service::UserService;
use crate::sys_user_vo::{
    DeleteUserReq, QueryUserDetailReq, QueryUserListReq, UpdateUserStatusReq, UserReq, UserResp,
};
use salvo::oapi::extract::JsonBody;
use salvo::prelude::*;
use tracing::info;

///添加用户信息
#[handler]
pub async fn add_sys_user(depot: &mut Depot, req: JsonBody<UserReq>) -> AppResult<i64> {
    let item = req.into_inner();
    info!("add sys_user params: {:?}", &item);

    let id = UserService::add_sys_user(depot, item).await?;
    ok_result_data(id)
}

///删除用户信息
#[handler]
pub async fn delete_sys_user(depot: &mut Depot, req: JsonBody<DeleteUserReq>) -> AppResult<String> {
    let item = req.into_inner();
    info!("delete sys_user params: {:?}", &item);

    UserService::delete_sys_user(depot, item).await?;
    ok_result()
}

///更新用户信息
#[handler]
pub async fn update_sys_user(depot: &mut Depot, req: JsonBody<UserReq>) -> AppResult<String> {
    let item = req.into_inner();
    info!("update sys_user params: {:?}", &item);

    UserService::update_sys_user(depot, item).await?;
    ok_result()
}

///更新用户信息状态
#[handler]
pub async fn update_sys_user_status(
    depot: &mut Depot,
    req: JsonBody<UpdateUserStatusReq>,
) -> AppResult<String> {
    let item = req.into_inner();
    info!("update sys_user_status params: {:?}", &item);

    UserService::update_sys_user_status(depot, item).await?;
    ok_result()
}

///查询用户信息详情
#[handler]
pub async fn query_sys_user_detail(
    depot: &mut Depot,
    req: JsonBody<QueryUserDetailReq>,
) -> AppResult<UserResp> {
    let item = req.into_inner();
    info!("query sys_user_detail params: {:?}", &item);

    let data = UserService::query_sys_user_detail(depot, item).await?;
    ok_result_data(data)
}

///查询用户信息列表
#[handler]
pub async fn query_sys_user_list(
    depot: &mut Depot,
    req: JsonBody<QueryUserListReq>,
) -> AppResultPage<UserResp> {
    let item = req.into_inner();
    info!("query sys_user_list params: {:?}", &item);

    let (list, total) = UserService::query_sys_user_list(depot, item).await?;
    ok_result_page(list, total)
}
