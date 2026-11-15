#![warn(missing_docs)]

//! Toasty driver for [PostgreSQL](https://www.postgresql.org/) using
//! [`tokio-postgres`](https://docs.rs/tokio-postgres).
//!
//! # Examples
//!
//! ```no_run
//! use toasty_driver_postgresql::PostgreSQL;
//!
//! let driver = PostgreSQL::new("postgresql://localhost/mydb").unwrap();
//! ```

mod oid_cache;
mod statement_cache;
#[cfg(feature = "tls")]
mod tls;
mod r#type;
mod value;

pub(crate) use value::Value;

use async_trait::async_trait;
use percent_encoding::percent_decode_str;
use std::{borrow::Cow, sync::Arc};
use toasty_core::{
    Result, Schema,
    driver::{
        Capability, ConnectContext, Driver, ExecResponse, Operation, QueryLogConfig,
        log::QueryLog,
        operation::{RawSqlRet, Transaction, TransactionMode, TypedValue},
    },
    schema::{
        db::{self, Migration, Table},
        diff,
    },
    stmt,
    stmt::ValueRecord,
};
use toasty_sql::{self as sql};
use tokio_postgres::{Client, Config, Socket, tls::MakeTlsConnect, types::ToSql};
use url::Url;

enum SqlReturn {
    Count,
    Infer,
    Types(Vec<stmt::Type>),
}

use crate::{oid_cache::OidCache, statement_cache::StatementCache};

/// Classifies a `tokio_postgres::Error` into a Toasty error.
///
/// Errors that carry a server-side `DbError` are mapped to typed
/// variants where one exists (`SerializationFailure`,
/// `ReadOnlyTransaction`); everything else with a `DbError` becomes
/// `DriverOperationFailed`. Errors *without* a `DbError` are
/// classified as `ConnectionLost`: per `tokio-postgres`, those
/// originate from the underlying socket or protocol layer (closed
/// socket, IO error, end-of-stream during handshake), which the
/// pool treats as evictable.
fn classify_pg_error(e: tokio_postgres::Error) -> toasty_core::Error {
    if let Some(db_err) = e.as_db_error() {
        match db_err.code().code() {
            "40001" => toasty_core::Error::serialization_failure(db_err.message()),
            "25006" => toasty_core::Error::read_only_transaction(db_err.message()),
            _ => toasty_core::Error::driver_operation_failed(e),
        }
    } else {
        toasty_core::Error::connection_lost(e)
    }
}

/// A PostgreSQL [`Driver`] that connects via `tokio-postgres`.
///
/// # Examples
///
/// ```no_run
/// use toasty_driver_postgresql::PostgreSQL;
///
/// let driver = PostgreSQL::new("postgresql://localhost/mydb").unwrap();
/// ```
pub struct PostgreSQL {
    url: String,
    config: Config,
    #[cfg(feature = "tls")]
    tls: Option<tokio_postgres_rustls::MakeRustlsConnect>,
}

impl std::fmt::Debug for PostgreSQL {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self {
            url,
            config,
            #[cfg(feature = "tls")]
            tls,
        } = self;
        let mut s = f.debug_struct("PostgreSQL");
        s.field("url", url);
        s.field("config", config);
        #[cfg(feature = "tls")]
        s.field("tls", &tls.as_ref().map(|_| "MakeRustlsConnect"));
        s.finish()
    }
}

impl PostgreSQL {
    /// Create a new PostgreSQL driver from a connection URL
    pub fn new(url: impl Into<String>) -> Result<Self> {
        let url_str = url.into();
        let url = Url::parse(&url_str).map_err(toasty_core::Error::driver_operation_failed)?;

        if !matches!(url.scheme(), "postgresql" | "postgres") {
            return Err(toasty_core::Error::invalid_connection_url(format!(
                "connection URL does not have a `postgresql` scheme; url={}",
                url
            )));
        }

        if url.path().is_empty() {
            return Err(toasty_core::Error::invalid_connection_url(format!(
                "no database specified - missing path in connection URL; url={}",
                url
            )));
        }

        let mut config = Config::new();

        let dbname = percent_decode_str(url.path().trim_start_matches('/'))
            .decode_utf8()
            .map_err(|_| {
                toasty_core::Error::invalid_connection_url("database name is not valid UTF-8")
            })?;
        config.dbname(&*dbname);

        if !url.username().is_empty() {
            let user = percent_decode_str(url.username())
                .decode_utf8()
                .map_err(|_| {
                    toasty_core::Error::invalid_connection_url("username is not valid UTF-8")
                })?;
            config.user(&*user);
        }

        if let Some(password) = url.password() {
            config.password(percent_decode_str(password).collect::<Vec<u8>>());
        }

        // libpq lets standard connection parameters appear in the query
        // string; honor the ones a Toasty user can reasonably set so that
        // `postgresql:///mydb?host=/tmp&user=alice` reaches the server.
        // Single-valued setters (user, password, dbname, application_name)
        // replace earlier calls, so we can apply them inline; host and
        // port are list-valued — staged into Options below so a query
        // parameter cleanly overrides the URL component instead of being
        // appended as a fallback tokio-postgres would try first.
        let mut host: Option<String> = None;
        let mut port: Option<u16> = None;

        for (key, value) in url.query_pairs() {
            match key.as_ref() {
                "host" => host = Some(value.into_owned()),
                "port" => {
                    port = Some(value.parse::<u16>().map_err(|_| {
                        toasty_core::Error::invalid_connection_url(format!(
                            "invalid port in connection URL query parameter: {value}"
                        ))
                    })?);
                }
                "user" => {
                    config.user(&*value);
                }
                "password" => {
                    config.password(value.as_bytes());
                }
                "dbname" => {
                    config.dbname(&*value);
                }
                "application_name" => {
                    config.application_name(&*value);
                }
                _ => {}
            }
        }

        let host = host
            .or_else(|| url.host_str().filter(|h| !h.is_empty()).map(String::from))
            .ok_or_else(|| {
                toasty_core::Error::invalid_connection_url(format!(
                    "missing host in connection URL; url={}",
                    url
                ))
            })?;
        config.host(&host);

        if let Some(port) = port.or_else(|| url.port()) {
            config.port(port);
        }

        #[cfg(feature = "tls")]
        let tls = tls::configure_tls(&url, &mut config)?;

        #[cfg(not(feature = "tls"))]
        for (key, value) in url.query_pairs() {
            if key == "sslmode" && value != "disable" {
                return Err(toasty_core::Error::invalid_connection_url(
                    "TLS not available: compile with the `tls` feature",
                ));
            }
        }

        Ok(Self {
            url: url_str,
            config,
            #[cfg(feature = "tls")]
            tls,
        })
    }

    async fn connect_with_config(&self, config: Config) -> Result<Connection> {
        #[cfg(feature = "tls")]
        if let Some(ref tls) = self.tls {
            return Connection::connect(config, tls.clone()).await;
        }
        Connection::connect(config, tokio_postgres::NoTls).await
    }
}

#[async_trait]
impl Driver for PostgreSQL {
    fn url(&self) -> Cow<'_, str> {
        Cow::Borrowed(&self.url)
    }

    fn capability(&self) -> &'static Capability {
        &Capability::POSTGRESQL
    }

    async fn connect(
        &self,
        cx: &ConnectContext,
    ) -> toasty_core::Result<Box<dyn toasty_core::driver::Connection>> {
        let mut connection = self.connect_with_config(self.config.clone()).await?;
        connection.query_log = cx.query_log;
        Ok(Box::new(connection))
    }

    fn generate_migration(&self, schema_diff: &diff::Schema<'_>) -> Migration {
        let statements = sql::MigrationStatement::from_diff(schema_diff, &Capability::POSTGRESQL);

        let sql_strings: Vec<String> = statements
            .iter()
            .map(|stmt| sql::Serializer::postgresql(stmt.schema()).serialize(stmt.statement()))
            .collect();

        Migration::new_sql(sql_strings.join("\n"))
    }

    async fn reset_db(&self) -> toasty_core::Result<()> {
        let dbname = self
            .config
            .get_dbname()
            .ok_or_else(|| {
                toasty_core::Error::invalid_connection_url("no database name configured")
            })?
            .to_string();

        // We cannot drop a database we are currently connected to, so we need a temp database.
        let temp_dbname = "__toasty_reset_temp";

        let connect = |dbname: &str| {
            let mut config = self.config.clone();
            config.dbname(dbname);
            self.connect_with_config(config)
        };

        // Step 1: Connect to the target DB and create a temp DB
        let conn = connect(&dbname).await?;
        conn.client
            .execute(&format!("DROP DATABASE IF EXISTS \"{}\"", temp_dbname), &[])
            .await
            .map_err(classify_pg_error)?;
        conn.client
            .execute(&format!("CREATE DATABASE \"{}\"", temp_dbname), &[])
            .await
            .map_err(classify_pg_error)?;
        drop(conn);

        // Step 2: Connect to the temp DB, drop and recreate the target
        let conn = connect(temp_dbname).await?;
        conn.client
            .execute(
                "SELECT pg_terminate_backend(pid) \
                 FROM pg_stat_activity \
                 WHERE datname = $1 AND pid <> pg_backend_pid()",
                &[&dbname],
            )
            .await
            .map_err(classify_pg_error)?;
        conn.client
            .execute(&format!("DROP DATABASE IF EXISTS \"{}\"", dbname), &[])
            .await
            .map_err(classify_pg_error)?;
        conn.client
            .execute(&format!("CREATE DATABASE \"{}\"", dbname), &[])
            .await
            .map_err(classify_pg_error)?;
        drop(conn);

        // Step 3: Connect back to the target and clean up the temp DB
        let conn = connect(&dbname).await?;
        conn.client
            .execute(&format!("DROP DATABASE IF EXISTS \"{}\"", temp_dbname), &[])
            .await
            .map_err(classify_pg_error)?;

        Ok(())
    }
}

/// An open connection to a PostgreSQL database.
#[derive(Debug)]
pub struct Connection {
    client: Client,
    statement_cache: StatementCache,
    oid_cache: OidCache,
    query_log: QueryLogConfig,
}

impl Connection {
    /// Initialize a Toasty PostgreSQL connection using an initialized client.
    pub fn new(client: Client) -> Self {
        Self {
            client,
            statement_cache: StatementCache::new(100),
            oid_cache: OidCache::new(),
            query_log: QueryLogConfig::default(),
        }
    }

    /// Connects to a PostgreSQL database using a [`postgres::Config`].
    ///
    /// See [`postgres::Client::configure`] for more information.
    pub async fn connect<T>(config: Config, tls: T) -> Result<Self>
    where
        T: MakeTlsConnect<Socket> + 'static,
        T::Stream: Send,
    {
        let (client, connection) = config.connect(tls).await.map_err(classify_pg_error)?;

        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("connection error: {e}");
            }
        });

        Ok(Self::new(client))
    }

    async fn exec_sql(
        &mut self,
        sql_as_str: &str,
        typed_params: Vec<TypedValue>,
        ret: SqlReturn,
    ) -> Result<ExecResponse> {
        let mut log = QueryLog::sql(
            &self.query_log,
            "postgresql",
            sql_as_str,
            typed_params.iter().map(|tv| &tv.value),
        );
        let result = self
            .exec_sql_inner(sql_as_str, typed_params, ret, &mut log)
            .await;
        log.finish(&result);
        result
    }

    async fn exec_sql_inner(
        &mut self,
        sql_as_str: &str,
        typed_params: Vec<TypedValue>,
        ret: SqlReturn,
        log: &mut QueryLog<'_>,
    ) -> Result<ExecResponse> {
        self.oid_cache
            .preload(&self.client, typed_params.iter().map(|tv| &tv.ty))
            .await?;
        let param_types: Vec<_> = typed_params
            .iter()
            .map(|tv| self.oid_cache.get(&tv.ty).clone())
            .collect();

        let values: Vec<_> = typed_params
            .into_iter()
            .map(|tv| Value::from(tv.value))
            .collect();
        let params = values
            .iter()
            .map(|param| param as &(dyn ToSql + Sync))
            .collect::<Vec<_>>();

        let statement = self
            .statement_cache
            .prepare_typed(&mut self.client, sql_as_str, &param_types)
            .await
            .map_err(classify_pg_error)?;

        if matches!(ret, SqlReturn::Count) {
            let count = self
                .client
                .execute(&statement, &params)
                .await
                .map_err(classify_pg_error)?;
            return Ok(ExecResponse::count(count));
        }

        let rows = self
            .client
            .query(&statement, &params)
            .await
            .map_err(classify_pg_error)?;

        log.rows(rows.len() as u64);

        let results = rows.into_iter().map(move |row| {
            let mut results = Vec::new();

            match &ret {
                SqlReturn::Count => unreachable!(),
                SqlReturn::Infer => {
                    for (i, column) in row.columns().iter().enumerate() {
                        results.push(Value::from_sql_infer(i, &row, column).into_inner());
                    }
                }
                SqlReturn::Types(ret_tys) => {
                    for (i, column) in row.columns().iter().enumerate() {
                        results.push(Value::from_sql(i, &row, column, &ret_tys[i]).into_inner());
                    }
                }
            }

            Ok(ValueRecord::from_vec(results))
        });

        Ok(ExecResponse::value_stream(stmt::ValueStream::from_iter(
            results,
        )))
    }

    /// Creates a table.
    pub async fn create_table(&mut self, schema: &db::Schema, table: &Table) -> Result<()> {
        let serializer = sql::Serializer::postgresql(schema);

        let sql = serializer.serialize(&sql::Statement::create_table(
            table,
            &Capability::POSTGRESQL,
        ));

        self.client
            .execute(&sql, &[])
            .await
            .map_err(classify_pg_error)?;

        for index in &table.indices {
            if index.primary_key {
                continue;
            }

            let sql = serializer.serialize(&sql::Statement::create_index(index));

            self.client
                .execute(&sql, &[])
                .await
                .map_err(classify_pg_error)?;
        }

        Ok(())
    }
}

impl From<Client> for Connection {
    fn from(client: Client) -> Self {
        Self::new(client)
    }
}

#[async_trait]
impl toasty_core::driver::Connection for Connection {
    async fn exec(&mut self, schema: &Arc<Schema>, op: Operation) -> Result<ExecResponse> {
        tracing::trace!(driver = "postgresql", op = %op.name(), "driver exec");

        if let Operation::Transaction(ref t) = op {
            // PostgreSQL has no `BEGIN IMMEDIATE` / `BEGIN EXCLUSIVE`
            // analogue; reject non-Default modes loudly rather than
            // silently dropping them at the serializer.
            if let Transaction::Start {
                mode: mode @ (TransactionMode::Immediate | TransactionMode::Exclusive),
                ..
            } = t
            {
                return Err(toasty_core::Error::unsupported_feature(format!(
                    "PostgreSQL does not support TransactionMode::{mode:?}"
                )));
            }
            let sql = sql::Serializer::postgresql(&schema.db).serialize_transaction(t);
            self.client
                .batch_execute(&sql)
                .await
                .map_err(classify_pg_error)?;
            return Ok(ExecResponse::count(0));
        }

        let (sql, typed_params, ret_tys) = match op {
            Operation::Insert(op) => (sql::Statement::from(op.stmt), op.params, None),
            Operation::QuerySql(query) => {
                assert!(
                    query.last_insert_id_hack.is_none(),
                    "last_insert_id_hack is MySQL-specific and should not be set for PostgreSQL"
                );
                (sql::Statement::from(query.stmt), query.params, query.ret)
            }
            Operation::RawSql(op) => {
                let ret = match op.ret {
                    RawSqlRet::None => SqlReturn::Count,
                    RawSqlRet::Infer => SqlReturn::Infer,
                    RawSqlRet::Types(types) => SqlReturn::Types(types),
                };
                return self.exec_sql(&op.sql, op.params, ret).await;
            }
            op => todo!("op={:#?}", op),
        };

        let sql_as_str = sql::Serializer::postgresql(&schema.db).serialize(&sql);

        let ret = if sql.returning_len().is_some() {
            SqlReturn::Types(ret_tys.unwrap())
        } else {
            SqlReturn::Count
        };

        self.exec_sql(&sql_as_str, typed_params, ret).await
    }

    async fn push_schema(&mut self, schema: &Schema) -> Result<()> {
        let serializer = sql::Serializer::postgresql(&schema.db);

        // Create PostgreSQL enum types before creating tables.
        // Collect unique enum types across all columns.
        let mut created_enum_types = hashbrown::HashSet::new();
        for table in &schema.db.tables {
            for column in &table.columns {
                if let toasty_core::schema::db::Type::Enum(type_enum) = &column.storage_ty
                    && created_enum_types.insert(type_enum.name.clone())
                {
                    let sql = serializer.serialize(&sql::Statement::create_enum_type(type_enum));

                    tracing::debug!(enum_type = ?type_enum.name, "creating enum type");
                    self.client
                        .execute(&sql, &[])
                        .await
                        .map_err(classify_pg_error)?;
                }
            }
        }

        for table in &schema.db.tables {
            tracing::debug!(table = %table.name, "creating table");
            self.create_table(&schema.db, table).await?;
        }
        Ok(())
    }

    async fn applied_migrations(
        &mut self,
    ) -> Result<Vec<toasty_core::schema::db::AppliedMigration>> {
        // Ensure the migrations table exists
        self.client
            .execute(
                "CREATE TABLE IF NOT EXISTS __toasty_migrations (
                id BIGINT PRIMARY KEY,
                name TEXT NOT NULL,
                applied_at TIMESTAMP NOT NULL
            )",
                &[],
            )
            .await
            .map_err(classify_pg_error)?;

        // Query all applied migrations
        let rows = self
            .client
            .query(
                "SELECT id FROM __toasty_migrations ORDER BY applied_at",
                &[],
            )
            .await
            .map_err(classify_pg_error)?;

        Ok(rows
            .iter()
            .map(|row| {
                let id: i64 = row.get(0);
                toasty_core::schema::db::AppliedMigration::new(id as u64)
            })
            .collect())
    }

    async fn apply_migration(
        &mut self,
        id: u64,
        name: &str,
        migration: &toasty_core::schema::db::Migration,
    ) -> Result<()> {
        tracing::info!(id = id, name = %name, "applying migration");
        // Ensure the migrations table exists
        self.client
            .execute(
                "CREATE TABLE IF NOT EXISTS __toasty_migrations (
                id BIGINT PRIMARY KEY,
                name TEXT NOT NULL,
                applied_at TIMESTAMP NOT NULL
            )",
                &[],
            )
            .await
            .map_err(classify_pg_error)?;

        // Start transaction
        let transaction = self.client.transaction().await.map_err(classify_pg_error)?;

        // Execute each migration statement
        for statement in migration.statements() {
            if let Err(e) = transaction
                .batch_execute(statement)
                .await
                .map_err(classify_pg_error)
            {
                transaction.rollback().await.map_err(classify_pg_error)?;
                return Err(e);
            }
        }

        // Record the migration
        if let Err(e) = transaction
            .execute(
                "INSERT INTO __toasty_migrations (id, name, applied_at) VALUES ($1, $2, NOW())",
                &[&(id as i64), &name],
            )
            .await
            .map_err(classify_pg_error)
        {
            transaction.rollback().await.map_err(classify_pg_error)?;
            return Err(e);
        }

        // Commit transaction
        transaction.commit().await.map_err(classify_pg_error)?;
        Ok(())
    }

    fn is_valid(&self) -> bool {
        !self.client.is_closed()
    }

    async fn ping(&mut self) -> Result<()> {
        // An empty `simple_query` is the lightest sync round-trip in
        // the PG protocol — it skips parsing entirely. Any failure is
        // surfaced as `connection_lost`: the only meaningful outcome
        // of a ping is "the connection is alive" or "evict it."
        self.client
            .simple_query("")
            .await
            .map(|_| ())
            .map_err(toasty_core::Error::connection_lost)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_postgres::config::Host;

    fn cfg(url: &str) -> Config {
        PostgreSQL::new(url).expect("valid URL").config
    }

    #[test]
    fn host_in_url_authority() {
        let c = cfg("postgresql://example.com/mydb");
        assert_eq!(c.get_hosts(), &[Host::Tcp("example.com".into())]);
        assert_eq!(c.get_dbname(), Some("mydb"));
    }

    // `Host::Unix` only exists on Unix targets in tokio-postgres, so the
    // tests that assert socket-path resolution are gated to those
    // platforms. Windows builds still exercise the URL-parsing path via
    // the non-Unix tests below.
    #[cfg(unix)]
    mod unix_socket {
        use super::*;
        use std::path::PathBuf;

        #[test]
        fn unix_socket_via_host_query_param() {
            // Regression for #984: libpq lets a Unix-socket directory be
            // supplied via `?host=/path`, since URL syntax cannot put a
            // filesystem path in the authority.
            let c = cfg("postgresql:///mydb?host=/tmp&user=myuser");
            assert_eq!(c.get_hosts(), &[Host::Unix(PathBuf::from("/tmp"))]);
            assert_eq!(c.get_user(), Some("myuser"));
            assert_eq!(c.get_dbname(), Some("mydb"));
        }

        #[test]
        fn query_param_host_overrides_url_authority() {
            // libpq semantics: a `host=` query parameter replaces (not
            // appends to) the URL authority host. tokio-postgres's
            // `Config::host` is additive across calls, so an authority
            // host would otherwise be tried first and the configured
            // socket reached only as a fallback.
            let c = cfg("postgresql://example.com/mydb?host=/var/run/postgresql");
            assert_eq!(
                c.get_hosts(),
                &[Host::Unix(PathBuf::from("/var/run/postgresql"))]
            );
        }

        #[test]
        fn query_param_port_user_password_dbname() {
            let c = cfg(
                "postgresql:///placeholder?host=/tmp&port=5433&user=alice&password=s3cret&dbname=real",
            );
            assert_eq!(c.get_hosts(), &[Host::Unix(PathBuf::from("/tmp"))]);
            assert_eq!(c.get_ports(), &[5433]);
            assert_eq!(c.get_user(), Some("alice"));
            assert_eq!(c.get_password(), Some(&b"s3cret"[..]));
            assert_eq!(c.get_dbname(), Some("real"));
        }
    }

    #[test]
    fn query_param_port_overrides_url_port() {
        // `Config::port` is additive too, so a `port=` query parameter
        // must replace the URL authority port for the same reason.
        let c = cfg("postgresql://example.com:5432/mydb?port=5433");
        assert_eq!(c.get_ports(), &[5433]);
    }

    #[test]
    fn application_name_query_param() {
        let c = cfg("postgresql://localhost/mydb?application_name=my_app");
        assert_eq!(c.get_application_name(), Some("my_app"));
    }

    #[test]
    fn missing_host_rejected() {
        let err = PostgreSQL::new("postgresql:///mydb").unwrap_err();
        assert!(
            err.to_string().contains("missing host"),
            "expected missing-host error, got: {err}"
        );
    }

    #[test]
    fn invalid_port_query_param_rejected() {
        let err = PostgreSQL::new("postgresql:///mydb?host=/tmp&port=not-a-number").unwrap_err();
        assert!(
            err.to_string().contains("invalid port"),
            "expected invalid-port error, got: {err}"
        );
    }
}
