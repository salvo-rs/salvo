use juniper::{graphql_object, FieldResult};

use crate::schema::{DatabaseContext, User, UserInput};

pub struct MutationRoot;

#[graphql_object(context = DatabaseContext)]
impl MutationRoot {
    // here database canot be mutable, we need RwLock
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
