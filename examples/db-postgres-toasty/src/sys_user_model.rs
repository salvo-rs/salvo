use jiff::Timestamp;

/// 用户信息
#[derive(Debug, toasty::Model)]
#[table = "sys_user_test"]
pub struct User {
    #[key]
    #[auto(increment)]
    pub id: i64, //主键
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
    #[default("admin".to_string())]
    pub create_by: String, //创建者
    #[index]
    #[default(jiff::Timestamp::now())]
    pub create_time: Timestamp, //创建时间
    #[update(Some("admin".to_string()))]
    pub update_by: Option<String>, //更新者
    #[update(Some(jiff::Timestamp::now()))]
    pub update_time: Option<Timestamp>, //更新时间
}
