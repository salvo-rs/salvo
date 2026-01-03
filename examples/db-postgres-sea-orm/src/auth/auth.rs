use salvo::prelude::*;
use salvo_oapi::extract::HeaderParam;
use salvo_oapi::endpoint;
use jsonwebtoken::{decode, DecodingKey, Validation};
use sea_orm::{EntityTrait, QueryFilter};
use crate::{
    database::db::DbPool, models::users, schemas::{ErrorResponseModel, JwtClaims}, utils::SECRET_KEY
};
use std::sync::Arc;
use time::OffsetDateTime;
use crate::models::users::Entity as Users;
use crate::models::users::Column as UsersColumn;
use sea_orm::ColumnTrait;


#[endpoint]
pub async fn auth_user(res: &mut Response, depot: &mut Depot, ctrl: &mut FlowCtrl, authentication: HeaderParam<String, true>,) {
    println!("🔐 Call Authentication");

    // ✅ Get DB connection
    let connection = depot.obtain::<Arc<DbPool>>().unwrap();

    let db = &**connection;


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
    let existing_user: Option<users::Model> = Users::find()
        .filter(UsersColumn::Username.eq(claims.username.clone()))
        .one(db)
        .await
        .expect("Error loading users");


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
