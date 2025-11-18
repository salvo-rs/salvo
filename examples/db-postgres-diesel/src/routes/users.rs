use chrono::Utc;
use salvo::prelude::*;
use salvo_oapi::endpoint;
use salvo_oapi::extract::{HeaderParam, JsonBody, PathParam};
use time::{OffsetDateTime, Duration};
use jsonwebtoken::{EncodingKey, encode};
use uuid::Uuid;
use crate::auth::auth::auth_user;
use crate::database::db::DbPool;
use crate::models::schema::users::{full_name, id, updated_at, username};
use crate::schemas::{ErrorResponseModel, JwtClaims, TokenResponseModel};
use crate::schemas::users::{UserCreate, UserCredentiel, UserResponseModel, UserSuccessResponseModel, UserUpdate};
use crate::models::users::{NewUser, Users};
use crate::models::posts:: {Posts};
use crate::utils::utils::{hash_password, verify_password};
use std::sync::Arc;
use crate::models::schema::users::dsl::users;
use crate::models::schema::posts::dsl::posts;
use diesel::prelude::*;
use crate::utils::SECRET_KEY;


#[endpoint(
    tags("Users"),
    summary = "get all users",
    description = "the objective of this endpoint is to retreive all the users in database"
)]
fn get_all_users(res: &mut Response, authentification: HeaderParam<String, true>, depot: &mut Depot) {
    
    
    println!("🪪 Authentication header: {}", authentification.as_str());

    // ✅ Get DB connection
    let pool = depot.obtain::<Arc<DbPool>>().unwrap();
    let mut conn = pool.get().expect("❌ Failed to get DB connection");


    let current_user: &Users = depot.get::<Users>("user").unwrap();


    println!("👤 Current user: {:?}", current_user);

    let all_users = users
                .load::<Users>(&mut conn)
                .expect("Error loading users");

    let all_users_respone: Vec<UserResponseModel> = all_users
        .iter()
        .map(|user| UserResponseModel{
            id: user.id, email: user.username.clone(), 
            full_name: user.username.clone(), 
            created_at: user.created_at.clone(),
            updated_at: user.updated_at.clone(),
        })
        .collect();

    res.render(Json(all_users_respone))
}

#[endpoint(
    tags("Users"),
    summary = "create users",
    description = "the objective of this endpoint is to create the new users"
)]
fn create_users(res: &mut Response,  depot: &mut Depot, user_create: JsonBody<UserCreate>,) {

    // ✅ Get DB connection
    let pool = depot.obtain::<Arc<DbPool>>().unwrap();
    let mut conn = pool.get().expect("❌ Failed to get DB connection");

   println!("📥 Create new user ...");

    // ✅ Check if user already exists
    let existing_user = users
        .filter(username.eq(&user_create.email))
        .first::<Users>(&mut conn)
        .optional()
        .expect("❌ Failed to query user");

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
    let new_user = NewUser {
        id: Uuid::new_v4(),
        username: user_create.email.clone(),
        password: hashed,
        full_name: user_create.fullname.clone(),
        created_at: now,
        updated_at: now,
    };

    // ✅ Insert into DB
    diesel::insert_into(users)
        .values(&new_user)
        .execute(&mut conn)
        .expect("❌ Failed to insert new user");

    // ✅ Respond success
    res.status_code(StatusCode::CREATED);
    res.render(format!("✅ User '{}' created successfully!", user_create.email));

}

#[endpoint(
    tags("Users"),
    summary = "get users information",
    description = "the objective of the endpoints is to get users information"
)]
pub async fn get_users_information(res: &mut Response, depot: &mut Depot, authentification: HeaderParam<String, true>) {
    
    println!("🪪 Authentication header: {}", authentification.as_str());

    // ✅ Get DB connection
    let pool = depot.obtain::<Arc<DbPool>>().unwrap();
    let mut _conn = pool.get().expect("❌ Failed to get DB connection");

    println!("📥 Fetching user information...");

    let current_user = depot.get::<Users>("user").unwrap();
    
    println!("👤 Current user: {:?}", current_user);


    // ✅ Build response model
    let user_response_model = UserResponseModel {
        id: current_user.id,
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
    description = "the objectve of this endpoints is to update users information"
)]
fn update_users(user_id: PathParam<Uuid>, res: &mut Response, authentification: HeaderParam<String, true>, depot: &mut Depot, user_update: JsonBody<UserUpdate>) {
    
    println!("🪪 Authentication header: {}", authentification.as_str());

    let user_uuid = user_id.into_inner();

    let pool = depot.obtain::<Arc<DbPool>>().unwrap();
    let mut conn = pool.get().expect("Failed to get DB connection");

    let current_user: &Users  = depot.get::<Users>("user").unwrap();

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

    let result = diesel::update(users.find(user_uuid))
        .set((
            full_name.eq(&update_data.fullname),
            updated_at.eq(&Utc::now().naive_utc())
            ),
        )
        .execute(&mut conn)
        ;

    match result {
        Ok(affected_rows) => {
            if affected_rows == 0 {
                res.status_code(StatusCode::NOT_FOUND);
                res.render(Json(ErrorResponseModel {
                    detail: format!("⚠️ No user found with id {}", user_uuid),
                }));
                return ;
            } else {
                res.status_code(StatusCode::OK);
                res.render(Json(UserSuccessResponseModel {
                    id: current_user.id,
                    email: current_user.username.clone(),
                    full_name: current_user.full_name.clone(),
                    created_at: current_user.created_at.clone(),
                    updated_at: current_user.updated_at.clone()

                }));
                return ;
            }
        }
        Err(e) => {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(Json(ErrorResponseModel {
                detail: format!("❌ Failed to update user: {}", e),
            }));
            return ;
        }
    }

}

#[endpoint(
    tags("Users"),
    summary = "Delete Users Information",
    description = "the objective of this endpoints is to delete users information"
)]
fn delete_users(user_id: PathParam<Uuid>, res: &mut Response, authentification: HeaderParam<String, true>, depot: &mut Depot) {
    
    println!("🪪 Authentication header: {}", authentification.as_str());

    let pool = depot.obtain::<Arc<DbPool>>().unwrap();
    let mut conn = pool.get().expect("Failed to get DB connection");


    let current_user: &Users  = depot.get::<Users>("user").unwrap();

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

    let affected = diesel::delete(users.filter(id.eq(user_uuid)))
        .execute(&mut conn);

    match affected {
        Ok(0) => {
            res.status_code(StatusCode::NOT_FOUND);
            res.render(Json(ErrorResponseModel {
                detail: "❌ User not found".to_string(),
            }));
        }
        Ok(affected_row) => {
            println!("affected row: {}", affected_row);
            res.status_code(StatusCode::OK);
            res.render(Json(UserSuccessResponseModel {
                    id: current_user.id,
                    email: current_user.username.clone(),
                    full_name: current_user.full_name.clone(),
                    created_at: current_user.created_at.clone(),
                    updated_at: current_user.updated_at.clone(),

                }));
        }
        Err(err) => {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(Json(ErrorResponseModel {
                detail: format!("❌ Failed to delete user: {}", err),
            }));
        }
    }

}

#[endpoint(
    tags("Users"),
    summary = "Get all posts by Users",
    description = "the objective of the endpoints is to get all post given users"
)]
fn get_posts_by_users(user_id: PathParam<Uuid>, res: &mut Response, authentification: HeaderParam<String, true>, depot: &mut Depot) {
    
    println!("🪪 Authentication header: {}", authentification.as_str());

    let pool = depot.obtain::<Arc<DbPool>>().unwrap();
    let mut conn = pool.get().expect("Failed to get DB connection");


    let current_user: &Users  = depot.get::<Users>("user").unwrap();

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

    let all_posts = posts
            .filter(crate::models::schema::posts::user_id.eq(&current_user.id))
            .load::<Posts>(&mut conn)
            .expect("Failed to get all posts of the user");

    res.status_code(StatusCode::OK);
    res.render(Json(all_posts));
}

#[endpoint(
    tags("Users"),
    summary = "get access token for login",
    description = "The objective of this endpoint is to get access token of the given users"
)]
fn get_access_token(res: &mut Response, user_credentiel: JsonBody<UserCredentiel>, depot: &mut Depot) {

    let pool = depot.obtain::<Arc<DbPool>>().unwrap();
    let mut conn = pool.get().expect("Failed to get DB connection");

    // ✅ Query user by ID
    let existing_user = users
        .filter(username.eq(&user_credentiel.username))
        .first::<Users>(&mut conn)
        .optional()
        .expect("❌ Failed to query user");

    // ✅ Handle "not found" case
    let Some(user) = existing_user else {
        print!("no existing users");
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(Json(
            ErrorResponseModel{
                detail: format!("🚫 Invalide username or password")
            }
        ));
        return;
    };

    if !verify_password(&user_credentiel.password.clone().as_str(), &user.password.clone().as_str()){
        print!("bad password");
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(Json(
            ErrorResponseModel{
                detail: format!("🚫 Invalide user or password")
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
        type_token: String::from("Bearer"),
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
