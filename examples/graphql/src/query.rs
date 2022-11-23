use juniper::{graphql_object, FieldError, FieldResult};

use crate::schema::{DatabaseContext, User};

pub struct QueryRoot;

#[graphql_object(context = DatabaseContext)]
impl QueryRoot {
    fn get_all_users(context: &DatabaseContext) -> FieldResult<Vec<User>> {
        let read = context.0.read().expect("could not access the database");
        let users = read.get_all_users();
        let mut result = Vec::<User>::new();
        result.reserve(users.len());
        for user in users {
            result.push(User {
                id: user.id,
                name: user.name.clone(),
            })
        }
        Ok(result)
    }
    fn get_user_by_id(context: &DatabaseContext, id: i32) -> FieldResult<User> {
        let read = context.0.read().expect("could not access the database");
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
