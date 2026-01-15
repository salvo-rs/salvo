#[cfg(test)]
mod tests {
    use tokio;

    use crate::db::*;

    #[tokio::test]
    async fn test_database_connection() {
        let db = establish_connection_pool().await;
        check(db).await;
    }
}
