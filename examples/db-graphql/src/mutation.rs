use juniper::{FieldResult, graphql_object};

use crate::schema::{DatabaseContext, User, UserInput};

// Root type for all GraphQL mutations
pub struct MutationRoot;

// Implement GraphQL mutation resolvers
#[graphql_object(context = DatabaseContext)]
impl MutationRoot {
    // Mutation to create a new user
    // Note: database needs RwLock for thread-safe writes
    fn create_user(context: &DatabaseContext, user: UserInput) -> FieldResult<User> {
        let mut write = context.0.write();
        let user = User {
            id: user.id,
            name: user.name,
        };
        let user_to_return = user.clone();
        write.insert(user);
        Ok(user_to_return)
    }
}
