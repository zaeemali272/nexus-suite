//! SQLite connection pool management and WAL mode pragma configuration.

use nexus_core::{NexusError, NexusResult};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::path::PathBuf;
use std::str::FromStr;
use tracing::info;

/// Configuration options for the SQLite database engine.
#[derive(Debug, Clone)]
pub struct DbConfig {
    pub db_path: PathBuf,
    pub max_connections: u32,
}

impl Default for DbConfig {
    fn default() -> Self {
        Self {
            db_path: PathBuf::from("nexus.db"),
            max_connections: 16,
        }
    }
}

/// Managed SQLite database pool wrapper.
#[derive(Debug, Clone)]
pub struct DatabasePool {
    pool: SqlitePool,
}

impl DatabasePool {
    /// Initialize SQLite connection pool with WAL mode and custom pragmas.
    pub async fn connect(config: &DbConfig) -> NexusResult<Self> {
        let connection_string = format!("sqlite://{}", config.db_path.to_string_lossy());
        
        let options = SqliteConnectOptions::from_str(&connection_string)
            .map_err(|e| NexusError::Database(e.to_string()))?
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .synchronous(sqlx::sqlite::SqliteSynchronous::Normal)
            .foreign_keys(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(config.max_connections)
            .connect_with(options)
            .await
            .map_err(|e| NexusError::Database(format!("Failed to connect to SQLite pool: {e}")))?;

        info!("SQLite WAL mode initialized successfully at {:?}", config.db_path);

        // Run embedded database schema migrations
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .map_err(|e| NexusError::Database(format!("Failed to run DB migrations: {e}")))?;

        Ok(Self { pool })
    }

    /// Access the underlying `sqlx::SqlitePool`.
    pub fn inner(&self) -> &SqlitePool {
        &self.pool
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_sqlite_wal_connection_and_migrations() {
        let tmp = NamedTempFile::new().unwrap();
        let config = DbConfig {
            db_path: tmp.path().to_path_buf(),
            max_connections: 5,
        };

        let pool = DatabasePool::connect(&config).await.expect("DB pool failed");
        let result: (String,) = sqlx::query_as("PRAGMA journal_mode;")
            .fetch_one(pool.inner())
            .await
            .expect("Failed to query pragma");

        assert_eq!(result.0.to_lowercase(), "wal");
    }
}
