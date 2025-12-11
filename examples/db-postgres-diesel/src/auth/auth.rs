use salvo::prelude::*;
use salvo_oapi::extract::HeaderParam;
use salvo_oapi::endpoint;
use jsonwebtoken::{decode, DecodingKey, Validation};
use crate::{
    database::db::DbPool, models::{schema::users::username, users::Users}, schemas::{ErrorResponseModel, JwtClaims}, utils::SECRET_KEY
};
use std::sync::Arc;
use time::OffsetDateTime;
use diesel::prelude::*;
use crate::models::schema::users::dsl::users;

#[endpoint]
pub fn auth_user(res: &mut Response, depot: &mut Depot, ctrl: &mut FlowCtrl, authentication: HeaderParam<String, true>,) {
    println!("🔐 Call Authentication");

    // ✅ Get DB connection
    let pool = depot.obtain::<Arc<DbPool>>().unwrap();
    let mut conn = pool.get().expect("❌ Failed to get DB connection");


    // ✅ Decode the JWT
    let decoded = match decode::<JwtClaims>(
        authentication.clone(),
        &DecodingKey::from_secret(SECRET_KEY.as_ref()),
        &Validation::default(),
    ) {
        Ok(data) => data,
        Err(err) => {
            eprintln!("❌ Invalid token: {:?}", err);
            res.status_code(StatusCode::UNAUTHORIZED);
            res.render(Json(ErrorResponseModel {
                detail: String::from("Invalid or malformed token"),
            }));
            ctrl.skip_rest();
            return;
        }
    };

    // ✅ Check token expiration
    let current_timestamp = OffsetDateTime::now_utc().unix_timestamp();
    if decoded.claims.exp < current_timestamp {
        println!("⏰ Token expired at {}", decoded.claims.exp);
        res.status_code(StatusCode::UNAUTHORIZED);
        res.render(Json(
            ErrorResponseModel {
            detail: String::from("Invalid or expired token"),
        }));
        ctrl.skip_rest();
        return;
    }

    // ✅ Token valid — continue
    let claims = decoded.claims;
    println!("✅ Authenticated user: {}", claims.username);

    // ✅ Query user by username from the database
    let existing_user = users
        .filter(username.eq(&claims.username))
        .first::<Users>(&mut conn)
        .optional()
        .expect("❌ Failed to query user");


    if let Some(user) = existing_user {
            println!("👤 User found: {:?}", user);
            depot.insert("user", user);
        
    } else {
        res.status_code(StatusCode::UNAUTHORIZED);
            res.render(Json(
                ErrorResponseModel {
                detail: format!("🚫 User '{}' not found", claims.username),
            }));
            ctrl.skip_rest();
            return;

    }
}
