use parking_lot::RwLock;
use std::collections::HashMap;
use std::default::Default;

use juniper::{EmptySubscription, GraphQLInputObject, GraphQLObject, RootNode};

use crate::{mutation::MutationRoot, query::QueryRoot};

// Define the GraphQL schema type with Query and Mutation roots
pub type Schema = RootNode<'static, QueryRoot, MutationRoot, EmptySubscription<DatabaseContext>>;

// Create and initialize the GraphQL schema
pub fn create_schema() -> Schema {
    Schema::new(
        QueryRoot {},
        MutationRoot {},
        EmptySubscription::<DatabaseContext>::default(),
    )
}

// User model that can be returned in GraphQL responses
#[derive(GraphQLObject, Clone)]
pub struct User {
    pub id: i32,
    pub name: String,
}

// Input type for creating new users through GraphQL mutations
#[derive(GraphQLInputObject)]
pub struct UserInput {
    pub id: i32,
    pub name: String,
}

// In-memory database structure to store users
pub struct Database {
    users: HashMap<i32, User>,
}

// Database context wrapper with thread-safe read/write access
pub struct DatabaseContext(pub RwLock<Database>);

// Default implementation for DatabaseContext
impl Default for DatabaseContext {
    fn default() -> DatabaseContext {
        DatabaseContext::new()
    }
}

// DatabaseContext implementation with initialization
impl DatabaseContext {
    pub fn new() -> Self {
        DatabaseContext(RwLock::<Database>::new(Database::new()))
    }
}

// Default implementation for Database
impl Default for Database {
    fn default() -> Self {
        Self::new()
    }
}

// Database implementation with CRUD operations
impl Database {
    // Initialize database with some sample users
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

    // Get all users from the database
    pub fn get_all_users(&self) -> Vec<&User> {
        Vec::from_iter(self.users.values())
    }

    // Get a specific user by ID
    pub fn get_user_by_id(&self, id: &i32) -> Option<&User> {
        self.users.get(id)
    }

    // Insert a new user into the database
    pub fn insert(&mut self, user: User) {
        self.users.insert(user.id, user);
    }
}
