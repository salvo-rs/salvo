use salvo::Depot;

use crate::common::error::AppError;
use crate::common::error::AppError::BusinessError;
use crate::sys_user_model::User;
use crate::sys_user_vo::{
    DeleteUserReq, QueryUserDetailReq, QueryUserListReq, UpdateUserStatusReq, UserReq, UserResp,
};

pub struct UserService;

impl UserService {
    ///添加用户信息
    pub async fn add_sys_user(depot: &mut Depot, req: UserReq) -> Result<i64, AppError> {
        let db = depot
            .get::<toasty::Db>("db")
            .map_err(|_| BusinessError("获取db异常".to_string()))?;

        let count = User::filter(User::fields().user_name().eq(&req.user_name))
            .count()
            .exec(&mut db.clone())
            .await?;
        if count > 0 {
            return Err(BusinessError("用户账号已存在".to_string()));
        }

        let count = User::filter(User::fields().mobile().eq(&req.mobile))
            .count()
            .exec(&mut db.clone())
            .await?;
        if count > 0 {
            return Err(BusinessError("用手机号码已存在".to_string()));
        }

        let count = User::filter(User::fields().email().eq(&req.email))
            .count()
            .exec(&mut db.clone())
            .await?;
        if count > 0 {
            return Err(BusinessError("用户邮箱已存在".to_string()));
        }

        let user = User::create()
            .mobile(req.mobile) //手机号码
            .user_name(req.user_name) //用户账号
            .nick_name(req.nick_name) //用户昵称
            .user_type(req.user_type) //用户类型（00系统用户）
            .avatar(req.avatar) //头像路径
            .email(req.email) //用户邮箱
            .password(req.password) //密码
            .status(req.status) //状态(1:正常，0:禁用)
            .dept_id(req.dept_id) //部门ID
            .remark(req.remark) //备注
            .exec(&mut db.clone())
            .await?;

        Ok(user.id)
    }

    ///删除用户信息
    pub async fn delete_sys_user(depot: &mut Depot, req: DeleteUserReq) -> Result<(), AppError> {
        let db = depot
            .get::<toasty::Db>("db")
            .map_err(|_| BusinessError("获取db异常".to_string()))?;

        User::filter(User::fields().id().in_list(req.ids))
            .delete()
            .exec(&mut db.clone())
            .await
            .map(|_| Ok(()))?
    }

    /*
     *更新用户信息
     *author：刘飞华
     *date：2026/07/14 11:02:18
     */
    pub async fn update_sys_user(depot: &mut Depot, req: UserReq) -> Result<(), AppError> {
        let db = depot
            .get::<toasty::Db>("db")
            .map_err(|_| BusinessError("获取db异常".to_string()))?;
        let id = req.id;
        if id.is_none() {
            return Err(BusinessError("用户ID不能为空".to_string()));
        }
        let user_id = id.unwrap_or_default();

        let opt_user = User::filter_by_id(user_id)
            .first()
            .exec(&mut db.clone())
            .await?;
        match opt_user {
            None => Err(BusinessError("用户信息不存在".to_string())),
            Some(mut record) => {
                let count = User::filter(User::fields().user_name().eq(&req.user_name))
                    .filter(User::fields().id().ne(user_id))
                    .count()
                    .exec(&mut db.clone())
                    .await?;
                if count > 0 {
                    return Err(BusinessError("用户账号已存在".to_string()));
                }

                let count = User::filter(User::fields().mobile().eq(&req.mobile))
                    .filter(User::fields().id().ne(user_id))
                    .count()
                    .exec(&mut db.clone())
                    .await?;
                if count > 0 {
                    return Err(BusinessError("用手机号码已存在".to_string()));
                }

                let count = User::filter(User::fields().email().eq(&req.email))
                    .filter(User::fields().id().ne(user_id))
                    .count()
                    .exec(&mut db.clone())
                    .await?;
                if count > 0 {
                    return Err(BusinessError("用户邮箱已存在".to_string()));
                }

                record
                    .update()
                    .mobile(req.mobile) //手机号码
                    .user_name(req.user_name) //用户账号
                    .nick_name(req.nick_name) //用户昵称
                    .user_type(req.user_type) //用户类型（00系统用户）
                    .avatar(req.avatar) //头像路径
                    .email(req.email) //用户邮箱
                    .password(req.password) //密码
                    .status(req.status) //状态(1:正常，0:禁用)
                    .dept_id(req.dept_id) //部门ID
                    .remark(req.remark) //备注
                    .exec(&mut db.clone())
                    .await.map(|_| Ok(()))?
            }
        }
    }

    ///更新用户信息状态
    pub async fn update_sys_user_status(
        depot: &mut Depot,
        req: UpdateUserStatusReq,
    ) -> Result<(), AppError> {
        let db = depot
            .get::<toasty::Db>("db")
            .map_err(|_| BusinessError("获取db异常".to_string()))?;

        User::filter(User::fields().id().in_list(req.ids))
            .update()
            .status(req.status)
            .exec(&mut db.clone())
            .await
            .map(|_| Ok(()))?
    }

    ///查询用户信息详情
    pub async fn query_sys_user_detail(
        depot: &mut Depot,
        req: QueryUserDetailReq,
    ) -> Result<UserResp, AppError> {
        let db = depot
            .get::<toasty::Db>("db")
            .map_err(|_| BusinessError("获取db异常".to_string()))?;

        let opt_user = User::filter_by_id(req.id)
            .first()
            .exec(&mut db.clone())
            .await?;
        match opt_user {
            None => Err(BusinessError("用户信息不存在".to_string())),
            Some(item) => Ok(Self::build_res(item)),
        }
    }

    fn build_res(item: User) -> UserResp {
        UserResp {
            id: item.id,                   //主键
            mobile: item.mobile,           //手机号码
            user_name: item.user_name,     //用户账号
            nick_name: item.nick_name,     //用户昵称
            user_type: item.user_type,     //用户类型（00系统用户）
            avatar: item.avatar,           //头像路径
            email: item.email,             //用户邮箱
            password: item.password,       //密码
            status: item.status,           //状态(1:正常，0:禁用)
            dept_id: item.dept_id,         //部门ID
            remark: item.remark,           //备注
            create_by: item.create_by,     //创建者
            create_time: item.create_time, //创建时间
            update_by: item.update_by,     //更新者
            update_time: item.update_time, //更新时间
        }
    }

    ///查询用户信息列表
    pub async fn query_sys_user_list(
        depot: &mut Depot,
        req: QueryUserListReq,
    ) -> Result<(Vec<UserResp>, u64), AppError> {
        let db = depot
            .get::<toasty::Db>("db")
            .map_err(|_| BusinessError("获取db异常".to_string()))?;

        let mut dept_ids = Vec::new();
        if let Some(x) = req.dept_id {
            // 查询部门下的用户
            // dept_ids = toasty::sql::query(
            //     "SELECT id FROM sys_dept WHERE $1 = ANY(string_to_array(ancestors, ','))",
            // )
            // .bind(x)
            // .column_types([stmt::Type::I64])
            // .exec(&mut db.clone())
            // .await?
            // .into_iter()
            // .map(|row| row.to_i64().unwrap_or_default())
            // .collect::<Vec<i64>>();

            dept_ids.push(x);
        }

        let page_no = req.page_no;
        let page_size = req.page_size;
        let offset = (page_no - 1) * page_size;

        let mut query_build = User::all();
        if let Some(x) = req.mobile {
            //手机号码
            query_build = query_build.filter(User::fields().mobile().like(format!("%{}%", x)));
        }
        if let Some(x) = req.user_name {
            //用户账号
            query_build = query_build.filter(User::fields().user_name().like(format!("%{}%", x)));
        }
        if let Some(x) = req.nick_name {
            //用户昵称
            query_build = query_build.filter(User::fields().nick_name().like(format!("%{}%", x)));
        }
        if let Some(x) = req.email {
            //用户邮箱
            query_build = query_build.filter(User::fields().email().like(format!("%{}%", x)));
        }
        if let Some(x) = req.status {
            //状态(1:正常，0:禁用)
            query_build = query_build.filter(User::fields().status().eq(x));
        }
        if let Some(_) = req.dept_id {
            //部门ID
            query_build = query_build.filter(User::fields().dept_id().in_list(dept_ids));
        }
        let total = query_build.clone().count().exec(&mut db.clone()).await?;
        query_build
            .latest_by(User::fields().create_time())
            .limit(page_size)
            .offset(offset)
            .exec(&mut db.clone())
            .await
            .map(|x| {
                Ok((
                    x.into_iter()
                        .map(|x| Self::build_res(x))
                        .collect::<Vec<UserResp>>(),
                    total,
                ))
            })?
    }
}
