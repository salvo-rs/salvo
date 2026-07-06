use std::sync::Arc;

use chrono::Utc;
use salvo::prelude::*;
use salvo_oapi::endpoint;
use salvo_oapi::extract::{HeaderParam, JsonBody, PathParam};
use sea_orm::ActiveValue::Set;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, ModelTrait, QueryFilter};
use uuid::Uuid;

use crate::auth::auth_user;
use crate::db::DbPool;
use crate::models::posts::{self, Column as PostColumn, Entity as Posts};
use crate::models::users::{self};
use crate::schemas::ErrorResponseModel;
use crate::schemas::posts::PostCreate;

#[endpoint(
    tags("Posts"),
    summary = "get all posts",
    description = "the objective of this endpoint is to retrieve all create post of given the current user"
)]
async fn get_all_posts(
    res: &mut Response,
    authentication: HeaderParam<String, true>,
    depot: &mut Depot,
) {
    println!("🪪 Authentication header: {}", authentication.as_str());

    // ✅ Get DB connection
    let connection = depot.get_typed::<Arc<DbPool>>().unwrap();
    let db = &**connection;

    let current_user: &users::Model = depot.get::<users::Model>("user").unwrap();

    println!("👤 Current user: {:?}", current_user);

    let all_posts: Vec<posts::Model> = Posts::find()
        .filter(PostColumn::UserId.eq(current_user.id))
        .all(db)
        .await
        .expect("failed to load all posts");

    res.status_code(StatusCode::OK);
    res.render(Json(all_posts));
}

#[endpoint(
    tags("Posts"),
    summary = "create posts",
    description = " the objective of this endpoint is to create a post"
)]
async fn create_posts(
    res: &mut Response,
    post_create: JsonBody<PostCreate>,
    depot: &mut Depot,
    authentication: HeaderParam<String, true>,
) {
    println!("🪪 Authentication header: {}", authentication.as_str());

    // ✅ Get DB connection
    let connection = depot.get_typed::<Arc<DbPool>>().unwrap();
    let db = &**connection;

    let current_user: &users::Model = depot.get::<users::Model>("user").unwrap();

    println!("👤 Current user: {:?}", current_user);

    let now = Utc::now().naive_utc();

    // ✅ Create new post
    let new_post = posts::ActiveModel {
        id: Set(Uuid::new_v4()),
        content: Set(post_create.content.clone()),
        title: Set(post_create.title.clone()),
        user_id: Set(current_user.id),
        created_at: Set(now),
        updated_at: Set(now),
    };

    // ✅ Insert into DB
    let post = Posts::insert(new_post)
        .exec(db)
        .await
        .expect("❌ Failed to insert new post");

    println!("The last inserted id is: {}", post.last_insert_id);

    res.status_code(StatusCode::OK);
    // res.render(Json(new_post));
    res.render(format!(
        "✅ Post '{}' created successfully!",
        post.last_insert_id
    ));
}

#[endpoint(
    tags("Posts"),
    summary = "update posts",
    description = "update a specific post by id"
)]
async fn update_posts(
    post_id: PathParam<Uuid>,
    res: &mut Response,
    post_update: JsonBody<PostCreate>,
    depot: &mut Depot,
    authentication: HeaderParam<String, true>,
) {
    println!("🪪 Authentication header: {}", authentication.as_str());

    // ✅ Get DB connection
    let connection = depot.get_typed::<Arc<DbPool>>().unwrap();
    let db = &**connection;
    let current_user: &users::Model = depot.get::<users::Model>("user").unwrap();

    println!("👤 Current user: {:?}", current_user);

    let post_uuid = post_id.into_inner();

    let existing_post: Option<posts::Model> = Posts::find()
        .filter(PostColumn::Id.eq(post_uuid))
        .one(db)
        .await
        .expect("❌ Failed to query post");

    let Some(post) = existing_post else {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(Json(ErrorResponseModel {
            detail: format!("The post with id: {} don't exits in database", {
                post_uuid
            }),
        }));
        return;
    };

    // ✅ Check permission (user can only update their own info)
    if post.user_id != current_user.id {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(Json(ErrorResponseModel {
            detail: "You can delete the post that you don't create".to_owned(),
        }));
        return;
    }

    let update_data = post_update.into_inner();

    let mut post: posts::ActiveModel = post.into();

    post.content = Set(update_data.content.clone());
    post.title = Set(update_data.title.clone());
    post.updated_at = Set(Utc::now().naive_utc());

    let post = post.update(db).await;
    if post.is_err() {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(Json(ErrorResponseModel {
            detail: "❌ Failed to update post".to_owned(),
        }));
    } else {
        let existing_post: Option<posts::Model> = Posts::find()
            .filter(PostColumn::Id.eq(post_uuid))
            .one(db)
            .await
            .expect("❌ Failed to query post");
        res.status_code(StatusCode::OK);
        res.render(Json(existing_post.unwrap()));
    }

    // println!("The number of row affected by this update is : {}", row_affcted);
}

#[endpoint(
    tags("Posts"),
    summary = "delete posts",
    description = "delete a specific post by id"
)]
async fn delete_posts(
    post_id: PathParam<Uuid>,
    res: &mut Response,
    depot: &mut Depot,
    authentication: HeaderParam<String, true>,
) {
    println!("🪪 Authentication header: {}", authentication.as_str());

    // ✅ Get DB connection
    let connection = depot.get_typed::<Arc<DbPool>>().unwrap();
    let db = &**connection;

    let current_user: &users::Model = depot.get::<users::Model>("user").unwrap();

    println!("👤 Current user: {:?}", current_user);

    let post_uuid = post_id.into_inner();

    let existing_post: Option<posts::Model> = Posts::find()
        .filter(PostColumn::Id.eq(post_uuid))
        .one(db)
        .await
        .expect("❌ Failed to query post");

    let Some(post) = existing_post else {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(Json(ErrorResponseModel {
            detail: format!("The post with id: {} don't exits in database", {
                post_uuid
            }),
        }));
        return;
    };

    if post.user_id != current_user.id {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(Json(ErrorResponseModel {
            detail: "You can delete the post that you don't create".to_owned(),
        }));
        return;
    }

    let post_delete = post.clone().delete(db).await;

    if post_delete.is_err() {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render("Error during the delete post".to_owned());
    } else {
        let post_result = post_delete.unwrap();
        println!(
            "The number of row affected is {}",
            post_result.rows_affected
        );
        res.status_code(StatusCode::OK);
        res.render(Json(post));
        return;
    }
}

#[endpoint(
    tags("Posts"),
    summary = "get posts information",
    description = "get a specific post by id"
)]
async fn get_posts_information(
    post_id: PathParam<Uuid>,
    res: &mut Response,
    depot: &mut Depot,
    authentication: HeaderParam<String, true>,
) {
    println!("🪪 Authentication header: {}", authentication.as_str());

    // ✅ Get DB connection
    let connection = depot.get_typed::<Arc<DbPool>>().unwrap();
    let db = &**connection;

    let current_user: &users::Model = depot.get::<users::Model>("user").unwrap();

    println!("👤 Current user: {:?}", current_user);

    let post_uuid = post_id.into_inner();

    let existing_post: Option<posts::Model> = Posts::find()
        .filter(PostColumn::Id.eq(post_uuid))
        .one(db)
        .await
        .expect("❌ Failed to query user");

    let Some(post) = existing_post else {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(Json(ErrorResponseModel {
            detail: format!("The post with id: {} don't exits in databe", { post_uuid }),
        }));
        return;
    };

    if post.user_id != current_user.id {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(Json(ErrorResponseModel {
            detail: "You can delete the post that you don't create".to_owned(),
        }));
        return;
    }

    res.render(Json(post));
}

pub fn get_posts_router() -> Router {
    Router::with_path("/posts")
        .hoop(auth_user)
        .get(get_all_posts)
        .post(create_posts)
        .push(
            Router::with_path("{post_id}")
                .get(get_posts_information)
                .put(update_posts)
                .delete(delete_posts),
        )
}
