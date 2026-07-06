use std::sync::Arc;

use diesel::prelude::*;
use jsonwebtoken::{DecodingKey, Validation, decode};
use salvo::prelude::*;
use salvo_oapi::endpoint;
use salvo_oapi::extract::HeaderParam;
use time::OffsetDateTime;

use crate::db::DbPool;
use crate::models::users::User;
use crate::models::{JwtClaims, ResErrorBody};
use crate::schema::*;
use crate::utils::SECRET_KEY;

#[endpoint]
pub fn auth_user(
    res: &mut Response,
    depot: &mut Depot,
    ctrl: &mut FlowCtrl,
    authentication: HeaderParam<String, true>,
) {
    println!("🔐 Call Authentication");

    // ✅ Get DB connection
    let pool = depot.get_typed::<Arc<DbPool>>().unwrap();
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
            res.render(Json(ResErrorBody {
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
        res.render(Json(ResErrorBody {
            detail: String::from("Invalid or expired token"),
        }));
        ctrl.skip_rest();
        return;
    }

    // ✅ Token valid — continue
    let claims = decoded.claims;
    println!("✅ Authenticated user: {}", claims.username);

    // ✅ Query user by username from the database
    let existing_user = users::table
        .filter(users::username.eq(&claims.username))
        .first::<User>(&mut conn)
        .optional()
        .expect("❌ Failed to query user");

    if let Some(user) = existing_user {
        println!("👤 User found: {:?}", user);
        depot.insert("user", user);
    } else {
        res.status_code(StatusCode::UNAUTHORIZED);
        res.render(Json(ResErrorBody {
            detail: format!("🚫 User '{}' not found", claims.username),
        }));
        ctrl.skip_rest();
        return;
    }
}
