//! Tests against a real PostgreSQL server.
//!
//! Skipped unless `INTALIX_TEST_PG` is set to a libpq-style connection string for a role that
//! may create schemas, for example:
//!
//! ```text
//! INTALIX_TEST_PG="host=localhost port=5432 user=postgres password=secret dbname=postgres" cargo test
//! ```
//!
//! The suite creates and drops its own schema `intalix_it`; nothing else is touched.
//! Everything runs in one test so setup happens once and steps run in a fixed order.

use std::str::FromStr;
use std::time::Duration;

use secrecy::SecretString;
use tokio_postgres::config::Host;

use crate::error::ErrorCategory;
use crate::jobs::Jobs;
use crate::lang::variables::{
    OptionItem, OptionsSource, VariableDef, VariableType, resolve_values,
};
use crate::model::{
    Cell, ConnectionTarget, ConnectionTestResult, QueryOutcome, RelationKind, SslMode,
};
use crate::pg::connect::{Pools, test_connection};
use crate::pg::variables::CheckOutcome;
use crate::pg::{introspect, query};
use serde_json::{Value, json};
use std::collections::BTreeMap;

struct Env {
    target: ConnectionTarget,
    password: SecretString,
}

fn env() -> Option<Env> {
    let raw = std::env::var("INTALIX_TEST_PG").ok()?;
    let config = tokio_postgres::Config::from_str(&raw).ok()?;
    let host = match config.get_hosts().first()? {
        Host::Tcp(h) => h.clone(),
        #[allow(unreachable_patterns)]
        _ => return None,
    };
    let password = String::from_utf8(config.get_password()?.to_vec()).ok()?;
    Some(Env {
        target: ConnectionTarget {
            host,
            port: *config.get_ports().first().unwrap_or(&5432),
            database: config.get_dbname()?.to_owned(),
            username: config.get_user()?.to_owned(),
            ssl_mode: SslMode::Prefer,
            ca_cert_path: String::new(),
            connect_timeout_secs: 5,
            query_timeout_secs: 30,
            max_connections: 3,
            application_name: "Intalix Studio tests".into(),
        },
        password: SecretString::from(password),
    })
}

/// Plain read-write connection used only to create and drop the fixture schema.
async fn admin(env: &Env) -> tokio_postgres::Client {
    let tls = crate::pg::connect::tls_connector(&env.target).unwrap();
    let (client, conn) = crate::pg::connect::pg_config(&env.target, &env.password)
        .connect(tls)
        .await
        .unwrap();
    tokio::spawn(async move {
        let _ = conn.await;
    });
    client
}

const FIXTURE: &str = r#"
DROP SCHEMA IF EXISTS intalix_it CASCADE;
CREATE SCHEMA intalix_it;
COMMENT ON SCHEMA intalix_it IS 'Intalix integration tests';
CREATE TABLE intalix_it.customers (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    code text NOT NULL UNIQUE,
    "Display Name" text,
    credit numeric(14,2) DEFAULT 0,
    segment text CHECK (segment IN ('a', 'b'))
);
COMMENT ON TABLE intalix_it.customers IS 'Customers, it''s a test';
COMMENT ON COLUMN intalix_it.customers.code IS 'External code';
CREATE TABLE intalix_it.orders (
    id bigserial PRIMARY KEY,
    customer_id bigint NOT NULL REFERENCES intalix_it.customers(id),
    placed_at timestamptz NOT NULL DEFAULT now(),
    total numeric(14,2) GENERATED ALWAYS AS (0) STORED
);
CREATE INDEX orders_customer_idx ON intalix_it.orders (customer_id);
CREATE VIEW intalix_it.customer_codes AS SELECT id, code FROM intalix_it.customers;
CREATE MATERIALIZED VIEW intalix_it.order_counts AS SELECT customer_id, count(*) AS n FROM intalix_it.orders GROUP BY 1;
CREATE FUNCTION intalix_it.add_one(x integer) RETURNS integer LANGUAGE sql AS 'SELECT x + 1';
INSERT INTO intalix_it.customers (code, "Display Name", credit, segment)
SELECT 'C' || g, 'Customer ' || g, g * 10.5, CASE WHEN g % 2 = 0 THEN 'a' ELSE 'b' END FROM generate_series(1, 50) g;
CREATE TYPE intalix_it.invoice_status AS ENUM ('open', 'paid', 'void');
CREATE TABLE intalix_it.invoices (
    id int PRIMARY KEY,
    status intalix_it.invoice_status NOT NULL,
    issued date NOT NULL,
    amount numeric(12,2) NOT NULL,
    region text NOT NULL
);
INSERT INTO intalix_it.invoices
SELECT g, (ARRAY['open', 'paid', 'void'])[1 + g % 3]::intalix_it.invoice_status, DATE '2026-01-01' + g,
       g * 10.25, CASE WHEN g % 2 = 0 THEN 'north' ELSE 'south' END
FROM generate_series(1, 60) g;
"#;

fn ok(outcome: QueryOutcome) -> crate::model::QueryResultSet {
    match outcome {
        QueryOutcome::Ok(s) => s.result,
        QueryOutcome::Failed(f) => panic!("query failed: {:?} {}", f.category, f.message),
    }
}

fn failed(outcome: QueryOutcome) -> ErrorCategory {
    match outcome {
        QueryOutcome::Ok(_) => panic!("expected the query to fail"),
        QueryOutcome::Failed(f) => f.category,
    }
}

#[tokio::test]
async fn against_real_postgres() {
    let Some(env) = env() else {
        eprintln!("INTALIX_TEST_PG not set; skipping PostgreSQL integration tests");
        return;
    };

    // --- Connection tests -------------------------------------------------------------
    match test_connection(&env.target, &env.password).await {
        ConnectionTestResult::Ok(s) => assert!(
            s.server_version.starts_with("PostgreSQL "),
            "{}",
            s.server_version
        ),
        ConnectionTestResult::Failed(f) => {
            panic!("connection test failed: {:?} {}", f.category, f.message)
        }
    }
    let wrong = SecretString::from("definitely-not-the-password".to_owned());
    match test_connection(&env.target, &wrong).await {
        ConnectionTestResult::Failed(f) => {
            assert_eq!(f.category, ErrorCategory::AuthenticationFailed)
        }
        ConnectionTestResult::Ok(_) => panic!("wrong password must fail"),
    }
    let missing_db = ConnectionTarget {
        database: "intalix_no_such_db".into(),
        ..env.target.clone()
    };
    match test_connection(&missing_db, &env.password).await {
        ConnectionTestResult::Failed(f) => assert_eq!(f.category, ErrorCategory::DatabaseNotFound),
        ConnectionTestResult::Ok(_) => panic!("missing database must fail"),
    }
    let closed_port = ConnectionTarget {
        port: 1,
        ..env.target.clone()
    };
    match test_connection(&closed_port, &env.password).await {
        ConnectionTestResult::Failed(f) => {
            assert!(
                matches!(
                    f.category,
                    ErrorCategory::ConnectionFailed | ErrorCategory::Timeout
                ),
                "{:?}",
                f.category
            )
        }
        ConnectionTestResult::Ok(_) => panic!("closed port must fail"),
    }

    // --- Fixture --------------------------------------------------------------------
    let admin = admin(&env).await;
    admin.batch_execute(FIXTURE).await.unwrap();

    let pools = Pools::new();
    let password = env.password.clone();
    let handle = pools
        .get_or_create("it", &env.target, move || Ok(password))
        .await
        .unwrap();
    let jobs = Jobs::default();
    let run = |sql: &'static str, limit: u32, id: &'static str| {
        let (handle, jobs) = (&handle, &jobs);
        async move {
            jobs.register(id);
            let r = query::execute(
                handle,
                jobs,
                query::QueryRequest {
                    request_id: id,
                    sql,
                    row_limit: limit,
                },
            )
            .await;
            jobs.finish(id);
            r
        }
    };

    // --- Exact values ---------------------------------------------------------------
    let result = ok(run(
        "SELECT 12345678901234567890.12::numeric AS n, -0.000001::numeric(10,6) AS small, 'NaN'::numeric AS nan, \
                9223372036854775807::bigint AS big, true AS b, 'héllo'::text AS t, NULL::int AS nul, \
                DATE '2026-01-31' AS d, 'infinity'::date AS dinf, TIMESTAMP '2026-01-31 13:05:09.25' AS ts, \
                TIMESTAMPTZ '2026-01-31 13:05:09+05:30' AS tstz, INTERVAL '1 year 2 months 3 days 04:05:06' AS iv, \
                '6f9619ff-8b86-d011-b42d-00c04fc964ff'::uuid AS u, '{\"a\": [1, 2]}'::jsonb AS j, \
                '10.1.0.0/16'::cidr AS net, ARRAY[1, NULL, 3] AS arr, '\\xdeadbeef'::bytea AS bin, \
                12.5::money AS m, 1.5::float8 AS f",
        10,
        "types",
    )
    .await
    .unwrap());
    let row = &result.rows[0];
    let col = |name: &str| &row[result.columns.iter().position(|c| c.name == name).unwrap()];
    assert_eq!(
        col("n"),
        &Cell::Decimal {
            v: "12345678901234567890.12".into()
        }
    );
    assert_eq!(
        col("small"),
        &Cell::Decimal {
            v: "-0.000001".into()
        }
    );
    assert_eq!(col("nan"), &Cell::Decimal { v: "NaN".into() });
    assert_eq!(
        col("big"),
        &Cell::Int {
            v: "9223372036854775807".into()
        }
    );
    assert_eq!(col("b"), &Cell::Bool { v: true });
    assert_eq!(
        col("t"),
        &Cell::Text {
            v: "héllo".into(),
            truncated: false
        }
    );
    assert_eq!(col("nul"), &Cell::Null);
    assert_eq!(
        col("d"),
        &Cell::Date {
            v: "2026-01-31".into()
        }
    );
    assert_eq!(
        col("dinf"),
        &Cell::Date {
            v: "infinity".into()
        }
    );
    assert_eq!(
        col("ts"),
        &Cell::Timestamp {
            v: "2026-01-31 13:05:09.25".into()
        }
    );
    assert_eq!(
        col("tstz"),
        &Cell::Timestamptz {
            v: "2026-01-31 07:35:09+00".into()
        }
    );
    assert_eq!(
        col("iv"),
        &Cell::Text {
            v: "1 year 2 mons 3 days 04:05:06".into(),
            truncated: false
        }
    );
    assert_eq!(
        col("u"),
        &Cell::Uuid {
            v: "6f9619ff-8b86-d011-b42d-00c04fc964ff".into()
        }
    );
    assert_eq!(
        col("j"),
        &Cell::Json {
            v: "{\"a\": [1, 2]}".into(),
            truncated: false
        }
    );
    assert_eq!(
        col("net"),
        &Cell::Text {
            v: "10.1.0.0/16".into(),
            truncated: false
        }
    );
    assert_eq!(
        col("arr"),
        &Cell::Text {
            v: "{1,NULL,3}".into(),
            truncated: false
        }
    );
    assert_eq!(col("bin"), &Cell::Bytes { size: 4 });
    assert_eq!(col("m"), &Cell::Decimal { v: "12.50".into() });
    assert_eq!(col("f"), &Cell::Float { v: 1.5 });

    // --- Row limits -----------------------------------------------------------------
    let limited = ok(
        run("SELECT g FROM generate_series(1, 1000) g", 100, "limit")
            .await
            .unwrap(),
    );
    assert_eq!(limited.rows.len(), 100);
    assert!(limited.truncated);
    let exact = ok(run("SELECT g FROM generate_series(1, 100) g", 100, "exact")
        .await
        .unwrap());
    assert_eq!(exact.rows.len(), 100);
    assert!(!exact.truncated);

    // --- Read-only layers -----------------------------------------------------------
    // Layer 1: policy.
    assert_eq!(
        failed(
            run("DELETE FROM intalix_it.customers", 10, "p1")
                .await
                .unwrap()
        ),
        ErrorCategory::QueryRejected
    );
    // Layer 3: the database's READ ONLY transaction (bypassing the policy on purpose).
    {
        let mut client = handle.pool.get().await.unwrap();
        let err = query::run(
            &mut client,
            "CREATE TABLE intalix_it.should_not_exist (a int)",
            10,
            30,
        )
        .await
        .err()
        .unwrap();
        assert_eq!(
            err.category,
            ErrorCategory::QueryRejected,
            "{}",
            err.message
        );
        let err = query::run(
            &mut client,
            "SELECT nextval('intalix_it.orders_id_seq')",
            10,
            30,
        )
        .await
        .err()
        .unwrap();
        assert_eq!(
            err.category,
            ErrorCategory::QueryRejected,
            "{}",
            err.message
        );
        // Layer 2: the protocol refuses a second statement.
        let err = query::run(&mut client, "SELECT 1; SELECT 2", 10, 30)
            .await
            .err()
            .unwrap();
        assert_eq!(err.category, ErrorCategory::SyntaxError, "{}", err.message);
    }
    let exists: bool = admin
        .query_one(
            "SELECT to_regclass('intalix_it.should_not_exist') IS NOT NULL",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert!(!exists, "a write slipped through the read-only transaction");

    // --- Errors from the server -------------------------------------------------------
    assert_eq!(
        failed(
            run("SELECT * FROM intalix_it.nope", 10, "e1")
                .await
                .unwrap()
        ),
        ErrorCategory::UndefinedObject
    );
    assert_eq!(
        failed(run("SELECT 1/0", 10, "e2").await.unwrap()),
        ErrorCategory::QueryFailed
    );
    match run(
        "SELECT * FROM intalix_it.customers WHERE nosuch = 1",
        10,
        "e3",
    )
    .await
    .unwrap()
    {
        QueryOutcome::Failed(f) => {
            assert_eq!(f.category, ErrorCategory::UndefinedObject);
            assert!(f.position.is_some(), "position should be reported");
        }
        QueryOutcome::Ok(_) => panic!("expected failure"),
    }
    assert_eq!(
        failed(
            run("SELECT * FROM intalix_it.customers WHERE id = $1", 10, "e4")
                .await
                .unwrap()
        ),
        ErrorCategory::QueryRejected
    );

    // --- Cancellation ---------------------------------------------------------------
    let started = std::time::Instant::now();
    let (outcome, ()) = tokio::join!(run("SELECT pg_sleep(30)", 10, "cancel-me"), async {
        tokio::time::sleep(Duration::from_millis(500)).await;
        jobs.cancel("cancel-me").await;
    });
    assert_eq!(
        outcome.err().map(|e| e.category),
        Some(ErrorCategory::Cancelled)
    );
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "cancel took {:?}",
        started.elapsed()
    );

    // Cancel before the query starts.
    jobs.register("early");
    jobs.cancel("early").await;
    let early = query::execute(
        &handle,
        &jobs,
        query::QueryRequest {
            request_id: "early",
            sql: "SELECT 1",
            row_limit: 1,
        },
    )
    .await;
    jobs.finish("early");
    assert_eq!(
        early.err().map(|e| e.category),
        Some(ErrorCategory::Cancelled)
    );

    // --- Statement timeout ------------------------------------------------------------
    let fast = ConnectionTarget {
        query_timeout_secs: 1,
        ..env.target.clone()
    };
    let password = env.password.clone();
    let fast_handle = pools
        .get_or_create("it-fast", &fast, move || Ok(password))
        .await
        .unwrap();
    jobs.register("slow");
    let slow = query::execute(
        &fast_handle,
        &jobs,
        query::QueryRequest {
            request_id: "slow",
            sql: "SELECT pg_sleep(5)",
            row_limit: 1,
        },
    )
    .await
    .unwrap();
    jobs.finish("slow");
    assert_eq!(failed(slow), ErrorCategory::Timeout);

    // --- Introspection --------------------------------------------------------------
    let mut client = handle.pool.get().await.unwrap();
    let info = introspect::database_info(&mut client).await.unwrap();
    assert_eq!(info.name, env.target.database);
    let schemas = introspect::list_schemas(&mut client).await.unwrap();
    let it = schemas
        .iter()
        .find(|s| s.name == "intalix_it")
        .expect("fixture schema listed");
    assert_eq!(it.comment.as_deref(), Some("Intalix integration tests"));
    assert!(!it.is_system);
    assert!(
        schemas
            .iter()
            .any(|s| s.name == "pg_catalog" && s.is_system)
    );

    let objects = introspect::list_objects(&mut client, "intalix_it")
        .await
        .unwrap();
    let kinds: Vec<(&str, RelationKind)> = objects
        .relations
        .iter()
        .map(|r| (r.name.as_str(), r.kind))
        .collect();
    assert!(kinds.contains(&("customers", RelationKind::Table)));
    assert!(kinds.contains(&("customer_codes", RelationKind::View)));
    assert!(kinds.contains(&("order_counts", RelationKind::MaterializedView)));
    assert!(
        objects
            .routines
            .iter()
            .any(|r| r.name == "add_one" && r.arguments == "x integer")
    );
    let seq = objects
        .sequences
        .iter()
        .find(|s| s.name == "orders_id_seq")
        .expect("sequence listed");
    assert_eq!(seq.owned_by.as_deref(), Some("intalix_it.orders.id"));

    let customers = introspect::relation_detail(&mut client, "intalix_it", "customers")
        .await
        .unwrap();
    assert_eq!(
        customers.summary.comment.as_deref(),
        Some("Customers, it's a test")
    );
    let id = &customers.columns[0];
    assert!(id.is_primary_key && !id.nullable);
    assert_eq!(
        id.default_value.as_deref(),
        Some("generated always as identity")
    );
    let code = customers.columns.iter().find(|c| c.name == "code").unwrap();
    assert_eq!(code.comment.as_deref(), Some("External code"));
    assert!(
        customers
            .constraints
            .iter()
            .any(|k| k.kind == crate::model::ConstraintKind::Unique)
    );
    assert!(
        customers
            .constraints
            .iter()
            .any(|k| k.kind == crate::model::ConstraintKind::Check)
    );
    assert!(
        customers.ddl.contains("\"Display Name\" text"),
        "{}",
        customers.ddl
    );
    assert!(
        customers
            .ddl
            .contains("COMMENT ON TABLE intalix_it.customers IS 'Customers, it''s a test';")
    );

    let orders = introspect::relation_detail(&mut client, "intalix_it", "orders")
        .await
        .unwrap();
    let fk = orders
        .columns
        .iter()
        .find(|c| c.name == "customer_id")
        .and_then(|c| c.foreign_key.clone())
        .unwrap();
    assert_eq!(
        (fk.schema.as_str(), fk.table.as_str(), fk.column.as_str()),
        ("intalix_it", "customers", "id")
    );
    let total = orders.columns.iter().find(|c| c.name == "total").unwrap();
    assert_eq!(
        total.default_value.as_deref(),
        Some("generated always as (0) stored")
    );
    assert!(
        orders
            .indexes
            .iter()
            .any(|i| i.name == "orders_customer_idx" && i.columns == ["customer_id"])
    );
    assert!(
        orders.ddl.contains("CREATE INDEX orders_customer_idx"),
        "{}",
        orders.ddl
    );

    let view = introspect::relation_detail(&mut client, "intalix_it", "customer_codes")
        .await
        .unwrap();
    assert!(
        view.ddl
            .starts_with("CREATE OR REPLACE VIEW intalix_it.customer_codes AS"),
        "{}",
        view.ddl
    );
    assert!(
        introspect::relation_detail(&mut client, "intalix_it", "nope")
            .await
            .is_err()
    );

    let catalog = introspect::completion_catalog(&mut client).await.unwrap();
    assert_eq!(catalog["intalix_it"]["customers"][0], "id");

    // --- Preview quoting --------------------------------------------------------------
    let preview = ok(run_preview(&handle, &jobs).await);
    assert_eq!(preview.rows.len(), 6); // limit 5 + 1 look-ahead row, trimmed by the service layer

    // --- SQL Editor: language catalog and variables -----------------------------------
    let lang = introspect::language_catalog(&mut client).await.unwrap();
    let invoices = lang
        .find_relation(Some("intalix_it"), "invoices")
        .expect("invoices in the language catalog");
    assert_eq!(invoices.columns[1].name, "status");
    assert!(invoices.columns[1].data_type.ends_with("invoice_status"));
    assert_eq!(invoices.columns[3].data_type, "numeric(12,2)");
    assert!(
        lang.routines
            .iter()
            .any(|r| r.schema == "intalix_it" && r.name == "add_one" && r.arguments == "x integer")
    );
    let status_column = crate::lang::columns::compared_column(
        &crate::lang::lexer::lex("SELECT * FROM intalix_it.invoices i WHERE i.status IN ({{s}})"),
        &lang,
        "s",
    )
    .expect("multi-select column found");
    assert_eq!(status_column.label(), "intalix_it.invoices.status");
    variable_steps(&handle, &jobs).await;

    // --- Cleanup ----------------------------------------------------------------------
    drop(client);
    pools.invalidate("it").await;
    pools.invalidate("it-fast").await;
    admin
        .batch_execute("DROP SCHEMA intalix_it CASCADE")
        .await
        .unwrap();
}

async fn run_preview(handle: &crate::pg::connect::PoolHandle, jobs: &Jobs) -> QueryOutcome {
    let sql = query::preview_sql("intalix_it", "customers", 5);
    jobs.register("preview");
    let outcome = query::execute(
        handle,
        jobs,
        query::QueryRequest {
            request_id: "preview",
            sql: &sql,
            row_limit: 6,
        },
    )
    .await;
    jobs.finish("preview");
    outcome.unwrap()
}

/// Regression: requiring TLS against a server with TLS off must say so clearly, not
/// "server unreachable". Only meaningful when the test server has `ssl = off`.
#[tokio::test]
async fn tls_required_but_server_has_tls_off() {
    let Some(env) = env() else { return };
    let ssl_on = {
        let admin = admin(&env).await;
        let row = admin.query_one("SHOW ssl", &[]).await.unwrap();
        row.get::<_, String>(0) == "on"
    };
    if ssl_on {
        eprintln!("server has TLS on; skipping");
        return;
    }
    for mode in [SslMode::Require, SslMode::VerifyCa, SslMode::VerifyFull] {
        let target = ConnectionTarget {
            ssl_mode: mode,
            ..env.target.clone()
        };
        match test_connection(&target, &env.password).await {
            ConnectionTestResult::Failed(f) => {
                assert_eq!(
                    f.category,
                    ErrorCategory::TlsError,
                    "{mode:?}: {}",
                    f.message
                );
                assert_eq!(
                    f.message,
                    "The server does not accept encrypted (TLS) connections."
                );
            }
            ConnectionTestResult::Ok(_) => {
                panic!("{mode:?} must fail against a server without TLS")
            }
        }
    }
    // Prefer falls back to an unencrypted connection, like libpq.
    let prefer = ConnectionTarget {
        ssl_mode: SslMode::Prefer,
        ..env.target.clone()
    };
    match test_connection(&prefer, &env.password).await {
        ConnectionTestResult::Ok(s) => assert_eq!(s.tls, None),
        ConnectionTestResult::Failed(f) => panic!("prefer should connect: {}", f.message),
    }
}

fn var(name: &str, kind: VariableType, choices: &[&str]) -> VariableDef {
    let options = (!choices.is_empty()).then(|| OptionsSource::List {
        values: choices
            .iter()
            .map(|c| OptionItem {
                value: (*c).to_owned(),
                label: String::new(),
            })
            .collect(),
    });
    VariableDef {
        name: name.into(),
        label: String::new(),
        kind,
        required: false,
        default: Value::Null,
        options,
        help: String::new(),
    }
    .required_flag(true)
}

trait RequiredFlag {
    fn required_flag(self, on: bool) -> Self;
}

impl RequiredFlag for VariableDef {
    fn required_flag(mut self, on: bool) -> Self {
        self.required = on;
        self
    }
}

async fn run_vars(
    handle: &crate::pg::connect::PoolHandle,
    jobs: &Jobs,
    sql: &str,
    defs: &[VariableDef],
    values: Value,
) -> QueryOutcome {
    let values: BTreeMap<String, Value> = serde_json::from_value(values).unwrap();
    let bound = resolve_values(defs, &values).unwrap();
    jobs.register("vars");
    let outcome = crate::pg::variables::run(handle, jobs, "vars", sql, defs, &bound, 1000).await;
    jobs.finish("vars");
    outcome.unwrap()
}

async fn check_vars(
    handle: &crate::pg::connect::PoolHandle,
    jobs: &Jobs,
    sql: &str,
    defs: &[VariableDef],
) -> CheckOutcome {
    jobs.register("check");
    let outcome = crate::pg::variables::check(handle, jobs, "check", sql, defs).await;
    jobs.finish("check");
    outcome.unwrap()
}

fn single_int(outcome: QueryOutcome) -> String {
    match &ok(outcome).rows[0][0] {
        Cell::Int { v } => v.clone(),
        other => panic!("expected an integer, got {other:?}"),
    }
}

/// `{{variables}}` end to end: inference, binding, NULL for empty filters, injection safety,
/// error positions and the read-only policy.
async fn variable_steps(handle: &crate::pg::connect::PoolHandle, jobs: &Jobs) {
    let defs = vec![
        var("period", VariableType::DateRange, &[]),
        var("region", VariableType::Enum, &["north", "south"]),
        var("status", VariableType::MultiEnum, &["open", "paid", "void"]),
        var("min", VariableType::Number, &[]),
    ];
    let sql = "SELECT count(*) AS n FROM intalix_it.invoices \
               WHERE issued BETWEEN {{period.from}} AND {{period.to}} \
                 AND ({{region}} IS NULL OR region = {{region}}) \
                 AND status = ANY({{status}}) \
                 AND amount >= {{min}}";
    let january = json!({
        "period": {"from": "2026-01-01", "to": "2026-01-31"},
        "region": "north",
        "status": ["open", "paid"],
        "min": "50",
    });
    assert_eq!(
        single_int(run_vars(handle, jobs, sql, &defs, january).await),
        "9"
    );

    // Required unchecked switches the filter off, even with a value: all regions.
    let mut region_off = defs.clone();
    region_off[1] = region_off[1].clone().required_flag(false);
    let any_region = json!({
        "period": {"from": "2026-01-01", "to": "2026-01-31"},
        "region": "north",
        "status": ["open", "paid"],
        "min": "50",
    });
    assert_eq!(
        single_int(run_vars(handle, jobs, sql, &region_off, any_region).await),
        "17"
    );

    // The check reports the types PostgreSQL will use, including the enum array.
    match check_vars(handle, jobs, sql, &defs).await {
        CheckOutcome::Ok(c) => {
            assert_eq!(c.columns[0].name, "n");
            let types: Vec<(String, String)> = c
                .parameters
                .into_iter()
                .map(|p| (p.reference, p.pg_type))
                .collect();
            assert!(
                types.contains(&("period.from".into(), "date".into())),
                "{types:?}"
            );
            assert!(
                types.contains(&("status".into(), "invoice_status[]".into())),
                "{types:?}"
            );
            assert!(
                types.contains(&("region".into(), "text".into())),
                "{types:?}"
            );
            assert!(
                types.contains(&("min".into(), "numeric".into())),
                "{types:?}"
            );
        }
        CheckOutcome::Failed(f) => panic!("check failed: {}", f.message),
    }

    // A not-required variable left empty skips its condition instead of comparing with NULL.
    let skip_sql =
        "SELECT count(*) FROM intalix_it.invoices WHERE region = {{region}} AND amount >= {{min}}";
    assert_eq!(
        single_int(run_vars(handle, jobs, skip_sql, &region_off, json!({"min": "600"})).await),
        "2"
    );

    // Values can't change the query: a quote is just part of the text.
    let text = vec![var("q", VariableType::Text, &[])];
    let injected = run_vars(
        handle,
        jobs,
        "SELECT count(*) FROM intalix_it.invoices WHERE region = {{q}}",
        &text,
        json!({"q": "north' OR '1'='1"}),
    )
    .await;
    assert_eq!(single_int(injected), "0");

    // Inference retries a parameter PostgreSQL can't type on its own.
    let echo = ok(run_vars(
        handle,
        jobs,
        "SELECT {{q}} AS echo",
        &text,
        json!({"q": "it's"}),
    )
    .await);
    assert!(matches!(&echo.rows[0][0], Cell::Text { v, .. } if v == "it's"));

    // A single enum value against an enum column, and a number used by LIMIT.
    let one = vec![var("st", VariableType::Enum, &["open", "paid", "void"])];
    let paid = run_vars(
        handle,
        jobs,
        "SELECT count(*) FROM intalix_it.invoices WHERE status = {{st}}",
        &one,
        json!({"st": "paid"}),
    )
    .await;
    assert_eq!(single_int(paid), "20");
    let n = vec![var("n", VariableType::Number, &[])];
    let limited = ok(run_vars(
        handle,
        jobs,
        "SELECT id FROM intalix_it.invoices ORDER BY id LIMIT {{n}}",
        &n,
        json!({"n": "3"}),
    )
    .await);
    assert_eq!(limited.rows.len(), 3);

    // `IN ({{list}})` works like `= ANY(…)`, and NOT IN like `<> ALL(…)`.
    let statuses = vec![var("status", VariableType::MultiEnum, &[])];
    let in_list = run_vars(
        handle,
        jobs,
        "SELECT count(*) FROM intalix_it.invoices WHERE status IN ({{status}})",
        &statuses,
        json!({"status": ["paid", "void"]}),
    )
    .await;
    assert_eq!(single_int(in_list), "40");
    let not_in = run_vars(
        handle,
        jobs,
        "SELECT count(*) FROM intalix_it.invoices WHERE status NOT IN ({{status}})",
        &statuses,
        json!({"status": ["paid", "void"]}),
    )
    .await;
    assert_eq!(single_int(not_in), "20");

    // A list compared with = gets a clear message pointing at the variable.
    let list = vec![var("status", VariableType::MultiEnum, &["open"])];
    let in_sql = "SELECT 1 FROM intalix_it.invoices WHERE status = {{status}}";
    match check_vars(handle, jobs, in_sql, &list).await {
        CheckOutcome::Failed(f) => {
            assert_eq!(f.category, ErrorCategory::QueryRejected);
            assert!(f.hint.unwrap_or_default().contains("IN ("));
            assert_eq!(f.position, Some(in_sql.find("{{").unwrap() as u32 + 1));
        }
        CheckOutcome::Ok(_) => panic!("= with a list must be rejected"),
    }

    // Server errors point at the source SQL, not the rewritten one.
    let bad = "SELECT {{min}} + nope FROM intalix_it.invoices";
    match check_vars(handle, jobs, bad, &defs).await {
        CheckOutcome::Failed(f) => {
            assert_eq!(f.category, ErrorCategory::UndefinedObject);
            assert_eq!(f.position, Some(bad.find("nope").unwrap() as u32 + 1));
        }
        CheckOutcome::Ok(_) => panic!("unknown column must fail"),
    }

    // The read-only policy still applies.
    let write = check_vars(
        handle,
        jobs,
        "DELETE FROM intalix_it.invoices WHERE id = {{min}}",
        &defs,
    )
    .await;
    assert!(
        matches!(&write, CheckOutcome::Failed(f) if f.category == ErrorCategory::QueryRejected),
        "{write:?}"
    );
    assert_eq!(
        single_int(
            run_vars(
                handle,
                jobs,
                "SELECT count(*) FROM intalix_it.invoices",
                &[],
                json!({}),
            )
            .await
        ),
        "60"
    );
}
