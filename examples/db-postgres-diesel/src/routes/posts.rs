use std::sync::Arc;
use chrono::Utc;
use salvo::prelude::*;
use diesel::prelude::*;
use salvo_oapi::{
    endpoint,
    extract::{HeaderParam, JsonBody, PathParam},
};
use crate::{models::{posts::NewPost, schema::posts::{content, dsl::posts, title, updated_at}}, schemas::ErrorResponseModel};
use crate::{auth::auth::auth_user, database::db::DbPool, models::{posts::Posts, users::Users}};
use crate::schemas::posts::PostCreate;
use uuid::Uuid;
use crate::models::schema::posts::{id, user_id};

#[endpoint(
    tags("Posts"),
    summary = "get all posts",
    description = "the objective of this endpoint is to retrieve all create post of given the current user"
)]
fn get_all_posts(res: &mut Response, authentication: HeaderParam<String, true>, depot: &mut Depot) {

    println!("🪪 Authentication header: {}", authentication.as_str());

    let pool = depot.obtain::<Arc<DbPool>>().unwrap();
    let mut conn = pool.get().expect("Failed to get DB connection");


    let current_user: &Users  = depot.get::<Users>("user").unwrap();

    println!("👤 Current user: {:?}", current_user);

    let all_posts = posts
            .filter(user_id.eq(&current_user.id))
            .load::<Posts>(&mut conn)
            .expect("Failed to get all posts of the user");

    res.status_code(StatusCode::OK);
    res.render(Json(all_posts));
}


#[endpoint(
    tags("Posts"),
    summary = "create posts",
    description = " the objective of this endpoint is to create a post"
)]
fn create_posts(res: &mut Response, post_create: JsonBody<PostCreate>, depot: &mut Depot, authentication: HeaderParam<String, true>) {

    println!("🪪 Authentication header: {}", authentication.as_str());

    // ✅ Get DB connection
    let pool = depot.obtain::<Arc<DbPool>>().unwrap();
    let mut conn = pool.get().expect("❌ Failed to get DB connection");

    let current_user = depot.get::<Users>("user").unwrap();

    println!("👤 Current user: {:?}", current_user);

    let now = Utc::now().naive_utc();

    // ✅ Create new post
    let new_post = NewPost {
        id: Uuid::new_v4(),
        content: post_create.content.clone(),
        title: post_create.title.clone(),
        user_id: current_user.id.clone(),
        created_at: now,
        updated_at: now

    };

    // ✅ Insert into DB
    let row_affcted = diesel::insert_into(posts)
            .values(&new_post)
            .execute(&mut conn)
            .expect("❌ Failed to insert new post");

    println!("The number of Row affcted: {}", row_affcted);

    res.status_code(StatusCode::OK);
    res.render(Json(new_post));
}

#[endpoint(
    tags("Posts"),
    summary = "update posts",
    description = "update a specific post by id"
)]
fn update_posts(post_id: PathParam<Uuid>, res: &mut Response, post_update: JsonBody<PostCreate>, depot: &mut Depot, authentication: HeaderParam<String, true>) {

    println!("🪪 Authentication header: {}", authentication.as_str());

    // ✅ Get DB connection
    let pool = depot.obtain::<Arc<DbPool>>().unwrap();
    let mut conn = pool.get().expect("❌ Failed to get DB connection");

    let current_user = depot.get::<Users>("user").unwrap();

    println!("👤 Current user: {:?}", current_user);

    let post_uuid = post_id.into_inner();
    

    let existing_post  = posts
                    .filter(id.eq(&post_uuid))
                    .first::<Posts>(&mut conn)
                    .optional()
                    .expect("❌ Failed to query post");

                
    
    let Some(post) = existing_post else {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(Json(ErrorResponseModel{
            detail: format!("The post with id: {} don't exits in databe", {post_uuid})
        }));
        return  ;
    };

    // ✅ Check permission (user can only update their own info)
    if post.user_id != current_user.id {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(Json(ErrorResponseModel{
            detail: format!("You can delete the post that you don't create")
        }));
        return ;
    }
    
    let update_data = post_update.into_inner();

    let row_affcted = diesel::update(posts.find(post_uuid))
                .set((
                    title.eq(&update_data.title),
                    content.eq(&update_data.content),
                    updated_at.eq(&Utc::now().naive_utc())
                ))
                .execute(&mut conn)
                .expect("Failed to update post information");
    
    println!("The number of row affected by this update is : {}", row_affcted);
    
    let existing_post  = posts
                    .filter(id.eq(&post_uuid))
                    .first::<Posts>(&mut conn)
                    .optional()
                    .expect("❌ Failed to query post");

            
    res.status_code(StatusCode::OK);
    res.render(Json(existing_post));
    


}

#[endpoint(
    tags("Posts"),
    summary = "delete posts",
    description = "delete a specific post by id"
)]
fn delete_posts(post_id: PathParam<Uuid>, res: &mut Response, depot: &mut Depot, authentication: HeaderParam<String, true>) {
   
    println!("🪪 Authentication header: {}", authentication.as_str());

    // ✅ Get DB connection
    let pool = depot.obtain::<Arc<DbPool>>().unwrap();
    let mut conn = pool.get().expect("❌ Failed to get DB connection");

    let current_user = depot.get::<Users>("user").unwrap();

    println!("👤 Current user: {:?}", current_user);

    let post_uuid = post_id.into_inner();

    let existing_post  = posts
                    .filter(id.eq(&post_uuid))
                    .first::<Posts>(&mut conn)
                    .optional()
                    .expect("❌ Failed to query post");

                
    
    let Some(post) = existing_post else {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(Json(ErrorResponseModel{
            detail: format!("The post with id: {} don't exits in database", {post_uuid})
        }));
        return  ;
    };

    if post.user_id != current_user.id {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(Json(ErrorResponseModel{
            detail: format!("You can delete the post that you don't create")
        }));
        return ;
    }

    let row_affected = diesel::delete(posts
                .filter(id.eq(post_uuid)))
                .execute(&mut conn)
                .expect(format!("Failed to delete posts with id: {}", {post_uuid}).as_str());

    println!("The number of row affected is {}", {row_affected});
    res.render(Json(post));
}

#[endpoint(
    tags("Posts"),
    summary = "get posts information",
    description = "get a specific post by id"
)]
fn get_posts_information(post_id: PathParam<Uuid>, res: &mut Response, depot: &mut Depot, authentication: HeaderParam<String, true>) {
   println!("🪪 Authentication header: {}", authentication.as_str());

    // ✅ Get DB connection
    let pool = depot.obtain::<Arc<DbPool>>().unwrap();
    let mut conn = pool.get().expect("❌ Failed to get DB connection");

    let current_user = depot.get::<Users>("user").unwrap();

    println!("👤 Current user: {:?}", current_user);

    let post_uuid = post_id.into_inner();


    let existing_post = posts
                .filter(id.eq(&post_uuid))
                .first::<Posts>(&mut conn)
                .optional()
                .expect("❌ Failed to query user");
    
    let Some(post) = existing_post else {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(Json(ErrorResponseModel{
            detail: format!("The post with id: {} don't exits in databe", {post_uuid})
        }));
        return  ;
    };

    if post.user_id != current_user.id {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(Json(ErrorResponseModel{
            detail: format!("You can delete the post that you don't create")
        }));
        return ;
    }

    res.render(Json(post));
}

pub fn get_posts_router() -> Router {
    let posts_router = Router::with_path("/posts")
        .hoop(auth_user)
        .get(get_all_posts)
        .post(create_posts)
        .push(
            Router::with_path("{post_id}")
                .get(get_posts_information)
                .put(update_posts)
                .delete(delete_posts),
        );
    posts_router
}
