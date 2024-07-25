use std::sync::OnceLock;

use futures::stream::TryStreamExt;
use mongodb::{bson::doc, bson::oid::ObjectId, bson::Document, options::IndexOptions, Client, Collection, IndexModel};
use salvo::prelude::*;
use serde::{Deserialize, Serialize};

const DB_NAME: &str = "myApp";
const COLL_NAME: &str = "users";

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("MongoDB Error")]
    ErrorMongo(#[from] mongodb::error::Error),
}

pub type AppResult<T> = Result<T, Error>;

#[async_trait]
impl Writer for Error {
    async fn write(self, _req: &mut Request, _depot: &mut Depot, _res: &mut Response) {}
}

#[derive(Debug, Deserialize, Serialize)]
struct User {
    _id: Option<ObjectId>,
    first_name: String,
    last_name: String,
    username: String,
    email: String,
}

static MONGODB_CLIENT: OnceLock<Client> = OnceLock::new();

#[inline]
pub fn get_mongodb_client() -> &'static Client {
    MONGODB_CLIENT.get().unwrap()
}

#[handler]
async fn add_user(req: &mut Request, res: &mut Response) {
    let client = get_mongodb_client();
    let coll_users = client.database(DB_NAME).collection::<Document>(COLL_NAME);
    let new_user = req.parse_json::<User>().await.unwrap();

    let user = doc! {
        "first_name": new_user.first_name,
        "last_name": new_user.last_name,
        "username": new_user.username,
        "email": new_user.email,
    };

    let result = coll_users.insert_one(user, None).await;
    match result {
        Ok(id) => res.render(format!("user added with ID {:?}", id.inserted_id)),
        Err(e) => res.render(format!("error {e:?}")),
    }
}

#[handler]
async fn get_users(res: &mut Response) -> AppResult<()> {
    let client = get_mongodb_client();
    let coll_users = client.database(DB_NAME).collection::<User>(COLL_NAME);
    let mut cursor = coll_users.find(None, None).await?;
    let mut vec_users: Vec<User> = Vec::new();
    while let Some(user) = cursor.try_next().await? {
        vec_users.push(user);
    }
    res.render(Json(vec_users));
    Ok(())
}

#[handler]
async fn get_user(req: &mut Request, res: &mut Response) {
    let client = get_mongodb_client();
    let coll_users: Collection<User> = client.database(DB_NAME).collection(COLL_NAME);

    let username = req.param::<String>("username").unwrap();
    match coll_users.find_one(doc! { "username": &username }, None).await {
        Ok(Some(user)) => res.render(Json(user)),
        Ok(None) => res.render(format!("No user found with username {username}")),
        Err(e) => res.render(format!("error {e:?}")),
    }
}

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
    tracing_subscriber::fmt().init();

    let mongodb_uri = std::env::var("MONGODB_URI").unwrap_or_else(|_| "mongodb://10.1.1.80:27017".into());
    let client = Client::with_uri_str(mongodb_uri).await.expect("failed to connect");
    create_username_index(&client).await;

    MONGODB_CLIENT.set(client).unwrap();

    // router
    let router = Router::with_path("users")
        .get(get_users)
        .post(add_user)
        .push(Router::with_path("<username>").get(get_user));

    let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
    Server::new(acceptor).serve(router).await;
}
