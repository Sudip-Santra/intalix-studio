//! SQL language service for the editor: PostgreSQL-aware lexing, instant diagnostics,
//! completions, signature help and `{{variables}}`. Everything here is pure and offline;
//! catalog data is passed in, and server-side checks live in `pg::variables`.

pub mod analyze;
pub mod catalog;
pub mod columns;
pub mod complete;
pub mod docs;
pub mod lexer;
pub mod optional;
pub mod scope;
pub mod variables;
