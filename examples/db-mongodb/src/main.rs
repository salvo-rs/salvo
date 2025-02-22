use std::sync::OnceLock;

use futures::stream::TryStreamExt;
use mongodb::{
    Client, Collection, IndexModel, bson::Document, bson::doc, bson::oid::ObjectId,
    options::IndexOptions,
};
use salvo::prelude::*;
use serde::{Deserialize, Serialize};

// Database and collection names
const DB_NAME: &str = "myApp";
const COLL_NAME: &str = "users";

use thiserror::Error;

// Custom error type for MongoDB operations
#[derive(Error, Debug)]
pub enum Error {
    #[error("MongoDB Error")]
    ErrorMongo(#[from] mongodb::error::Error),
}

pub type AppResult<T> = Result<T, Error>;

// Implement error response writer for our custom error type
#[async_trait]
impl Writer for Error {
    async fn write(self, _req: &mut Request, _depot: &mut Depot, _res: &mut Response) {}
}

// User model representing the document structure in MongoDB
#[derive(Debug, Deserialize, Serialize)]
struct User {
    _id: Option<ObjectId>,
    first_name: String,
    last_name: String,
    username: String,
    email: String,
}

// Global MongoDB client instance
static MONGODB_CLIENT: OnceLock<Client> = OnceLock::new();

// Helper function to get the MongoDB client instance
#[inline]
pub fn get_mongodb_client() -> &'static Client {
    MONGODB_CLIENT.get().unwrap()
}

// Handler for adding a new user to the database
#[handler]
async fn add_user(req: &mut Request, res: &mut Response) {
    let client = get_mongodb_client();
    let coll_users = client.database(DB_NAME).collection::<Document>(COLL_NAME);
    let new_user = req.parse_json::<User>().await.unwrap();

    // Create BSON document from user data
    let user = doc! {
        "first_name": new_user.first_name,
        "last_name": new_user.last_name,
        "username": new_user.username,
        "email": new_user.email,
    };

    // Insert user document into MongoDB
    let result = coll_users.insert_one(user, None).await;
    match result {
        Ok(id) => res.render(format!("user added with ID {:?}", id.inserted_id)),
        Err(e) => res.render(format!("error {e:?}")),
    }
}

// Handler for retrieving all users from the database
#[handler]
async fn get_users(res: &mut Response) -> AppResult<()> {
    let client = get_mongodb_client();
    let coll_users = client.database(DB_NAME).collection::<User>(COLL_NAME);
    // Find all users and convert cursor to vector
    let mut cursor = coll_users.find(None, None).await?;
    let mut vec_users: Vec<User> = Vec::new();
    while let Some(user) = cursor.try_next().await? {
        vec_users.push(user);
    }
    res.render(Json(vec_users));
    Ok(())
}

// Handler for retrieving a single user by username
#[handler]
async fn get_user(req: &mut Request, res: &mut Response) {
    let client = get_mongodb_client();
    let coll_users: Collection<User> = client.database(DB_NAME).collection(COLL_NAME);

    let username = req.param::<String>("username").unwrap();
    // Find user by username
    match coll_users
        .find_one(doc! { "username": &username }, None)
        .await
    {
        Ok(Some(user)) => res.render(Json(user)),
        Ok(None) => res.render(format!("No user found with username {username}")),
        Err(e) => res.render(format!("error {e:?}")),
    }
}

// Create a unique index on the username field
async fn create_username_index(client: &Client) {
    let options = IndexOptions::builder().unique(true).build();
    let model = IndexModel::builder()
        .keys(doc! { "username": 1 })
        .options(options)
        .build();
    client
        .database(DB_NAME)
        .collection::<User>(COLL_NAME)
        .create_index(model, None)
        .await
        .expect("creating an index should succeed");
}

#[tokio::main]
async fn main() {
    // Initialize logging system
    tracing_subscriber::fmt().init();

    // Get MongoDB connection URI from environment or use default
    let mongodb_uri =
        std::env::var("MONGODB_URI").unwrap_or_else(|_| "mongodb://10.1.1.80:27017".into());

    // Connect to MongoDB and create unique index
    let client = Client::with_uri_str(mongodb_uri)
        .await
        .expect("failed to connect");
    create_username_index(&client).await;

    // Store MongoDB client in global state
    MONGODB_CLIENT.set(client).unwrap();

    // Configure router with user management endpoints:
    // - GET /users : List all users
    // - POST /users : Create new user
    // - GET /users/{username} : Get user by username
    let router = Router::with_path("users")
        .get(get_users)
        .post(add_user)
        .push(Router::with_path("{username}").get(get_user));

    // Start server on port 5800
    let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
    Server::new(acceptor).serve(router).await;
}
