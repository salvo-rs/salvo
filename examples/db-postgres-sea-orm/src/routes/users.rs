use chrono::Utc;
use salvo::prelude::*;
use salvo_oapi::endpoint;
use salvo_oapi::extract::{HeaderParam, JsonBody, PathParam};
use sea_orm::ActiveValue::Set;
use sea_orm::{ActiveModelTrait, EntityTrait, ModelTrait, QueryFilter};
use time::{OffsetDateTime, Duration};
use jsonwebtoken::{EncodingKey, encode};
use uuid::Uuid;
use crate::auth::auth::auth_user;
use crate::database::db::DbPool;
use crate::models::posts;
use crate::schemas::{ErrorResponseModel, JwtClaims, TokenResponseModel};
use crate::schemas::users::{UserCreate, UserCredentiel, UserResponseModel, UserSuccessResponseModel, UserUpdate};
use crate::utils::utils::{hash_password, verify_password};
use std::sync::Arc;
use crate::utils::SECRET_KEY;
use crate::models::users::{self, Entity as Users};
use crate::models::posts::Entity as Posts;
use crate::models::users::Column as UserColumn;
use crate::models::posts::Column as PostColumn;
use sea_orm::ColumnTrait;

#[endpoint(
    tags("Users"),
    summary = "Get all users",
    description = "The objective of this endpoint is to retrieve all the users in database"
)]
async fn get_all_users(res: &mut Response, authentication: HeaderParam<String, true>, depot: &mut Depot) {
    
    
    println!("🪪 Authentication header: {}", authentication.as_str());

    // ✅ Get DB connection
    let connection = depot.obtain::<Arc<DbPool>>().unwrap();
    let db = &**connection;


    let current_user: &users::Model = depot.get::<users::Model>("user").unwrap();


    println!("👤 Current user: {:?}", current_user);

    let all_users = Users::load().all(db)
                    .await
                    .expect("Error loading users");

    let all_users_response: Vec<UserResponseModel> = all_users
        .iter()
        .map(|user| UserResponseModel{
            id: user.id, email: user.username.clone(), 
            full_name: user.username.clone(), 
            created_at: user.created_at.clone(),
            updated_at: user.updated_at.clone(),
        })
        .collect();

    res.render(Json(all_users_response))
}

#[endpoint(
    tags("Users"),
    summary = "Create users",
    description = "The objective of this endpoint is to create the new users"
)]
async fn create_users(res: &mut Response,  depot: &mut Depot, user_create: JsonBody<UserCreate>,) {

    // ✅ Get DB connection
    let connection = depot.obtain::<Arc<DbPool>>().unwrap();
    let db = &**connection;

   println!("📥 Create new user ...");

    // ✅ Check if user already exists
    let existing_user: Option<users::Model> = Users::find()
                .filter(UserColumn::Username.eq(user_create.email.clone()))
                .one(db)
                .await
                .expect("Error to loading users");

    if existing_user.is_some() {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(Json(ErrorResponseModel{
            detail: format!("🚫 User '{}' already exists", user_create.email).to_string()
        }));
        return;
    }

    // ✅ Hash the password
    let hashed = match hash_password(&user_create.password.clone().as_str()) {
        Ok(h) => h,
        Err(e) => {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(Json(ErrorResponseModel{
                detail: format!("❌ Password hashing error: {}", e).to_string(),
            }));
            return;
        }
    };

    let now = Utc::now().naive_utc();

    // ✅ Create new user
    let new_user = users::ActiveModel {
        id: Set(Uuid::new_v4()),
        username: Set(user_create.email.clone()),
        full_name: Set(user_create.fullname.clone()),
        password: Set(hashed),
        created_at: Set(now),
        updated_at: Set(now),
    };

    // ✅ Insert into DB
    let user = Users::insert(new_user)
                                            .exec(db)
                                            .await
                                            .expect("Error to insert users");

    println!("The last inserted user id : {}", {user.last_insert_id});

    // ✅ Respond success
    res.status_code(StatusCode::CREATED);
    res.render(format!("✅ User '{}' created successfully!", user_create.email));

}

#[endpoint(
    tags("Users"),
    summary = "Get users information",
    description = "The objective of the endpoints is to get users information"
)]
pub async fn get_users_information(res: &mut Response, depot: &mut Depot, authentication: HeaderParam<String, true>) {
    
    println!("🪪 Authentication header: {}", authentication.as_str());

    // ✅ Get DB connection
    let connection = depot.obtain::<Arc<DbPool>>().unwrap();
    let _db  = &**connection;

    println!("📥 Fetching user information...");

    let current_user: &users::Model = depot.get::<users::Model>("user").unwrap();
    
    println!("👤 Current user: {:?}", current_user);


    // ✅ Build response model
    let user_response_model = UserResponseModel {
        id: current_user.id.clone(),
        email: current_user.username.clone(), // or change field if you have separate email
        full_name: current_user.full_name.clone(),
        created_at: current_user.created_at.clone(),
        updated_at: current_user.created_at.clone(),
        

    };

    // ✅ Send JSON response
    res.status_code(StatusCode::OK);
    res.render(Json(user_response_model));
}

#[endpoint(
    tags("Users"),
    summary = "Update users information",
    description = "The objectve of this endpoints is to update users information"
)]
async fn update_users(user_id: PathParam<Uuid>, res: &mut Response, authentication: HeaderParam<String, true>, depot: &mut Depot, user_update: JsonBody<UserUpdate>) {
    
    println!("🪪 Authentication header: {}", authentication.as_str());

    let user_uuid = user_id.into_inner();

    // ✅ Get DB connection
    let connection = depot.obtain::<Arc<DbPool>>().unwrap();
    let db = &**connection;

    let current_user: &users::Model  = depot.get::<users::Model>("user").unwrap();

    // ✅ Check permission (user can only update their own info)
    if current_user.id != user_uuid {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(Json(ErrorResponseModel {
            detail: format!(
                "❌ You cannot update the user with id: {}",
                user_uuid
            ),
        }));
        return;
    }
    println!("👤 Current user: {:?}", current_user);

    let update_data = user_update.into_inner();

    let user: Option<users::Model> = Users::find_by_id(user_uuid)
                                .one(db)
                                .await
                                .expect("failed to query users");

    let mut user: users::ActiveModel  = user.unwrap().into();

    user.full_name = Set(update_data.fullname.clone());
    user.updated_at = Set(Utc::now().naive_utc());

    let user = user
                            .update(db)
                            .await;
    if user.is_err(){
         res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Json(ErrorResponseModel {
            detail: format!("❌ Failed to update user: {}", user.err().unwrap()),
        }));
        return ;
    } else {
        res.status_code(StatusCode::OK);
        let user = user.unwrap();
        res.render(Json(UserSuccessResponseModel{
            id: user.id,
            email: user.username,
            full_name: user.full_name,
            created_at: user.created_at,
            updated_at: user.updated_at,
        }));
    }

}

#[endpoint(
    tags("Users"),
    summary = "Delete Users Information",
    description = "The objective of this endpoints is to delete users information"
)]
async fn delete_users(user_id: PathParam<Uuid>, res: &mut Response, authentication: HeaderParam<String, true>, depot: &mut Depot) {
    
    println!("🪪 Authentication header: {}", authentication.as_str());

    let connection = depot.obtain::<Arc<DbPool>>().unwrap();
    let db = &**connection;


    let current_user: &users::Model  = depot.get::<users::Model>("user").unwrap();

    let user_uuid = user_id.into_inner();

    // ✅ Check permission (user can only update their own info)
    if current_user.id != user_uuid {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(Json(ErrorResponseModel {
            detail: format!(
                "❌ You cannot delete the user with id: {}",
                user_uuid
            ),
        }));
        return;
    }
    println!("👤 Current user: {:?}", current_user);

    let user: Option<users::Model> = Users::find_by_id(user_uuid)
                                    .one(db)
                                    .await
                                    .expect("failed to query users in database");

    let user: users::Model = user.unwrap();
    
    let user_delete = user.delete(db).await;

    if user_delete.is_err(){
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Json(ErrorResponseModel {
            detail: format!("❌ Failed to delete user: {}", user_delete.err().unwrap()),
        }));
        return ;
    } else {
        let row_affected= user_delete.unwrap().rows_affected;
        if row_affected==0{
            res.status_code(StatusCode::NOT_FOUND);
            res.render(Json(ErrorResponseModel {
                detail: "❌ User not found".to_string(),
            }));
            return;
        } else{
            println!("affected row: {}", row_affected);
            res.status_code(StatusCode::OK);
            res.render(Json(UserSuccessResponseModel {
                    id: current_user.id,
                    email: current_user.username.clone(),
                    full_name: current_user.full_name.clone(),
                    created_at: current_user.created_at.clone(),
                    updated_at: current_user.updated_at.clone(),

                }));
        }
    }
}

#[endpoint(
    tags("Users"),
    summary = "Get all posts by Users",
    description = "The objective of the endpoints is to get all post given users"
)]
async fn get_posts_by_users(user_id: PathParam<Uuid>, res: &mut Response, authentication: HeaderParam<String, true>, depot: &mut Depot) {
    
    println!("🪪 Authentication header: {}", authentication.as_str());

    let connection = depot.obtain::<Arc<DbPool>>().unwrap();
    let db = &**connection;


    let current_user: &users::Model  = depot.get::<users::Model>("user").unwrap();

    let user_uuid = user_id.into_inner();

    // ✅ Check permission (user can only update their own info)
    if current_user.id != user_uuid {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(Json(ErrorResponseModel {
            detail: format!(
                "❌ You cannot get all post of the user with id: {}",
                user_uuid
            ),
        }));
        return;
    }
    println!("👤 Current user: {:?}", current_user);


    let all_posts: Vec<posts::Model> = Posts::find()
                .filter(PostColumn::UserId.eq(current_user.id.clone()))
                .all(db)
                .await
                .expect("Error to loading posts");

    res.status_code(StatusCode::OK);
    res.render(Json(all_posts));
}

#[endpoint(
    tags("Users"),
    summary = "Get access token for login",
    description = "The objective of this endpoint is to get access token of the given users"
)]
async fn get_access_token(res: &mut Response, user_credentiel: JsonBody<UserCredentiel>, depot: &mut Depot) {

    let connection = depot.obtain::<Arc<DbPool>>().unwrap();
    let db = &**connection;
    // ✅ Query user by ID

    let existing_user: Option<users::Model> = Users::find()
                        .filter(UserColumn::Username.eq(user_credentiel.username.clone()))
                        .one(db)
                        .await
                        .expect("❌ Failed to query user");

    // ✅ Handle "not found" case
    let Some(user) = existing_user else {
        print!("no existing users");
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(Json(
            ErrorResponseModel{
                detail: format!("🚫 Invalid username or password")
            }
        ));
        return;
    };

    if !verify_password(&user_credentiel.password.clone().as_str(), &user.password.clone().as_str()){
        println!("🚫 Bad password");
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(Json(
            ErrorResponseModel{
                detail: format!("🚫 Invalid username or password")
            }
        ));
        return;
    }

    let exp = OffsetDateTime::now_utc() + Duration::days(1);
    let claim = JwtClaims {
        username: user.username.clone(),
        exp: exp.unix_timestamp(),
    };
    let token = encode(
        &jsonwebtoken::Header::default(),
        &claim,
        &EncodingKey::from_secret(SECRET_KEY.as_bytes()),
    );

    res.status_code(StatusCode::OK);
    res.render(Json(TokenResponseModel{
        token_type: String::from("Bearer"),
        token: String::from(token.unwrap())
    }));
}

pub fn get_users_router() -> Router {
    Router::with_path("users")
        // 🟢 Public routes
        .push(Router::with_path("login").post(get_access_token))
        .push(Router::with_path("").post(create_users))
        
        // 🔒 Protected routes
        .push(
            Router::with_path("me")
                .hoop(auth_user) // middleware only for this
                .get(get_users_information),
        )
        .push(
            Router::with_path("{user_id}")
                .hoop(auth_user) // protect this subtree
                .put(update_users)
                .delete(delete_users)
                .push(
                    Router::with_path("posts")
                        .get(get_posts_by_users),
                ),
        )
        .push(Router::with_path("")
                .hoop(auth_user)
                .get(get_all_users)
    )
}
