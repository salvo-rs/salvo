use juniper::{FieldError, FieldResult, graphql_object};

use crate::schema::{DatabaseContext, User};

// Root type for all GraphQL queries
pub struct QueryRoot;

// Implement GraphQL query resolvers
#[graphql_object(context = DatabaseContext)]
impl QueryRoot {
    // Query to get all users from the database
    fn get_all_users(context: &DatabaseContext) -> FieldResult<Vec<User>> {
        let read = context.0.read();
        let users = read.get_all_users();
        let mut result = Vec::with_capacity(users.len());
        result.reserve(users.len());
        for user in users {
            result.push(User {
                id: user.id,
                name: user.name.clone(),
            })
        }
        Ok(result)
    }

    // Query to get a specific user by ID
    fn get_user_by_id(context: &DatabaseContext, id: i32) -> FieldResult<User> {
        let read = context.0.read();
        let user = read.get_user_by_id(&id);
        match user {
            Some(user) => Ok(User {
                id: user.id,
                name: user.name.clone(),
            }),
            None => Err(FieldError::from("could not find the user")),
        }
    }
}
