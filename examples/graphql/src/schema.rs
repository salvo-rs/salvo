use std::collections::HashMap;
use std::default::Default;
use std::sync::RwLock;

use juniper::{EmptySubscription, GraphQLInputObject, GraphQLObject, RootNode};

use crate::{mutation::MutationRoot, query::QueryRoot};

pub type Schema = RootNode<'static, QueryRoot, MutationRoot, EmptySubscription<DatabaseContext>>;

pub fn create_schema() -> Schema {
    Schema::new(
        QueryRoot {},
        MutationRoot {},
        EmptySubscription::<DatabaseContext>::default(),
    )
}
#[derive(GraphQLObject, Clone)]
pub struct User {
    pub id: i32,
    pub name: String,
}

#[derive(GraphQLInputObject)]
pub struct UserInput {
    pub id: i32,
    pub name: String,
}

pub struct Database {
    users: HashMap<i32, User>,
}

pub struct DatabaseContext(pub RwLock<Database>);

impl Default for DatabaseContext {
    fn default() -> DatabaseContext {
        DatabaseContext::new()
    }
}
impl DatabaseContext {
    pub fn new() -> Self {
        DatabaseContext(RwLock::<Database>::new(Database::new()))
    }
}

impl Default for Database {
    fn default() -> Self {
        Self::new()
    }
}

impl Database {
    pub fn new() -> Self {
        let mut users = HashMap::new();
        users.insert(
            0,
            User {
                id: 0,
                name: String::from("Alice"),
            },
        );
        users.insert(
            1,
            User {
                id: 1,
                name: String::from("Bob"),
            },
        );
        Database { users }
    }
    pub fn get_all_users(&self) -> Vec<&User> {
        Vec::from_iter(self.users.values())
    }
    pub fn get_user_by_id(&self, id: &i32) -> Option<&User> {
        self.users.get(id)
    }
    pub fn insert(&mut self, user: User) {
        self.users.insert(user.id, user);
    }
}
