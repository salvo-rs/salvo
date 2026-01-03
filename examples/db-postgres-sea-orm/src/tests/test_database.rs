#[cfg(test)]
mod tests {
    use crate::database::db::*;
    use tokio; 

    #[tokio::test]
    async fn test_database_connection() {
        let db = establish_connection_pool().await;
        check(db).await;
    }
}