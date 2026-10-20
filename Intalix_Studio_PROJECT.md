# Intalix Studio

**Product name:** Intalix Studio

**Document version:** 1.1\
**Status:** Engineering specification and implementation contract
**Product brand:** Intalix Studio\
**Target platform:** Windows desktop\
**Primary database:** PostgreSQL\
**Application model:** Local-first, single-user desktop application\
**Frontend:** React, TypeScript, Vite, ShadCN UI, Tailwind CSS\
**Desktop/runtime backend:** Rust + Tauri 2\
**Initial exports:** PDF, Excel (XLSX), CSV

------------------------------------------------------------------------

## 1. Product Vision

## Brand Identity

- **Product name:** Intalix Studio
- **Product category:** Database reporting and business analytics studio
- **Brand idea:** Intelligent analytics made practical. Intalix Studio brings database connections, SQL results, reusable report layouts, and business-ready exports into one focused desktop workspace.
- **Product descriptor:** Database Reporting & Analytics Studio
- **Brand voice:** Clear, capable, precise, professional, and approachable.
- **Naming rule:** Use **Intalix Studio** for the full product name in documentation, installer metadata, application About screens, and public-facing materials. Use **Intalix** only when a shorter label is needed in compact UI contexts.
- **Avoid:** Presenting the product as AI-powered unless AI features are explicitly designed and implemented. The name is a brand identity, not a claim that the current release includes AI.


Build **Intalix Studio**, a robust, extensible Windows desktop application that connects to
PostgreSQL databases and transforms SQL query results into consistent,
reusable, professionally formatted reports. The product combines a
secure SQL workspace, a reusable reporting engine, a visual report
designer, and export capabilities in one installable application.

The product is **reporting-first**, not a general-purpose database
administration client. Database browsing and SQL execution are
supporting capabilities. The reporting engine, report definition format,
layout system, and output correctness are the primary product assets.

The application must be useful for operational and enterprise reporting,
including financial summaries, transaction listings, reconciliation
reports, management summaries, customer activity, and other tabular
business reports. It must not assume that every customer is a bank or
that every report contains financial data.

### 1.1 Product goals

1.  Connect securely to multiple PostgreSQL environments.
2.  Let technical users author and validate SQL queries.
3.  Let report consumers run approved reports using validated parameters
    without editing SQL.
4.  Provide predefined templates and a visual report designer.
5.  Generate consistent PDF, XLSX, and CSV outputs from a shared report
    definition and normalized dataset.
6.  Handle large result sets without freezing the UI or buffering
    unbounded data in memory.
7.  Keep database credentials and query execution in the Rust backend.
8.  Work locally without a mandatory cloud service or hosted application
    backend.
9.  Provide a modular architecture that can add database adapters,
    exporters, and enterprise features later.
10. Establish security, testing, observability, and packaging practices
    from the beginning.

### 1.2 Non-goals for version 1

The first release will not include: - A hosted multi-tenant reporting
server. - Mandatory cloud accounts, telemetry, licensing servers, or
online activation. - Scheduled report execution while the application is
closed. - Email delivery or automated distribution. - Multi-user
collaboration or centralized role management. - Arbitrary plugins loaded
into the application process. - A general-purpose ETL/data
transformation platform. - Full database administration capabilities
such as schema modification, data editing, migrations of customer
databases, or query plan management. - Additional database engines
beyond PostgreSQL. - An unrestricted, freeform page-layout engine
equivalent to a full desktop publishing application.

These may be evaluated after the core reporting workflow is reliable.

------------------------------------------------------------------------

## 2. Core Product Workflow

The primary workflow is:

1.  Create or select a PostgreSQL connection.
2.  Test the connection and inspect permitted schemas, tables, views,
    and columns.
3.  Write a query or select a saved query/report.
4.  Validate the query and its parameter definitions.
5.  Execute the query with typed parameters under configured limits.
6.  Map the result fields to a report definition and template.
7.  Preview the report using the actual result data.
8.  Export to PDF, XLSX, or CSV.
9.  Review execution status and local audit history.

A report definition must separate **data retrieval** from
**presentation**. A SQL query determines which records and fields are
retrieved; the report template determines how those fields are
presented. Compatible templates should be reusable across multiple
queries.

------------------------------------------------------------------------

## 3. Architectural Principles

### 3.1 Local-first architecture

The application runs as a native Windows desktop application. The React
frontend is rendered in the Tauri webview, while Rust owns database
connectivity, secret access, query execution, report validation,
document generation, local persistence, and operating-system
integration.

No application server is required for version 1. The application
connects directly from the user's machine to PostgreSQL. The target
database must be reachable over the configured network or tunnel.

### 3.2 Strict trust boundary

-   The frontend must never connect directly to PostgreSQL.
-   Passwords and other secrets must not be placed in frontend state,
    localStorage, sessionStorage, URLs, report JSON, or logs.
-   All Tauri commands must validate their inputs in Rust.
-   Frontend validation improves usability but is not a security
    boundary.
-   Tauri capabilities must follow least privilege.
-   The report engine must not depend on React, Tauri window objects, or
    frontend state.
-   Exporters must not execute SQL or access credentials.
-   Database adapters must not own visual layout or export formatting.

### 3.3 Modularity and dependency direction

Use domain-oriented modules with explicit interfaces. Avoid circular
dependencies and avoid a single giant command module.

Preferred dependency direction:

`UI -> typed Tauri client -> Tauri commands -> application services -> domain interfaces -> infrastructure adapters`

The report engine depends on domain models and normalized report data,
not on the PostgreSQL driver or Tauri. Database adapters implement
query/data access interfaces. Exporters consume a validated render
model.

### 3.4 Correctness before visual complexity

The first end-to-end vertical slice must prove that a real PostgreSQL
query can produce correct PDF, XLSX, and CSV output. The visual designer
must be built on the report schema and renderer, not used as the source
of truth for report semantics.

### 3.5 Explicit limits

Every resource-intensive operation must have configurable or documented
limits: connection attempts, query duration, preview rows, export row
counts, output size, concurrent jobs, temporary storage, and memory
buffering. Defaults must be safe and limits must be enforced in Rust.

------------------------------------------------------------------------

## 4. Technology Stack

Choose maintained stable releases and pin versions in package lockfiles
and Cargo.lock after compatibility testing. The following are
recommended candidates, not a claim that all exact versions have been
tested together.

### 4.1 Frontend

-   React
-   TypeScript with strict compiler options
-   Vite
-   Tailwind CSS
-   ShadCN UI
-   Lucide React
-   `@tauri-apps/api`
-   TanStack Table for grid logic
-   TanStack Virtual for large grids
-   CodeMirror 6 for SQL editing
-   Zod for frontend form and DTO validation
-   React Hook Form for complex forms
-   Zustand only for genuinely shared client-side state
-   Vitest and React Testing Library for tests

Use explicit TypeScript imports where appropriate. Keep feature-specific
state close to its feature. Avoid a single global store that contains
query data, secrets, designer state, and all application settings.

### 4.2 Rust backend

Recommended candidates: - `tauri` / Tauri 2 - `tokio` - `sqlx` with
PostgreSQL and SQLite support - `serde` and `serde_json` - `uuid` -
`chrono` or `time` (choose one primary time library unless
interoperability requires both) - `rust_decimal` - `csv` -
`rust_xlsxwriter` - A vetted PDF rendering crate selected after a proof
of concept - `tracing` and a suitable subscriber - `thiserror` for
domain errors - `anyhow` for internal error context at application
boundaries where appropriate - `tempfile` - `proptest` for
property-based tests - `schemars` if Rust models are used to generate
JSON Schema - `keyring` or a narrowly scoped Windows Credential Manager
integration, after platform validation - `validator` or explicit domain
validation functions

Do not add dependencies merely because they are listed here. Confirm
maintenance status, license, Windows support, security advisories, and
required features before adoption.

### 4.3 Tooling

-   Rust stable toolchain
-   Cargo
-   Node.js LTS
-   npm, pnpm, or another single selected package manager
-   Tauri CLI
-   Windows build prerequisites required by the selected Tauri release
-   `cargo fmt`, Clippy, Rust tests
-   Frontend linting and formatting
-   CI for Windows build and test
-   Disposable PostgreSQL integration-test instance
-   Dependency and license audits

------------------------------------------------------------------------

## 5. Repository Structure

Use a monorepo. Keep the Tauri shell separate from reusable Rust domain
crates when the codebase grows.

``` text
intalix-studio/
├── PROJECT.md
├── README.md
├── CONTRIBUTING.md
├── SECURITY.md
├── package.json
├── package-lock.json
├── tsconfig.json
├── vite.config.ts
├── src/
│   ├── app/
│   │   ├── router/
│   │   ├── providers/
│   │   └── layouts/
│   ├── components/
│   │   ├── ui/                   # ShadCN components
│   │   ├── data-grid/
│   │   ├── sql-editor/
│   │   ├── report-preview/
│   │   └── common/
│   ├── features/
│   │   ├── dashboard/
│   │   ├── connections/
│   │   ├── schema-browser/
│   │   ├── query-workspace/
│   │   ├── saved-queries/
│   │   ├── report-library/
│   │   ├── report-designer/
│   │   ├── execution-history/
│   │   ├── exports/
│   │   └── settings/
│   ├── services/
│   │   └── tauri-client/
│   ├── hooks/
│   ├── stores/
│   ├── types/
│   ├── lib/
│   ├── styles/
│   └── main.tsx
├── src-tauri/
│   ├── Cargo.toml
│   ├── Cargo.lock
│   ├── tauri.conf.json
│   ├── capabilities/
│   ├── permissions/
│   ├── icons/
│   ├── migrations/
│   └── src/
│       ├── main.rs
│       ├── lib.rs
│       ├── commands/
│       ├── application/
│       ├── domain/
│       ├── database/
│       ├── credentials/
│       ├── reporting/
│       ├── exports/
│       ├── storage/
│       ├── security/
│       ├── audit/
│       ├── jobs/
│       ├── events/
│       └── errors/
├── templates/
│   └── builtin/
├── docs/
│   ├── architecture.md
│   ├── report-format.md
│   ├── security.md
│   ├── testing.md
│   └── decisions/
├── tests/
│   ├── fixtures/
│   ├── integration/
│   ├── security/
│   └── report-golden/
└── scripts/
```

### 5.1 Suggested Rust crate boundaries

As the project expands, split reusable components into workspace crates:

-   `reporting-domain`: report definitions, data types, validation
    errors, shared domain contracts.
-   `reporting-engine`: normalization, field mapping, grouping,
    aggregations, layout model, pagination contracts.
-   `reporting-db-postgres`: PostgreSQL connection, schema
    introspection, query execution and value conversion.
-   `reporting-export-csv`: CSV export.
-   `reporting-export-xlsx`: XLSX export.
-   `reporting-export-pdf`: PDF rendering.
-   `reporting-storage`: SQLite persistence and migrations.
-   `reporting-desktop`: Tauri commands, application startup, Windows
    integration, and packaging.

Do not split every file into a crate prematurely. Establish these
boundaries in code and extract crates when compilation, testing, or
dependency management benefits justify it.

------------------------------------------------------------------------

## 6. Application Modules

### 6.1 Connections

Responsibilities: - Create, edit, test, select, and remove PostgreSQL
connection profiles. - Store nonsecret connection metadata. - Store
credential references, not secrets. - Support host, port, database,
username, SSL configuration, connection timeout, and optional
application-level connection limits. - Show actionable connection errors
without exposing credentials. - Clean up active jobs and pools when a
connection is disconnected or removed.

Connection states should be explicit, such as `Disconnected`, `Testing`,
`Connected`, `Reconnecting`, and `Error`. Do not treat a saved profile
as proof that the database is currently reachable.

### 6.2 Schema browser

Responsibilities: - Discover schemas and permitted database objects
using PostgreSQL metadata. - Load object details on demand. - Display
columns, data types, nullability, keys where available, and view/table
distinction. - Cache metadata only with clear invalidation and refresh
behavior. - Respect the privileges of the connected database role.

Do not query every table's full contents just to populate the tree.
Metadata loading should be bounded and lazy.

### 6.3 SQL workspace

Responsibilities: - SQL syntax highlighting and editing. - Query tabs
and unsaved-change indicators. - Saved queries. - Typed report
parameters. - Execution progress, duration, cancellation, and errors. -
Results grid with column sorting where valid, column visibility,
resizing, copy, and bounded paging/streaming. - Clear distinction
between preview limits and full report execution.

Use a maintained SQL editor component such as CodeMirror 6. SQL syntax
highlighting is not a SQL security parser.

### 6.4 Report library

Responsibilities: - Create, open, duplicate, rename, archive, and delete
report definitions. - Store report name, description, tags, query
reference, template reference, parameter schema, version, and
timestamps. - Detect incompatible schema changes and missing fields. -
Support import/export of report definition packages with schema
validation. - Never embed credentials or connection passwords in a
report package.

### 6.5 Visual designer

Use a structured, section-based document model. Initial component
types: - Text block - Image/logo - Table - Summary/KPI block -
Horizontal divider or simple shape - Page number and report metadata -
Basic chart only after core tabular reporting is stable

Designer capabilities: - Component palette and drag/drop placement -
Section hierarchy - Field binding - Column ordering and widths - Font,
size, weight, color, alignment, border, padding, and number format -
Page size, orientation, margins, header/footer - Grouping and supported
aggregates - Repeated table headings - Undo/redo - Sample-data and
actual-data preview - Draft state, validation state, and saved version

The designer's JSON document is a source definition. The PDF/XLSX/CSV
exporters must consume validated domain models, not raw, untrusted
designer JSON.

### 6.6 Execution and export jobs

Represent long-running tasks as jobs with a stable job ID and states:
`Queued -> Running -> Succeeded | Failed | Cancelled`.

Each job should track: - Job ID and type - Start and finish time -
Progress when measurable - Cancellation token - Connection/report
references - Safe error category - Output file metadata, if applicable

Use Tauri events or channels for progress updates. The UI must handle
lost events by being able to query current job state. Do not rely on a
single progress event as the only record of job completion.

------------------------------------------------------------------------

## 7. Database Connectivity and Query Safety

### 7.1 Connection management

Use SQLx and Tokio for asynchronous PostgreSQL I/O. Use bounded pools
per active profile or a controlled pool manager. Set explicit maximum
connections, idle timeouts, acquire timeouts, and connection lifecycle
rules.

Use TLS for remote connections according to the configured policy.
Clearly distinguish secure verification modes from insecure modes. Do
not silently downgrade TLS or ignore certificate validation errors. If
certificate pinning or private CA support is required, document and test
the exact implementation.

### 7.2 Read-only reporting policy

The reporting workflow is read-only by design.

-   Recommend dedicated PostgreSQL accounts with only the required
    `CONNECT`, schema usage, and `SELECT` privileges.
-   Prefer read-only transactions where appropriate.
-   Apply statement timeouts and cancellation.
-   Restrict multi-statement execution in the reporting workflow.
-   Reject statement types outside the explicitly supported policy.
-   Validate query parameters and bind values through the database
    driver.
-   Do not concatenate user-supplied parameter values into SQL.
-   Treat SQL parsing as defense in depth, not as a replacement for
    PostgreSQL permissions.
-   Document that privileged SQL functions, database configuration, and
    permissions can affect the true behavior of a query.

The SQL editor must not silently execute DDL or DML. If an unrestricted
SQL console is ever added, it must be a separately designed and
explicitly enabled feature, not an accidental side effect of the
reporting engine.

### 7.3 Query parameters

Each parameter must define: - Stable name and identifier - Type -
Required/optional status - Default value, if safe and appropriate -
Validation constraints - UI control type - Display label and help text -
Sensitivity classification when applicable

Support common types first: text, integer, decimal, boolean, date,
timestamp, UUID, and bounded lists where binding semantics are
well-defined. Use PostgreSQL driver binding; do not implement parameters
by text substitution.

For date ranges, clearly define inclusive/exclusive boundaries and
timezone behavior. Prefer half-open intervals for timestamp ranges when
appropriate.

### 7.4 Result normalization

Create a stable typed cell model, for example: - Null - Boolean -
Signed/unsigned integer where representable - Exact decimal - Text -
Date - Time - Timestamp with or without timezone - UUID - JSON -
Binary/blob reference or explicitly bounded representation -
Unsupported/opaque value with safe display behavior

Do not force all values into JavaScript floating-point numbers.
PostgreSQL `NUMERIC`, large integers, and monetary values must retain
precision. Serialize values to the frontend using explicit string or
tagged representations where JSON numbers cannot safely preserve them.

Set explicit limits for large text and binary values in the grid.
Provide clear truncation indicators rather than silently losing data.

### 7.5 Large result sets

-   The preview grid must be virtualized.
-   Do not fetch unlimited rows by default.
-   Separate preview row limits from full export behavior.
-   Use streaming or bounded batches for large exports where supported.
-   Avoid collecting all rows in a `Vec` for a large report.
-   Set maximum concurrent exports and per-job memory budgets.
-   Support cancellation and clean resource release.
-   Define what happens if a database connection drops mid-export.
-   Use temporary files safely and remove incomplete output after
    cancellation or failure.

Do not introduce arbitrary pagination into SQL that changes query
semantics. For a query whose result must be globally ordered or
aggregated, ensure any paging strategy preserves correctness.

------------------------------------------------------------------------

## 8. Reporting Engine

### 8.1 Pipeline

The reporting engine must implement the following stages:

1.  Load and validate the versioned report definition.
2.  Resolve the selected connection reference.
3.  Validate and type-check parameter values.
4.  Execute the approved query through the database adapter.
5.  Normalize returned columns and values.
6.  Validate field bindings and required columns.
7.  Apply formatting, grouping, supported aggregations, and calculated
    presentation fields.
8.  Construct a normalized render model.
9.  Paginate for document formats or stream rows for flat-data formats.
10. Invoke the selected exporter.
11. Validate output, finalize the file, and record a safe job result.

Each stage should have explicit input/output types and test coverage.
The engine must not depend on React components or DOM measurement.

### 8.2 Versioned report definition

Store report definitions as versioned JSON. Every definition must
contain: - Schema version - Stable report ID - Name and description -
Query or saved-query reference - Parameter schema - Expected field
mappings and types where known - Template/layout definition - Export
options - Locale and timezone settings - Created/updated metadata -
Compatibility and migration metadata

Connection references are identifiers only. A report package must not
contain database credentials, saved secret values, or unredacted runtime
results.

A conceptual definition:

``` json
{
  "schemaVersion": 1,
  "id": "daily-transactions",
  "name": "Daily Transaction Report",
  "query": {
    "connectionId": "primary-postgres",
    "sql": "SELECT transaction_id, transaction_date, amount, status FROM reporting.daily_transactions WHERE transaction_date >= $1 AND transaction_date < $2",
    "parameters": [
      {"name": "start_date", "type": "date", "required": true},
      {"name": "end_date", "type": "date", "required": true}
    ]
  },
  "layout": {
    "type": "tabular",
    "pageSize": "A4",
    "orientation": "landscape",
    "repeatTableHeaders": true
  },
  "columns": [
    {"field": "transaction_id", "label": "Transaction ID"},
    {"field": "transaction_date", "label": "Transaction Date", "format": "date"},
    {"field": "amount", "label": "Amount", "format": "decimal"},
    {"field": "status", "label": "Status"}
  ],
  "exports": ["pdf", "xlsx", "csv"]
}
```

This is illustrative, not a complete production schema. Validate
definitions against a versioned JSON Schema or equivalent typed
validation. Never trust imported definitions.

### 8.3 Template model

Templates define reusable presentation rules, not database credentials
or a hardcoded source connection. Templates may define: - Page
dimensions and orientation - Margins and spacing - Typography and
colors - Header/footer sections - Table columns and widths - Repeated
headers - Grouping and supported aggregate rules - Number/date formats -
Page-break rules - Logo/image references with controlled file access -
Export-specific overrides

Templates should be independently versioned. A template must declare its
required fields or field types. Applying a template to an incompatible
dataset must produce a clear validation error rather than an empty or
malformed report.

### 8.4 Deterministic rendering

Given the same report definition, template version, dataset, locale,
timezone, and render settings, the engine should produce consistent
output. Record these inputs in execution metadata where practical.

Byte-for-byte identical PDFs may not be guaranteed if the chosen library
embeds timestamps or nondeterministic metadata. Define the intended
reproducibility level explicitly: visual/content consistency versus
binary-identical files.

### 8.5 Grouping and aggregation

Implement a limited, well-tested set first: - Count - Sum for supported
numeric values - Minimum and maximum for supported comparable values -
Group-by fields - Subtotals and grand totals

Define null behavior, decimal precision, sort order, and grouping
boundaries. Never silently convert an invalid aggregate to zero.
Unsupported aggregate/type combinations must be rejected with a useful
validation message.

------------------------------------------------------------------------

## 9. Export Architecture

Each exporter consumes the same validated render/data model but applies
format-specific rules. Do not try to force PDF, XLSX, and CSV into
identical layout behavior.

### 9.1 PDF

Requirements: - Page size, orientation, margins - Fonts and Unicode
support - Header/footer and page numbering - Repeated table headings -
Row splitting/overflow policy - Long text wrapping - Decimal/date
formatting - Page breaks and empty-report behavior - Deterministic
ordering and explicit error handling

Before choosing a PDF crate, create a proof of concept with multilingual
text, embedded fonts, wide tables, long text, multi-page output,
headers/footers, and large reports. A library that draws primitive PDF
objects may not provide the layout and pagination capabilities required
by this product. Document licensing and font redistribution rights.

### 9.2 XLSX

Use a maintained XLSX writer such as `rust_xlsxwriter`, subject to
compatibility and license review.

Requirements: - Native cell types when possible - Exact decimal
representation according to a documented Excel precision policy -
Date/time formats - Column widths - Frozen header rows - Optional
filters - Header styles and totals - Safe handling of strings beginning
with formula-like characters - Multiple sheets only when the report
definition requests them - Clear row/column limit errors

Excel has numeric precision limits. When source values exceed safe
spreadsheet precision, preserve the original representation as text or
apply a documented policy rather than silently rounding financial
values.

### 9.3 CSV

Use a CSV writer, not manual comma concatenation.

Requirements: - UTF-8 output - Correct quoting and escaping -
Configurable delimiter only if needed - Explicit header policy -
Explicit null representation - Documented date/time and decimal
formatting - Streaming for large exports - Safe handling of spreadsheet
formula injection when CSV files are likely to be opened in spreadsheet
software

CSV has no native types or formatting. Any formula-injection mitigation
must be documented because prefixing or modifying values changes their
representation. Offer a safe spreadsheet-oriented export policy rather
than silently changing raw data.

### 9.4 Export file lifecycle

-   Let the user choose the destination through an explicit save dialog.
-   Use a temporary file in an appropriate directory when finalization
    is required.
-   Write to a temporary path and atomically move/rename into the
    destination where supported.
-   Never overwrite an existing file without explicit user confirmation.
-   Remove incomplete temporary output on cancellation/failure where
    possible.
-   Do not log exported report contents.
-   Report the final path only after successful completion.

------------------------------------------------------------------------

## 10. Local Storage and Data Model

Use SQLite for application metadata and SQLx migrations for schema
evolution. Keep schema changes backward-compatible where feasible.

Suggested tables: - `connection_profiles`: nonsecret connection metadata
and secret reference. - `saved_queries`: query text, parameter
definitions, and metadata. - `report_definitions`: versioned report JSON
and metadata. - `template_definitions`: versioned template JSON and
compatibility information. - `execution_jobs`: job state and safe
execution metadata. - `audit_events`: local activity events. -
`app_settings`: nonsecret user preferences. - `schema_cache`: optional
bounded metadata cache.

Use UUIDs or another stable ID scheme. Add indexes for frequently
accessed IDs, names, timestamps, and job states. Use transactions for
multi-step updates. Validate migrations against both fresh and existing
installations.

Do not persist full query result sets by default. If result caching is
later introduced, it requires explicit retention, sensitivity,
storage-size, and deletion policies.

### 10.1 Backup and recovery

Define a supported backup/export mechanism for local definitions and
settings. Exclude secrets by default; credential migration should be an
explicit platform-aware operation. Test recovery from interrupted writes
and schema migration failures. Do not copy a live SQLite database
without using a safe backup mechanism or ensuring it is closed
consistently.

------------------------------------------------------------------------

## 11. Security Requirements

Security is a release requirement, not a future enhancement.

### 11.1 Credential management

-   Use Windows Credential Manager or a reviewed Windows-backed secret
    storage integration.
-   Persist a secret reference, not the password, in SQLite.
-   Never include secrets in logs, errors, crash reports, report
    definitions, or exported configuration.
-   Do not pass passwords through frontend storage or URLs.
-   Minimize secret copies in memory.
-   On connection removal, offer a clear choice to delete the associated
    stored credential.
-   Handle missing, revoked, or inaccessible credentials gracefully.

### 11.2 Database permissions and network security

-   Recommend dedicated least-privilege PostgreSQL roles.
-   Require explicit TLS configuration for remote production connections
    according to organizational policy.
-   Never silently disable certificate verification.
-   Apply query timeout, connection timeout, and bounded resource
    policies.
-   Document that a read-only UI is not a substitute for database-level
    permissions.
-   Avoid storing database URLs containing passwords in logs or
    configuration exports.

### 11.3 Tauri hardening

-   Use the narrowest feasible Tauri capabilities and command
    permissions.
-   Do not enable arbitrary shell or process execution for convenience.
-   Restrict filesystem access to user-selected destinations and
    application-owned directories.
-   Configure a restrictive Content Security Policy.
-   Do not load arbitrary remote pages in the privileged app window.
-   Validate all command arguments in Rust.
-   Keep dependencies and plugins to the minimum required.
-   Review updater/signing behavior before enabling automatic updates.

### 11.4 SQL policy

-   Parameter values must be bound by the database driver.
-   Do not construct SQL using string concatenation with user input.
-   Enforce the supported statement policy in Rust.
-   Restrict multiple statements in report execution.
-   Use PostgreSQL privileges and read-only transactions as layered
    controls.
-   Never claim a SQL parser alone guarantees a query is safe.

### 11.5 File handling

-   Sanitize suggested file names and avoid path traversal.
-   Require explicit save destinations.
-   Avoid following unexpected symlinks/reparse points where relevant to
    the operation.
-   Do not overwrite files silently.
-   Clean temporary files on failure.
-   Treat imported report/template files as untrusted input.
-   Limit imported file size and validate all schema fields, references,
    and asset paths.

### 11.6 Logging and privacy

-   Use structured logs with redaction.
-   Do not log passwords, tokens, full result rows, or sensitive
    parameter values.
-   Prefer query/report IDs and safe error categories.
-   Provide local log retention controls.
-   Avoid telemetry by default in version 1 unless separately designed,
    documented, and explicitly enabled.
-   Local audit history is not tamper-proof against a privileged local
    user. Do not make that claim.

### 11.7 Threat model

At minimum, review: - Malicious or malformed report definitions. - SQL
injection through parameter handling. - Privileged SQL or unsafe
database functions. - Compromised or over-permissive Tauri commands. -
Credential leakage in logs, crash reports, and exports. - Malicious file
paths or oversized imports. - Resource exhaustion from unbounded queries
and exports. - TLS misconfiguration and man-in-the-middle risk. -
Malicious dependencies and supply-chain compromise. - Unauthorized local
access to exported sensitive files.

------------------------------------------------------------------------

## 12. UI/UX Requirements

The interface must feel like a focused, high-quality Windows desktop
tool. Use ShadCN UI and Tailwind CSS with a consistent design system.

### 12.1 Main navigation

-   Overview
-   Connections
-   SQL Workspace
-   Reports
-   Templates
-   Report Designer
-   Execution History
-   Settings

### 12.2 Design direction

-   Professional enterprise visual language.
-   Clean typography, strong hierarchy, consistent spacing.
-   Light and dark themes.
-   Collapsible sidebar and compact top bar.
-   Resizable schema explorer and results grid.
-   Multi-tab SQL workspace.
-   Clear active connection/environment indicator.
-   Visible Run Query, Preview Report, and Export actions.
-   Keyboard navigation and accessible focus states.
-   Minimal animation; prioritize responsiveness.
-   Progress and cancellation for long-running jobs.
-   Empty states and actionable error messages.
-   No fake metrics or sample data presented as live results.

### 12.3 Designer layout

Use three panes: 1. Left: component palette and available data fields.
2. Center: page/report layout. 3. Right: selected component properties.

The report preview must reflect the actual render model as closely as
practical. Do not treat an HTML preview as proof that the final PDF
pagination will match. Use the same layout rules or a shared measurement
model where feasible.

------------------------------------------------------------------------

## 13. Performance and Reliability

These are initial targets to validate with representative Windows
hardware and test datasets, not pre-existing performance claims.

-   The application shell should become interactive within approximately
    two seconds on a documented reference machine, excluding first-time
    setup and unusual environment delays.
-   UI interactions must not block on database or export work.
-   Result grids must use virtualization for large datasets.
-   All long-running jobs must support cancellation where the underlying
    operation permits it.
-   Database queries and exports must use bounded concurrency.
-   Large CSV export must stream or use bounded batches.
-   XLSX and PDF memory behavior must be measured and documented.
-   Query failures must not crash the application.
-   Expected operational errors must be returned as typed errors, not
    panics.
-   Incomplete export files must not be reported as successful.
-   A connection failure must release resources and leave the
    application usable.
-   Local definitions must survive application restarts and ordinary
    export failures.

### 13.1 Resource controls

Define configurable defaults for: - Connection timeout - Query timeout -
Maximum connections per profile - Maximum concurrent jobs - Preview row
limit - Maximum import file size - Maximum layout complexity - Maximum
temporary output size where measurable - Maximum text/image dimensions -
Cancellation and cleanup behavior

Do not use one arbitrary row limit for every use case. Preview, PDF,
XLSX, and CSV have different practical constraints.

------------------------------------------------------------------------

## 14. Testing Strategy

### 14.1 Rust unit tests

Cover: - Report schema validation - Parameter type checking - PostgreSQL
value normalization - Decimal precision - Null handling -
Date/time/timezone conversion - Grouping and aggregates - Layout
validation - Pagination boundaries - CSV escaping - Export file naming -
Error redaction - Job state transitions

### 14.2 Frontend tests

Cover: - Connection form validation - Loading, error, empty, and success
states - SQL workspace interactions - Result-grid behavior - Report
field mapping - Designer undo/redo and component edits - Import
validation errors - Job progress and cancellation states - Keyboard
accessibility for critical workflows

### 14.3 Integration tests

Use a disposable PostgreSQL instance and synthetic datasets. Test: - TLS
and connection failures - Least-privilege roles - Bound parameters -
Statement timeout - Cancellation - Connection loss during export - Empty
and large result sets - Decimal, UUID, JSON, date/time, and null
values - Unsupported data types - Export to each supported format

### 14.4 Golden and regression tests

Maintain representative report fixtures for: - Single-page and
multi-page output - Wide tables - Long text - Unicode and embedded
fonts - Empty datasets - Repeated headers - Grouped totals - Negative
and high-precision numbers - Very large values and overflow - Page-break
edge cases

PDF tests may compare rendered pages or structured properties rather
than raw bytes if the library writes nondeterministic metadata.
Establish tolerances explicitly.

### 14.5 Security tests

Test: - Secrets absent from logs and serialized profiles - Rejection of
disallowed SQL statements - Query parameter binding - Tauri command
input validation - Filesystem path restrictions - Malformed and
oversized report imports - Formula injection handling - TLS certificate
failures - Cancellation cleanup - No accidental exposure of raw database
errors containing secrets

### 14.6 Windows release tests

Test on clean supported Windows installations: - Installer and
uninstall - Application data directory permissions - Credential store
behavior - Upgrade/migration path - Missing network/database - Export
destination permission failures - Paths containing spaces and non-ASCII
characters - Recovery after interrupted export - Code signing and
installer trust experience

------------------------------------------------------------------------

## 15. CI/CD and Engineering Quality

Every change should pass the applicable checks before merge.

### Frontend checks

-   Install from the lockfile
-   TypeScript typecheck
-   Lint
-   Unit/component tests
-   Production build

### Rust checks

-   `cargo fmt --check`
-   Clippy with project-approved warnings policy
-   Unit tests
-   Integration tests where infrastructure is available
-   Dependency/security audit
-   Release build for Windows

### Engineering rules

-   Keep `Cargo.lock` and the frontend lockfile committed for the
    application.
-   Use explicit error types at module boundaries.
-   Avoid `unwrap()` and `expect()` in user-controlled or operational
    paths.
-   Never silently swallow errors.
-   Keep command handlers thin; put business logic in application
    services.
-   Add a regression test for each fixed bug where feasible.
-   Use structured logs with redaction.
-   Document architecture decisions that affect compatibility, security,
    or file formats.
-   Review dependency licenses before shipping.
-   Use semantic versioning for application releases and explicit
    versions for report schemas.
-   Do not make a release depend on an undocumented developer machine
    configuration.

------------------------------------------------------------------------

## 16. Error Handling and Observability

Use typed domain errors such as: - `ConnectionFailed` -
`AuthenticationFailed` - `TlsConfigurationError` - `QueryRejected` -
`QueryTimedOut` - `QueryCancelled` - `ParameterValidationFailed` -
`FieldMappingFailed` - `TemplateValidationFailed` - `ExportFailed` -
`StorageFailed` - `PermissionDenied` - `UnsupportedDataType`

Map internal errors to safe user-facing messages. Keep detailed internal
context in redacted logs, not in frontend error strings. Do not expose
connection strings, secrets, filesystem details, or sensitive SQL
parameters.

Use tracing spans around connection attempts, query jobs, report
rendering, and exports. Correlate events using job IDs. Logs must be
useful for diagnosis without storing complete datasets.

------------------------------------------------------------------------

## 17. Implementation Phases

Timelines below are rough planning estimates for one experienced
developer and may change after the PDF and large-result-set proof of
concept.

### Phase 0 --- Technical spikes

**Deliverables** - Verify Tauri 2 setup and Windows build. - Prove
PostgreSQL connection and typed query execution. - Test secure
credential storage on Windows. - Generate a basic PDF with Unicode text
and multiple pages. - Generate an XLSX with native cell types and
exact-value policy. - Stream a CSV export. - Document selected library
licenses and limitations.

**Exit criteria:** the critical third-party dependencies have been
tested against representative requirements before large-scale
implementation.

### Phase 1 --- Application foundation

**Deliverables** - Repository structure and build pipeline. - Navigation
shell, theme, design tokens, and common components. - Tauri command
boundary and typed frontend client. - SQLite schema and migrations. -
Settings and error-handling conventions. - Windows development and
release build instructions.

**Exit criteria:** a clean installation launches, persists nonsecret
settings, and passes baseline checks.

### Phase 2 --- PostgreSQL workspace

**Deliverables** - Connection profiles and credential integration. -
Connection test, SSL options, schema browser. - SQL editor and saved
queries. - Parameter binding and read-only policy. - Virtualized result
grid. - Query cancellation, timeouts, and typed value conversion.

**Exit criteria:** a user can securely connect, inspect metadata,
execute supported queries, and handle failures without a crash.

### Phase 3 --- Reporting core

**Deliverables** - Versioned report schema. - Report definition
validation. - Normalized data model. - Field mapping and formatting. -
Basic grouping and aggregations. - PDF/XLSX/CSV exporters. - Built-in
templates and basic preview.

**Exit criteria:** the same report definition and dataset produce valid
output in all three formats with documented type and formatting
behavior.

### Phase 4 --- Visual designer

**Deliverables** - Three-pane designer. - Component palette and field
mapping. - Tables, text, logos, summary blocks, page configuration. -
Styling, column sizing, headers/footers. - Undo/redo, validation, save
and duplicate. - Actual-data preview and template compatibility checks.

**Exit criteria:** users can build and save useful reports without
hardcoded UI or editing JSON manually.

### Phase 5 --- Performance, security, and release hardening

**Deliverables** - Large dataset benchmarks. - PDF layout regression
suite. - Credential and Tauri security review. - Audit history and safe
logging. - Backup/recovery. - Signed installer and clean-machine
tests. - User documentation and troubleshooting guide.

**Exit criteria:** release acceptance criteria pass on documented
Windows environments and representative PostgreSQL datasets.

------------------------------------------------------------------------

## 18. Definition of Done

A feature is complete only when: - Its behavior and failure states are
documented. - Inputs are validated in the correct trust boundary. -
Relevant unit/integration tests exist. - Errors are actionable and
secrets are redacted. - Long-running work does not block the UI. -
Cancellation and cleanup are handled where applicable. - Accessibility
and keyboard interactions are considered. - No unbounded memory or
concurrency behavior is introduced without explicit justification. -
Documentation and migration behavior are updated. - CI checks pass.

A release is ready only when the critical end-to-end workflow has been
tested on a clean supported Windows machine.

------------------------------------------------------------------------

## 19. Version 1 Acceptance Checklist

### Connections and SQL

-   [ ] Multiple PostgreSQL profiles can be saved.
-   [ ] Credentials are not persisted in plain-text metadata.
-   [ ] TLS configuration and certificate failures are handled
    correctly.
-   [ ] Schema browsing respects database privileges.
-   [ ] SQL parameters are bound, not interpolated.
-   [ ] Disallowed query types are rejected by backend policy.
-   [ ] Query cancellation and timeouts work.
-   [ ] Large values and unsupported types have defined behavior.

### Reporting

-   [ ] Report definitions are versioned and validated.
-   [ ] Queries and templates are reusable independently.
-   [ ] Required field mappings are checked before rendering.
-   [ ] Grouping, totals, and null behavior are defined.
-   [ ] PDF pagination and font behavior pass regression fixtures.
-   [ ] XLSX preserves supported types and follows the precision policy.
-   [ ] CSV escaping, UTF-8, and formula-injection policy are tested.
-   [ ] Large exports use bounded resource strategies.
-   [ ] Failed/cancelled exports are not marked successful.

### Designer and application

-   [ ] Templates can be created, cloned, saved, and reopened.
-   [ ] Designer edits persist and undo/redo works for supported
    operations.
-   [ ] UI shows clear progress, errors, and cancellation state.
-   [ ] Local data migrations are tested.
-   [ ] Import/export of definitions excludes credentials.
-   [ ] Application works without an application server.
-   [ ] Windows installer, upgrade, and uninstall are tested.
-   [ ] Security review and dependency audit are complete.

------------------------------------------------------------------------

## 20. Future Extensibility

Design the system so future work can add capabilities without
destabilizing the core.

### Additional database adapters

Define a narrow database adapter interface for connection lifecycle,
metadata discovery, parameterized query execution, and normalized result
batches. PostgreSQL-specific SQL and type conversion must remain in the
PostgreSQL adapter.

Do not prematurely build a generic abstraction that hides important
driver behavior. Add another database only when its type system,
parameter model, TLS behavior, and query policy can be tested properly.

### Additional exporters

Exporters should consume the stable report/render model. New formats
must not require rewriting SQL execution or the visual designer.

### Centralized enterprise capabilities

If centralized template distribution, scheduled execution, team access
control, or centralized audit storage is required, design it as a
separate optional service with explicit authentication and
authorization. Do not pretend a local desktop application alone can
provide trustworthy organization-wide access control or tamper-evident
audit.

### Report format compatibility

Maintain explicit schema versions and migration functions. Imported
report definitions must be validated and migrated before use. Keep
example fixtures from earlier schema versions to test compatibility.

------------------------------------------------------------------------

## 21. Key Architecture Decisions

1.  **Rust owns data access and export generation.** The frontend must
    not receive credentials or open database connections directly.
2.  **The reporting engine is independent of Tauri.** It can be
    unit-tested and potentially reused by future tooling.
3.  **PostgreSQL is the first and only database adapter in version 1.**
4.  **SQLite stores local application metadata.** Customer database
    schemas are not modified.
5.  **Windows Credential Manager stores database secrets.** Local
    profile records contain references only.
6.  **Report definitions and templates are versioned JSON.** They are
    validated, portable, and free of secrets.
7.  **PDF, XLSX, and CSV are separate exporters.** They share data and
    domain semantics but retain format-specific behavior.
8.  **Read-only reporting is enforced in layers.** Database privileges
    are primary; application validation is defense in depth.
9.  **Large data handling is bounded.** Preview, full export, and layout
    rendering have separate resource strategies.
10. **The visual designer follows the report schema.** It does not
    define a separate, incompatible representation of report behavior.
11. **No mandatory cloud or backend service exists in version 1.**
12. **Enterprise claims require evidence.** Security, performance,
    reliability, and compatibility must be demonstrated by tests and
    documented deployment guidance.

------------------------------------------------------------------------

## 22. Instructions for AI Coding Agents and Contributors

When implementing this project, follow these instructions:

-   Read this `PROJECT.md` before making architectural changes.
-   Inspect the existing repository and current implementation before
    creating or replacing files.
-   Implement one phase and one vertical slice at a time.
-   Do not generate the entire application in one unreviewable change.
-   Do not replace real backend behavior with hardcoded mock data and
    then claim the feature is complete.
-   Keep mock data clearly identified and isolated to development/demo
    mode.
-   Do not expose database passwords or secrets to the frontend.
-   Do not bypass Rust validation by trusting frontend checks.
-   Do not use SQL string interpolation for parameter values.
-   Do not use unbounded result buffering for large exports.
-   Do not introduce additional database engines, a cloud service,
    authentication server, or scheduled jobs into version 1 without an
    explicit scope decision.
-   Do not add a dependency without checking maintenance, license,
    platform support, and security posture.
-   Write tests for domain logic and important failure cases.
-   Keep command handlers thin and business logic in application
    services.
-   Maintain typed errors, structured redacted logs, and cancellation
    behavior.
-   Update documentation and report schema versions when behavior
    changes.
-   If a requirement conflicts with this specification, explain the
    tradeoff and propose an explicit decision rather than silently
    changing the architecture.
-   Report what is implemented, what is tested, what remains incomplete,
    and any known limitations after each phase.

------------------------------------------------------------------------

## 23. Final Product Standard

The objective is not merely to produce a database connection window and
a PDF button. The objective is to build a dependable reporting product
whose core engine can be trusted with real-world datasets.

The project should be judged by: - Correctness of generated reports. -
Preservation of database value semantics and numeric precision. -
Reliability with large and malformed datasets. - Security of credentials
and exported data. - Predictability of report layouts and pagination. -
Ease of reusing templates across queries. - Clear separation of UI,
database, reporting, and export responsibilities. - Maintainability of
the codebase and report format. - A polished, accessible, responsive
Windows desktop experience.

**Build the database-to-report vertical slice first, establish the
report schema and rendering contracts, and then grow the designer on top
of that foundation.**

------------------------------------------------------------------------

## 24. Decisions and Progress Log

This section records decisions made after version 1.1 of this document
and the features built so far. Where it differs from earlier sections,
this section wins.

### 24.1 Scope and technology decisions (2026-10-19)

-   **Users:** SQL authors and business users use the same application
    from day one. There is no separate "runner" mode. Saved reports
    with parameters give non-SQL users a simple run form.
-   **Version 1 report:** the query output rendered as a formatted
    table (title, header/footer, column formats, totals). AI-generated
    charts and visuals come later; the data model must stay ready for
    them.
-   **Visual designer:** still to be confirmed. The recommendation is a
    report settings panel for version 1 instead of a drag-and-drop
    designer.
-   **Read-only:** the database explorer and SQL workspace stay
    read-only. pgAdmin-style schema and data changes (create, alter,
    drop, truncate, insert, update, delete) are out of scope for
    version 1. They may return later as a separate, guarded admin mode
    that is off by default and never available on Production
    connections.
-   **Scheduling:** later, not version 1.
-   **PDF limit:** about 500 pages. Larger outputs are directed to XLSX
    or CSV.
-   **PDF engine:** Typst, embedded as a Rust library (confirm with a
    Phase 0 spike).
-   **PostgreSQL driver:** `tokio-postgres` for customer databases,
    because it supports server-side query cancellation. `sqlx` stays
    for the local SQLite store.
-   **Licensing:** likely open source; no pricing or licensing work
    now.
-   **Build order:** the UI is built first, feature by feature, against
    a typed client interface backed by a clearly labelled simulated
    backend. The Rust backend then implements the same interface.

### 24.2 Design system

-   Sharp corners everywhere (no border radius), compact controls
    (32 px buttons and inputs), 13 px base text.
-   Light and dark themes; dark is the default.
-   Accent colour comes from CSS variables under `data-accent`. The
    default accent is orange; a Settings page will offer others.
-   Fonts are bundled locally (Inter, JetBrains Mono); nothing is loaded
    from the internet.
-   Environment colours: Development blue, Staging amber, Production
    red. A Production connection shows a red rule across the window
    and a PRODUCTION badge.

-   **Layout (2026-10-19):** no top/breadcrumb bar. In the workspace the
    active connection lives in the status bar (environment, target, ⇅);
    clicking it opens Switch Connection / Manage Connections /
    Disconnect. The light/dark toggle sits at the bottom right next to
    Read-only (top right on the Connections screen). Settings is not
    shown until it has content.
-   **Saved connections:** clicking a connection opens its workspace; the
    ⋮ menu offers Enter Workspace and Delete (with confirmation; deletes
    the profile, stored password and pooled connections).
-   **Text style:** headings and labels use normal title case without
    letter spacing; light theme is white throughout.
-   **Tooltips:** icon-only buttons get tooltips (150 ms delay, short
    fade/slide); the theme toggle's tooltip is instant.
-   **Window:** fitted to the monitor's work area on startup (never under
    the taskbar); minimum size 900×560; panels shrink instead of clipping.

-   **Accent colour:** a Color Theme button (status bar, and top right on
    the Connections screen) opens presets plus a custom picker / hex
    field. The colour is derived per theme by `src/app/accent.ts`, with
    lightness adjusted so text and buttons keep 4.5:1 contrast; orange
    stays the default. Environment colours never follow the accent.

### 24.3 Built so far

| Feature | Status |
|---|---|
| Project setup: React, TypeScript (strict), Vite, Tailwind, Radix/ShadCN-style components, Prettier | Done |
| Tauri 2 desktop shell: strict CSP, least-privilege capabilities, Windows installer targets, strict Rust lints | Done |
| Connect screen: validated form, environment picker, SSL mode guidance, connection-string import, cancellable test/save, saved connections | UI done, simulated backend |
| Workspace: schema explorer (schemas, tables, views, materialized views, functions, sequences, columns, keys), filter, keyboard navigation | UI done, simulated backend |
| Tabs: multiple query and table tabs, shortcuts (Ctrl+T, Ctrl+W, Ctrl+Tab), unsaved-query confirmation | UI done |
| Table view: Columns, Data preview, Indexes, Constraints, DDL | UI done, simulated backend |
| SQL workspace: editor with PostgreSQL syntax and schema autocomplete, run selection, cancel, row limit, typed result grid, read-only statement guard | UI done, simulated backend |
| Right-click menus (pgAdmin-style, read-only items) on explorer, tabs and result grid; browser menu suppressed | UI done |
| Real backend: connections, credential storage, schema introspection, read-only query execution, cancellation | Done, tested against PostgreSQL 17 |
| Command allow-list: the window may call only the 13 declared backend commands | Done |

### 24.4 Notes for the backend

-   Query results use a typed cell model (`Cell` in
    `src/services/client/types.ts`). Exact numerics travel as strings
    and must never be converted to floating point.
-   The simulated read-only guard accepts only one statement per run,
    of type SELECT, WITH (without data-modifying statements), VALUES,
    TABLE, SHOW or EXPLAIN. The Rust backend must enforce the same rule
    and also run queries in a `READ ONLY` transaction with a statement
    timeout.
-   Row-count figures in the explorer are planner estimates and must be
    labelled as such.

### 24.5 Backend architecture (as built)

```text
src-tauri/src/
├── lib.rs           startup: logging, credential store, SQLite, state, command registration
├── commands.rs      thin Tauri commands (the only UI entry points)
├── service.rs       application services; validation at the trust boundary
├── model.rs         DTOs shared with the UI (camelCase, mirrors types.ts)
├── error.rs         typed, user-safe errors with stable categories
├── jobs.rs          cancellable request registry
├── secrets.rs       Windows Credential Manager (Local persistence) + session-only secrets
├── storage.rs       SQLite with versioned migrations (src-tauri/migrations)
└── pg/
    ├── connect.rs   driver config, connection test, per-connection bounded pools
    ├── tls.rs       libpq-equivalent SSL modes; Windows trust store or CA file
    ├── policy.rs    lexer-based read-only statement policy
    ├── query.rs     READ ONLY execution, timeouts, batched fetch, row and memory limits
    ├── decode.rs    exact binary decoding of PostgreSQL values
    ├── introspect.rs catalog queries and DDL reconstruction (PostgreSQL 12+)
    └── errors.rs    driver error to user-safe error mapping
```

Key properties:

-   **Read-only in four layers:** statement policy, single-statement
    extended protocol, `READ ONLY` transaction with `statement_timeout`,
    and database role privileges (the primary control).
-   **Secrets:** passwords live in Windows Credential Manager (Local
    persistence, no roaming) or in memory for the session; never in
    SQLite, logs, errors or UI responses.
-   **TLS:** `verify-full` and `verify-ca` fail closed; `prefer` and
    `require` match libpq (encrypt without verification) and the UI
    warns about them.
-   **Limits:** per-connection pool size, 30 s pool wait, row limit up to
    50,000, 32 MB result budget, 64 KB per cell, 30 s catalog timeout,
    capped catalog result sizes.
-   **PgBouncer:** no startup options are sent. Named prepared
    statements require PgBouncer 1.21 or newer in transaction mode.
-   **Timestamps** with time zone are currently shown in UTC (`+00`);
    a display-time-zone setting is planned.

### 24.6 Running the tests

-   Frontend: `pnpm typecheck`, `pnpm build`.
-   Backend unit tests and lints (in `src-tauri`): `cargo test`,
    `cargo clippy --all-targets -- -D warnings`.
-   PostgreSQL integration tests run when `INTALIX_TEST_PG` holds a
    libpq connection string for a role that can create schemas, for
    example `host=localhost port=5432 user=postgres password=... dbname=postgres`.
    They create and drop a schema named `intalix_it` and touch nothing
    else.
