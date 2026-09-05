//! Connecting integration tests to a real PostgreSQL.
//!
//! These tests run against a live database on purpose. Row-level security,
//! `SET LOCAL ROLE` and unique constraints are the things most worth testing
//! here, and none of them exist in a mock.
//!
//! Set `ANTHOVAI_TEST_DATABASE_URL` to run them. When it is unset the tests
//! skip loudly rather than silently passing — a green suite that quietly tested
//! nothing is worse than a red one.

use anthovai_db::Db;

pub const TEST_DATABASE_ENV: &str = "ANTHOVAI_TEST_DATABASE_URL";

/// How many connections one test may hold.
///
/// Deliberately small. Each `#[tokio::test]` runs on its own runtime and gets
/// its own pool — a pool cannot be shared between them, because a sqlx pool
/// belongs to the runtime that built it and its connections die when that
/// runtime does. So the budget that matters is per test times however many run
/// at once, against PostgreSQL's connection limit.
const CONNECTIONS_PER_TEST: u32 = 3;

/// A pool for the test database, or `None` when the environment variable is
/// unset. Tests take the `None` branch as a skip.
pub async fn test_db() -> Option<Db> {
    let url = std::env::var(TEST_DATABASE_ENV).ok()?;
    let db = Db::connect(&url, CONNECTIONS_PER_TEST)
        .await
        .unwrap_or_else(|e| panic!("{TEST_DATABASE_ENV} is set but unusable: {e}"));

    // Idempotent, and sqlx takes an advisory lock, so concurrent tests
    // serialise here rather than racing.
    db.run_migrations()
        .await
        .unwrap_or_else(|e| panic!("could not migrate the test database: {e}"));

    Some(db)
}

/// Print why a test did nothing, so a skip is visible in the output.
pub fn skipped(test_name: &str) {
    eprintln!("SKIPPED {test_name}: set {TEST_DATABASE_ENV} to run database tests");
}

/// Wrap a database test. The body runs only when a test database is configured.
///
/// ```ignore
/// db_test!(async fn it_works(db) {
///     db.ping().await.unwrap();
/// });
/// ```
#[macro_export]
macro_rules! db_test {
    (async fn $name:ident($db:ident) $body:block) => {
        #[tokio::test]
        async fn $name() {
            let Some($db) = $crate::db::test_db().await else {
                $crate::db::skipped(stringify!($name));
                return;
            };
            $body
        }
    };
}
