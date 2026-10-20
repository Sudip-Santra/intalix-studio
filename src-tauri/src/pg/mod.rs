//! PostgreSQL adapter: everything specific to talking to customer databases.

pub mod connect;
pub mod decode;
pub mod errors;
pub mod introspect;
pub mod policy;
pub mod query;
pub mod tls;

#[cfg(test)]
mod integration_tests;
