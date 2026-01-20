use std::sync::Arc;

use chrono::Utc;
use diesel::prelude::*;
use jsonwebtoken::{EncodingKey, encode};
use salvo::prelude::*;
use salvo_oapi::endpoint;
use salvo_oapi::extract::{HeaderParam, JsonBody, PathParam};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

use crate::auth::auth_user;
use crate::db::DbPool;
use crate::models::posts::Post;
use crate::models::users::{NewUser, ResUserBody, User, UserCreate, UserCredentiel, UserUpdate};
use crate::models::{JwtClaims, ResErrorBody, ResTokenBody};
use crate::schema::*;
use crate::utils::{SECRET_KEY, hash_password, verify_password};

#[endpoint(
    tags("Users"),
    summary = "get all users",
    description = "the objective of this endpoint is to retrieve all the users in database"
)]
fn get_all_users(res: &mut Response, authentication: HeaderParam<String, true>, depot: &mut Depot) {
    println!("🪪 Authentication header: {}", authentication.as_str());

    // ✅ Get DB connection
    let pool = depot.obtain::<Arc<DbPool>>().unwrap();
    let mut conn = pool.get().expect("❌ Failed to get DB connection");

    let current_user = depot.get::<User>("user").unwrap();

    println!("👤 Current user: {:?}", current_user);

    let all_users = users::table
        .load::<User>(&mut conn)
        .expect("Error loading users");

    let all_users_response: Vec<ResUserBody> = all_users
        .iter()
        .map(|user| ResUserBody {
            id: user.id,
            email: user.username.clone(),
            full_name: user.username.clone(),
            created_at: user.created_at,
            updated_at: user.updated_at,
        })
        .collect();

    res.render(Json(all_users_response))
}

#[endpoint(
    tags("Users"),
    summary = "create users",
    description = "the objective of this endpoint is to create the new users"
)]
fn create_users(res: &mut Response, depot: &mut Depot, user_create: JsonBody<UserCreate>) {
    // ✅ Get DB connection
    let pool = depot.obtain::<Arc<DbPool>>().unwrap();
    let mut conn = pool.get().expect("❌ Failed to get DB connection");

    println!("📥 Create new user ...");

    // ✅ Check if user already exists
    let existing_user = users::table
        .filter(users::username.eq(&user_create.email))
        .first::<User>(&mut conn)
        .optional()
        .expect("❌ Failed to query user");

    if existing_user.is_some() {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(Json(ResErrorBody {
            detail: format!("🚫 User '{}' already exists", user_create.email).to_string(),
        }));
        return;
    }

    // ✅ Hash the password
    let hashed = match hash_password(user_create.password.clone().as_str()) {
        Ok(h) => h,
        Err(e) => {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(Json(ResErrorBody {
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
    diesel::insert_into(users::table)
        .values(&new_user)
        .execute(&mut conn)
        .expect("❌ Failed to insert new user");

    // ✅ Respond success
    res.status_code(StatusCode::CREATED);
    res.render(format!(
        "✅ User '{}' created successfully!",
        user_create.email
    ));
}

#[endpoint(
    tags("Users"),
    summary = "get users information",
    description = "the objective of the endpoints is to get users information"
)]
pub async fn get_users_information(
    res: &mut Response,
    depot: &mut Depot,
    authentication: HeaderParam<String, true>,
) {
    println!("🪪 Authentication header: {}", authentication.as_str());

    // ✅ Get DB connection
    let pool = depot.obtain::<Arc<DbPool>>().unwrap();
    let mut _conn = pool.get().expect("❌ Failed to get DB connection");

    println!("📥 Fetching user information...");

    let current_user = depot.get::<User>("user").unwrap();

    println!("👤 Current user: {:?}", current_user);

    // ✅ Build response model
    let user_response_model = ResUserBody {
        id: current_user.id,
        email: current_user.username.clone(), // or change field if you have separate email
        full_name: current_user.full_name.clone(),
        created_at: current_user.created_at,
        updated_at: current_user.updated_at,
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
fn update_users(
    user_id: PathParam<Uuid>,
    res: &mut Response,
    authentication: HeaderParam<String, true>,
    depot: &mut Depot,
    user_update: JsonBody<UserUpdate>,
) {
    println!("🪪 Authentication header: {}", authentication.as_str());

    let user_uuid = user_id.into_inner();

    let pool = depot.obtain::<Arc<DbPool>>().unwrap();
    let mut conn = pool.get().expect("Failed to get DB connection");

    let current_user = depot.get::<User>("user").unwrap();

    // ✅ Check permission (user can only update their own info)
    if current_user.id != user_uuid {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(Json(ResErrorBody {
            detail: format!("❌ You cannot update the user with id: {}", user_uuid),
        }));
        return;
    }
    println!("👤 Current user: {:?}", current_user);

    let update_data = user_update.into_inner();

    let result = diesel::update(users::table.find(user_uuid))
        .set((
            users::full_name.eq(&update_data.fullname),
            users::updated_at.eq(&Utc::now().naive_utc()),
        ))
        .execute(&mut conn);

    match result {
        Ok(affected_rows) => {
            if affected_rows == 0 {
                res.status_code(StatusCode::NOT_FOUND);
                res.render(Json(ResErrorBody {
                    detail: format!("⚠️ No user found with id {}", user_uuid),
                }));
                return;
            } else {
                res.status_code(StatusCode::OK);
                res.render(Json(ResUserBody {
                    id: current_user.id,
                    email: current_user.username.clone(),
                    full_name: update_data.fullname.clone(),
                    created_at: current_user.created_at,
                    updated_at: current_user.updated_at,
                }));
                return;
            }
        }
        Err(e) => {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(Json(ResErrorBody {
                detail: format!("❌ Failed to update user: {}", e),
            }));
            return;
        }
    }
}

#[endpoint(
    tags("Users"),
    summary = "Delete Users Information",
    description = "the objective of this endpoints is to delete users information"
)]
fn delete_users(
    user_id: PathParam<Uuid>,
    res: &mut Response,
    authentication: HeaderParam<String, true>,
    depot: &mut Depot,
) {
    println!("🪪 Authentication header: {}", authentication.as_str());

    let pool = depot.obtain::<Arc<DbPool>>().unwrap();
    let mut conn = pool.get().expect("Failed to get DB connection");

    let current_user = depot.get::<User>("user").unwrap();

    let user_uuid = user_id.into_inner();

    // ✅ Check permission (user can only update their own info)
    if current_user.id != user_uuid {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(Json(ResErrorBody {
            detail: format!("❌ You cannot delete the user with id: {}", user_uuid),
        }));
        return;
    }
    println!("👤 Current user: {:?}", current_user);

    let affected = diesel::delete(users::table.filter(users::id.eq(user_uuid))).execute(&mut conn);

    match affected {
        Ok(0) => {
            res.status_code(StatusCode::NOT_FOUND);
            res.render(Json(ResErrorBody {
                detail: "❌ User not found".to_owned(),
            }));
        }
        Ok(affected_row) => {
            println!("affected row: {}", affected_row);
            res.status_code(StatusCode::OK);
            res.render(Json(ResUserBody {
                id: current_user.id,
                email: current_user.username.clone(),
                full_name: current_user.full_name.clone(),
                created_at: current_user.created_at,
                updated_at: current_user.updated_at,
            }));
        }
        Err(err) => {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(Json(ResErrorBody {
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
fn get_posts_by_users(
    user_id: PathParam<Uuid>,
    res: &mut Response,
    authentication: HeaderParam<String, true>,
    depot: &mut Depot,
) {
    println!("🪪 Authentication header: {}", authentication.as_str());

    let pool = depot.obtain::<Arc<DbPool>>().unwrap();
    let mut conn = pool.get().expect("Failed to get DB connection");

    let current_user = depot.get::<User>("user").unwrap();

    let user_uuid = user_id.into_inner();

    // ✅ Check permission (user can only update their own info)
    if current_user.id != user_uuid {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(Json(ResErrorBody {
            detail: format!(
                "❌ You cannot get all post of the user with id: {}",
                user_uuid
            ),
        }));
        return;
    }
    println!("👤 Current user: {:?}", current_user);

    let all_posts = posts::table
        .filter(posts::user_id.eq(&current_user.id))
        .load::<Post>(&mut conn)
        .expect("Failed to get all posts of the user");

    res.status_code(StatusCode::OK);
    res.render(Json(all_posts));
}

#[endpoint(
    tags("Users"),
    summary = "get access token for login",
    description = "The objective of this endpoint is to get access token of the given users"
)]
fn get_access_token(
    res: &mut Response,
    user_credentiel: JsonBody<UserCredentiel>,
    depot: &mut Depot,
) {
    let pool = depot.obtain::<Arc<DbPool>>().unwrap();
    let mut conn = pool.get().expect("Failed to get DB connection");

    // ✅ Query user by ID
    let existing_user = users::table
        .filter(users::username.eq(&user_credentiel.username))
        .first::<User>(&mut conn)
        .optional()
        .expect("❌ Failed to query user");

    // ✅ Handle "not found" case
    let Some(user) = existing_user else {
        eprintln!("no existing users");
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(Json(ResErrorBody {
            detail: "🚫 Invalid username or password".to_owned(),
        }));
        return;
    };

    if !verify_password(
        user_credentiel.password.clone().as_str(),
        user.password.clone().as_str(),
    ) {
        println!("bad password");
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(Json(ResErrorBody {
            detail: "🚫 Invalid username or password".to_owned(),
        }));
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
    )
    .expect("failed to encode jwt token");

    res.status_code(StatusCode::OK);
    res.render(Json(ResTokenBody {
        token_type: "Bearer".to_owned(),
        token,
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
                .push(Router::with_path("posts").get(get_posts_by_users)),
        )
        .push(Router::with_path("").hoop(auth_user).get(get_all_users))
}
