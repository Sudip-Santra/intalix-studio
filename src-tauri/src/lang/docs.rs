//! Built-in PostgreSQL knowledge for hints: keywords, functions with signatures and short
//! descriptions, and data type names. Covers what report queries use day to day; functions
//! in the user's own schemas come from the database catalog instead.

/// How a built-in function is used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FnKind {
    Scalar,
    Aggregate,
    Window,
    /// Written without parentheses, e.g. `current_date`.
    Constant,
}

pub struct FnDoc {
    pub name: &'static str,
    pub kind: FnKind,
    /// One entry per overload, written as `name(arg type, …) → result`.
    pub signatures: &'static [&'static str],
    pub description: &'static str,
}

macro_rules! f {
    ($name:literal, $kind:ident, [$($sig:literal),+ $(,)?], $desc:literal) => {
        FnDoc { name: $name, kind: FnKind::$kind, signatures: &[$($sig),+], description: $desc }
    };
}

pub static FUNCTIONS: &[FnDoc] = &[
    // ----------------------------------------------------------------- aggregates
    f!(
        "count",
        Aggregate,
        ["count(*) → bigint", "count(value any) → bigint"],
        "Number of rows, or of non-null values."
    ),
    f!(
        "sum",
        Aggregate,
        ["sum(value numeric) → numeric"],
        "Sum of the non-null values."
    ),
    f!(
        "avg",
        Aggregate,
        ["avg(value numeric) → numeric"],
        "Average of the non-null values."
    ),
    f!(
        "min",
        Aggregate,
        ["min(value any) → same type"],
        "Smallest non-null value."
    ),
    f!(
        "max",
        Aggregate,
        ["max(value any) → same type"],
        "Largest non-null value."
    ),
    f!(
        "string_agg",
        Aggregate,
        ["string_agg(value text, delimiter text) → text"],
        "Joins the values into one text, separated by the delimiter. Add ORDER BY inside the call to fix the order."
    ),
    f!(
        "array_agg",
        Aggregate,
        ["array_agg(value any) → array"],
        "Collects the values into an array."
    ),
    f!(
        "json_agg",
        Aggregate,
        ["json_agg(value any) → json"],
        "Collects the values into a JSON array."
    ),
    f!(
        "jsonb_agg",
        Aggregate,
        ["jsonb_agg(value any) → jsonb"],
        "Collects the values into a JSONB array."
    ),
    f!(
        "jsonb_object_agg",
        Aggregate,
        ["jsonb_object_agg(key text, value any) → jsonb"],
        "Builds a JSONB object from key/value pairs."
    ),
    f!(
        "bool_and",
        Aggregate,
        ["bool_and(value boolean) → boolean"],
        "True if every non-null value is true."
    ),
    f!(
        "bool_or",
        Aggregate,
        ["bool_or(value boolean) → boolean"],
        "True if at least one value is true."
    ),
    f!(
        "stddev",
        Aggregate,
        ["stddev(value numeric) → numeric"],
        "Sample standard deviation."
    ),
    f!(
        "variance",
        Aggregate,
        ["variance(value numeric) → numeric"],
        "Sample variance."
    ),
    f!(
        "percentile_cont",
        Aggregate,
        [
            "percentile_cont(fraction double precision) WITHIN GROUP (ORDER BY value) → double precision"
        ],
        "Continuous percentile, e.g. 0.5 for the median (interpolated)."
    ),
    f!(
        "percentile_disc",
        Aggregate,
        ["percentile_disc(fraction double precision) WITHIN GROUP (ORDER BY value) → same type"],
        "Discrete percentile: the first value at or above the fraction."
    ),
    f!(
        "mode",
        Aggregate,
        ["mode() WITHIN GROUP (ORDER BY value) → same type"],
        "The most frequent value."
    ),
    // ----------------------------------------------------------------- window
    f!(
        "row_number",
        Window,
        ["row_number() OVER (…) → bigint"],
        "Number of the current row within its partition, from 1."
    ),
    f!(
        "rank",
        Window,
        ["rank() OVER (…) → bigint"],
        "Rank with gaps: ties share a rank and the next rank is skipped."
    ),
    f!(
        "dense_rank",
        Window,
        ["dense_rank() OVER (…) → bigint"],
        "Rank without gaps."
    ),
    f!(
        "percent_rank",
        Window,
        ["percent_rank() OVER (…) → double precision"],
        "Relative rank from 0 to 1."
    ),
    f!(
        "ntile",
        Window,
        ["ntile(buckets integer) OVER (…) → integer"],
        "Splits the partition into the given number of groups."
    ),
    f!(
        "lag",
        Window,
        ["lag(value any, offset integer = 1, default any = NULL) OVER (…) → same type"],
        "Value from a row before the current row."
    ),
    f!(
        "lead",
        Window,
        ["lead(value any, offset integer = 1, default any = NULL) OVER (…) → same type"],
        "Value from a row after the current row."
    ),
    f!(
        "first_value",
        Window,
        ["first_value(value any) OVER (…) → same type"],
        "Value from the first row of the window frame."
    ),
    f!(
        "last_value",
        Window,
        ["last_value(value any) OVER (…) → same type"],
        "Value from the last row of the window frame."
    ),
    f!(
        "nth_value",
        Window,
        ["nth_value(value any, n integer) OVER (…) → same type"],
        "Value from the n-th row of the window frame."
    ),
    // ----------------------------------------------------------------- conditional
    f!(
        "coalesce",
        Scalar,
        ["coalesce(value any, …) → same type"],
        "First argument that is not null."
    ),
    f!(
        "nullif",
        Scalar,
        ["nullif(value1 any, value2 any) → same type"],
        "NULL if both values are equal, otherwise value1. Handy to avoid division by zero."
    ),
    f!(
        "greatest",
        Scalar,
        ["greatest(value any, …) → same type"],
        "Largest of the arguments, ignoring nulls."
    ),
    f!(
        "least",
        Scalar,
        ["least(value any, …) → same type"],
        "Smallest of the arguments, ignoring nulls."
    ),
    // ----------------------------------------------------------------- strings
    f!(
        "length",
        Scalar,
        ["length(text text) → integer"],
        "Number of characters."
    ),
    f!(
        "lower",
        Scalar,
        ["lower(text text) → text"],
        "Converts to lower case."
    ),
    f!(
        "upper",
        Scalar,
        ["upper(text text) → text"],
        "Converts to upper case."
    ),
    f!(
        "initcap",
        Scalar,
        ["initcap(text text) → text"],
        "Capitalises the first letter of each word."
    ),
    f!(
        "trim",
        Scalar,
        ["trim([LEADING | TRAILING | BOTH] [characters text FROM] text text) → text"],
        "Removes spaces (or the given characters) from the ends."
    ),
    f!(
        "btrim",
        Scalar,
        ["btrim(text text, characters text = ' ') → text"],
        "Removes the characters from both ends."
    ),
    f!(
        "ltrim",
        Scalar,
        ["ltrim(text text, characters text = ' ') → text"],
        "Removes the characters from the start."
    ),
    f!(
        "rtrim",
        Scalar,
        ["rtrim(text text, characters text = ' ') → text"],
        "Removes the characters from the end."
    ),
    f!(
        "substring",
        Scalar,
        [
            "substring(text text FROM start integer FOR count integer) → text",
            "substring(text text, start integer, count integer) → text"
        ],
        "Part of the text, counting from 1."
    ),
    f!(
        "left",
        Scalar,
        ["left(text text, n integer) → text"],
        "First n characters (all but the last |n| when negative)."
    ),
    f!(
        "right",
        Scalar,
        ["right(text text, n integer) → text"],
        "Last n characters."
    ),
    f!(
        "replace",
        Scalar,
        ["replace(text text, from text, to text) → text"],
        "Replaces every occurrence of from with to."
    ),
    f!(
        "concat",
        Scalar,
        ["concat(value any, …) → text"],
        "Joins the values as text, ignoring nulls."
    ),
    f!(
        "concat_ws",
        Scalar,
        ["concat_ws(separator text, value any, …) → text"],
        "Joins the values with a separator, ignoring nulls."
    ),
    f!(
        "split_part",
        Scalar,
        ["split_part(text text, delimiter text, n integer) → text"],
        "The n-th field after splitting on the delimiter."
    ),
    f!(
        "position",
        Scalar,
        ["position(substring text IN text text) → integer"],
        "Where the substring starts (1-based), or 0."
    ),
    f!(
        "strpos",
        Scalar,
        ["strpos(text text, substring text) → integer"],
        "Where the substring starts (1-based), or 0."
    ),
    f!(
        "lpad",
        Scalar,
        ["lpad(text text, length integer, fill text = ' ') → text"],
        "Pads on the left to the given length."
    ),
    f!(
        "rpad",
        Scalar,
        ["rpad(text text, length integer, fill text = ' ') → text"],
        "Pads on the right to the given length."
    ),
    f!(
        "repeat",
        Scalar,
        ["repeat(text text, n integer) → text"],
        "Repeats the text n times."
    ),
    f!(
        "reverse",
        Scalar,
        ["reverse(text text) → text"],
        "Reverses the characters."
    ),
    f!(
        "format",
        Scalar,
        ["format(format text, value any, …) → text"],
        "printf-style formatting: %s text, %I identifier, %L literal."
    ),
    f!(
        "regexp_replace",
        Scalar,
        ["regexp_replace(text text, pattern text, replacement text, flags text = '') → text"],
        "Replaces matches of a regular expression. Use flag 'g' for all matches."
    ),
    f!(
        "regexp_match",
        Scalar,
        ["regexp_match(text text, pattern text, flags text = '') → text[]"],
        "Captured groups of the first match."
    ),
    f!(
        "starts_with",
        Scalar,
        ["starts_with(text text, prefix text) → boolean"],
        "True if the text starts with the prefix."
    ),
    f!(
        "md5",
        Scalar,
        ["md5(text text) → text"],
        "MD5 hash as hexadecimal."
    ),
    f!(
        "to_char",
        Scalar,
        [
            "to_char(value timestamp, format text) → text",
            "to_char(value numeric, format text) → text"
        ],
        "Formats a date or number as text, e.g. to_char(d, 'DD Mon YYYY') or to_char(n, 'FM999,999.00')."
    ),
    // ----------------------------------------------------------------- numbers
    f!(
        "round",
        Scalar,
        [
            "round(value numeric) → numeric",
            "round(value numeric, digits integer) → numeric"
        ],
        "Rounds to the nearest integer or to the given decimal places."
    ),
    f!(
        "trunc",
        Scalar,
        ["trunc(value numeric, digits integer = 0) → numeric"],
        "Truncates towards zero."
    ),
    f!(
        "ceil",
        Scalar,
        ["ceil(value numeric) → numeric"],
        "Smallest integer not less than the value."
    ),
    f!(
        "floor",
        Scalar,
        ["floor(value numeric) → numeric"],
        "Largest integer not greater than the value."
    ),
    f!(
        "abs",
        Scalar,
        ["abs(value numeric) → numeric"],
        "Absolute value."
    ),
    f!(
        "mod",
        Scalar,
        ["mod(y numeric, x numeric) → numeric"],
        "Remainder of y / x."
    ),
    f!(
        "power",
        Scalar,
        ["power(a numeric, b numeric) → numeric"],
        "a raised to the power b."
    ),
    f!(
        "sqrt",
        Scalar,
        ["sqrt(value numeric) → numeric"],
        "Square root."
    ),
    f!(
        "random",
        Scalar,
        ["random() → double precision"],
        "Random value between 0 and 1."
    ),
    f!(
        "to_number",
        Scalar,
        ["to_number(text text, format text) → numeric"],
        "Parses text into a number using a format."
    ),
    f!(
        "width_bucket",
        Scalar,
        ["width_bucket(value numeric, low numeric, high numeric, count integer) → integer"],
        "Bucket number of the value in a histogram of equal-width buckets."
    ),
    // ----------------------------------------------------------------- dates and times
    f!(
        "now",
        Scalar,
        ["now() → timestamp with time zone"],
        "Start time of the current transaction."
    ),
    f!(
        "current_date",
        Constant,
        ["current_date → date"],
        "Today's date (in the session time zone)."
    ),
    f!(
        "current_timestamp",
        Constant,
        ["current_timestamp → timestamp with time zone"],
        "Start time of the current transaction."
    ),
    f!(
        "localtimestamp",
        Constant,
        ["localtimestamp → timestamp"],
        "Current date and time without time zone."
    ),
    f!(
        "date_trunc",
        Scalar,
        [
            "date_trunc(field text, source timestamp) → timestamp",
            "date_trunc(field text, source timestamptz, time_zone text) → timestamptz"
        ],
        "Truncates to the start of 'year', 'quarter', 'month', 'week', 'day', 'hour' …"
    ),
    f!(
        "date_part",
        Scalar,
        ["date_part(field text, source timestamp) → double precision"],
        "A field of a date, e.g. date_part('year', d). EXTRACT is the standard form."
    ),
    f!(
        "extract",
        Scalar,
        ["extract(field FROM source timestamp) → numeric"],
        "A field of a date or interval: YEAR, QUARTER, MONTH, DAY, DOW, EPOCH …"
    ),
    f!(
        "age",
        Scalar,
        [
            "age(end timestamp, start timestamp) → interval",
            "age(value timestamp) → interval"
        ],
        "Difference as years, months and days."
    ),
    f!(
        "make_date",
        Scalar,
        ["make_date(year integer, month integer, day integer) → date"],
        "Builds a date from its parts."
    ),
    f!(
        "make_interval",
        Scalar,
        [
            "make_interval(years integer = 0, months integer = 0, weeks integer = 0, days integer = 0, hours integer = 0, mins integer = 0, secs double precision = 0) → interval"
        ],
        "Builds an interval from its parts."
    ),
    f!(
        "to_date",
        Scalar,
        ["to_date(text text, format text) → date"],
        "Parses text into a date, e.g. to_date('31/01/2026', 'DD/MM/YYYY')."
    ),
    f!(
        "to_timestamp",
        Scalar,
        [
            "to_timestamp(text text, format text) → timestamptz",
            "to_timestamp(epoch double precision) → timestamptz"
        ],
        "Parses text, or Unix seconds, into a timestamp."
    ),
    f!(
        "generate_series",
        Scalar,
        [
            "generate_series(start integer, stop integer, step integer = 1) → setof integer",
            "generate_series(start timestamp, stop timestamp, step interval) → setof timestamp"
        ],
        "A row for each value from start to stop. Good for filling gaps in date series."
    ),
    f!(
        "timezone",
        Scalar,
        ["timezone(zone text, value timestamptz) → timestamp"],
        "Converts to local time in the zone. Same as value AT TIME ZONE zone."
    ),
    // ----------------------------------------------------------------- json
    f!(
        "jsonb_build_object",
        Scalar,
        ["jsonb_build_object(key text, value any, …) → jsonb"],
        "Builds a JSONB object from alternating keys and values."
    ),
    f!(
        "json_build_object",
        Scalar,
        ["json_build_object(key text, value any, …) → json"],
        "Builds a JSON object from alternating keys and values."
    ),
    f!(
        "jsonb_array_elements",
        Scalar,
        ["jsonb_array_elements(value jsonb) → setof jsonb"],
        "One row per element of a JSONB array."
    ),
    f!(
        "jsonb_each",
        Scalar,
        ["jsonb_each(value jsonb) → setof (key text, value jsonb)"],
        "One row per key of a JSONB object."
    ),
    f!(
        "jsonb_extract_path_text",
        Scalar,
        ["jsonb_extract_path_text(value jsonb, path text, …) → text"],
        "Text at the given path. Same as value #>> '{a,b}'."
    ),
    f!(
        "jsonb_typeof",
        Scalar,
        ["jsonb_typeof(value jsonb) → text"],
        "Type of the JSON value: object, array, string, number, boolean or null."
    ),
    f!(
        "to_jsonb",
        Scalar,
        ["to_jsonb(value any) → jsonb"],
        "Converts any value to JSONB."
    ),
    // ----------------------------------------------------------------- arrays
    f!(
        "array_length",
        Scalar,
        ["array_length(array anyarray, dimension integer) → integer"],
        "Length of the given array dimension (usually 1)."
    ),
    f!(
        "cardinality",
        Scalar,
        ["cardinality(array anyarray) → integer"],
        "Total number of elements."
    ),
    f!(
        "unnest",
        Scalar,
        ["unnest(array anyarray) → setof element"],
        "One row per array element."
    ),
    f!(
        "array_to_string",
        Scalar,
        ["array_to_string(array anyarray, delimiter text, null_text text = NULL) → text"],
        "Joins the elements into text."
    ),
    f!(
        "string_to_array",
        Scalar,
        ["string_to_array(text text, delimiter text) → text[]"],
        "Splits text into an array."
    ),
    f!(
        "array_position",
        Scalar,
        ["array_position(array anyarray, value any) → integer"],
        "Index of the first matching element."
    ),
    // ----------------------------------------------------------------- misc
    f!(
        "cast",
        Scalar,
        ["cast(value AS type) → type"],
        "Converts a value to another type. Same as value::type."
    ),
    f!(
        "current_user",
        Constant,
        ["current_user → name"],
        "The role running the query."
    ),
    f!(
        "current_database",
        Scalar,
        ["current_database() → name"],
        "Name of the current database."
    ),
    f!(
        "version",
        Scalar,
        ["version() → text"],
        "PostgreSQL version string."
    ),
    f!(
        "pg_typeof",
        Scalar,
        ["pg_typeof(value any) → regtype"],
        "Data type of the value."
    ),
    f!(
        "gen_random_uuid",
        Scalar,
        ["gen_random_uuid() → uuid"],
        "A random version 4 UUID."
    ),
    f!(
        "pg_size_pretty",
        Scalar,
        ["pg_size_pretty(bytes bigint) → text"],
        "Byte count as kB, MB, GB …"
    ),
];

/// Keywords offered while typing. Multi-word entries complete in one go.
pub static KEYWORDS: &[&str] = &[
    "SELECT",
    "DISTINCT",
    "DISTINCT ON",
    "FROM",
    "WHERE",
    "GROUP BY",
    "HAVING",
    "ORDER BY",
    "LIMIT",
    "OFFSET",
    "FETCH FIRST",
    "ROWS ONLY",
    "AS",
    "AND",
    "OR",
    "NOT",
    "IN",
    "NOT IN",
    "EXISTS",
    "NOT EXISTS",
    "BETWEEN",
    "LIKE",
    "ILIKE",
    "NOT LIKE",
    "SIMILAR TO",
    "IS NULL",
    "IS NOT NULL",
    "IS DISTINCT FROM",
    "IS NOT DISTINCT FROM",
    "IS TRUE",
    "IS FALSE",
    "NULL",
    "TRUE",
    "FALSE",
    "CASE",
    "WHEN",
    "THEN",
    "ELSE",
    "END",
    "JOIN",
    "INNER JOIN",
    "LEFT JOIN",
    "RIGHT JOIN",
    "FULL JOIN",
    "CROSS JOIN",
    "LEFT JOIN LATERAL",
    "LATERAL",
    "ON",
    "USING",
    "UNION",
    "UNION ALL",
    "INTERSECT",
    "EXCEPT",
    "WITH",
    "WITH RECURSIVE",
    "RECURSIVE",
    "MATERIALIZED",
    "NOT MATERIALIZED",
    "ASC",
    "DESC",
    "NULLS FIRST",
    "NULLS LAST",
    "OVER",
    "PARTITION BY",
    "ROWS BETWEEN",
    "RANGE BETWEEN",
    "UNBOUNDED PRECEDING",
    "CURRENT ROW",
    "UNBOUNDED FOLLOWING",
    "FILTER",
    "WITHIN GROUP",
    "ANY",
    "ALL",
    "SOME",
    "ARRAY",
    "INTERVAL",
    "AT TIME ZONE",
    "VALUES",
    "TABLE",
    "EXPLAIN",
    "ANALYZE",
    "SHOW",
    "WINDOW",
    "GROUPING SETS",
    "ROLLUP",
    "CUBE",
    "COLLATE",
    "TABLESAMPLE",
    "ONLY",
    "DATE",
    "TIMESTAMP",
    "TIME",
];

/// Clause keywords: what usually follows them, used to rank keyword suggestions.
pub fn keywords_after(previous: &str) -> &'static [&'static str] {
    match previous {
        "select" => &["DISTINCT", "DISTINCT ON", "CASE", "COUNT"],
        "from" | "join" => &["LATERAL", "ONLY"],
        "where" | "and" | "or" | "having" | "on" => &["NOT", "EXISTS", "NOT EXISTS", "CASE"],
        "group" | "order" | "partition" => &["BY"],
        "is" => &["NULL", "NOT NULL", "NOT", "DISTINCT FROM", "TRUE", "FALSE"],
        "not" => &["IN", "EXISTS", "LIKE", "ILIKE", "BETWEEN", "NULL"],
        "union" => &["ALL", "SELECT"],
        "left" | "right" | "full" | "inner" | "cross" => &["JOIN"],
        "when" | "then" | "else" => &["CASE", "NULL"],
        "with" => &["RECURSIVE"],
        "nulls" => &["FIRST", "LAST"],
        _ => &[],
    }
}

/// Keywords that can follow a complete table reference or expression in the given clause.
pub static AFTER_RELATION: &[&str] = &[
    "WHERE",
    "JOIN",
    "LEFT JOIN",
    "INNER JOIN",
    "CROSS JOIN",
    "FULL JOIN",
    "RIGHT JOIN",
    "ON",
    "USING",
    "GROUP BY",
    "ORDER BY",
    "LIMIT",
    "UNION",
    "UNION ALL",
    "AS",
];

pub static AFTER_EXPRESSION: &[&str] = &[
    "AND",
    "OR",
    "AS",
    "FROM",
    "IS NULL",
    "IS NOT NULL",
    "IN",
    "NOT IN",
    "BETWEEN",
    "LIKE",
    "ILIKE",
    "GROUP BY",
    "ORDER BY",
    "HAVING",
    "LIMIT",
    "THEN",
    "ELSE",
    "END",
    "WHEN",
    "ASC",
    "DESC",
    "NULLS LAST",
    "NULLS FIRST",
    "OVER",
    "FILTER",
];

/// Data types for `::type` and `CAST(… AS type)`.
pub static TYPES: &[(&str, &str)] = &[
    ("text", "Variable-length text"),
    ("varchar", "Text with an optional maximum length"),
    ("char", "Fixed-length text"),
    ("integer", "4-byte whole number"),
    ("int", "4-byte whole number"),
    ("bigint", "8-byte whole number"),
    ("smallint", "2-byte whole number"),
    ("numeric", "Exact decimal number"),
    (
        "numeric(12,2)",
        "Exact decimal, 12 digits with 2 after the point",
    ),
    ("real", "4-byte floating point"),
    ("double precision", "8-byte floating point"),
    ("boolean", "true / false"),
    ("date", "Calendar date"),
    ("time", "Time of day"),
    ("timestamp", "Date and time without time zone"),
    ("timestamptz", "Date and time with time zone"),
    ("interval", "A span of time"),
    ("uuid", "Universally unique identifier"),
    ("json", "JSON text"),
    ("jsonb", "Binary JSON"),
    ("bytea", "Binary data"),
    ("money", "Currency amount"),
    ("text[]", "Array of text"),
    ("integer[]", "Array of integers"),
    ("regclass", "Table name resolved to an object id"),
];

pub fn function(name: &str) -> Option<&'static FnDoc> {
    FUNCTIONS.iter().find(|f| f.name == name)
}

/// Words that are keywords rather than names: they are never offered as columns and need
/// quoting when used as identifiers.
pub fn is_reserved(word: &str) -> bool {
    matches!(
        word,
        "all"
            | "analyse"
            | "analyze"
            | "and"
            | "any"
            | "array"
            | "as"
            | "asc"
            | "asymmetric"
            | "both"
            | "case"
            | "cast"
            | "check"
            | "collate"
            | "column"
            | "constraint"
            | "create"
            | "current_catalog"
            | "current_date"
            | "current_role"
            | "current_time"
            | "current_timestamp"
            | "current_user"
            | "default"
            | "deferrable"
            | "desc"
            | "distinct"
            | "do"
            | "else"
            | "end"
            | "except"
            | "false"
            | "fetch"
            | "for"
            | "foreign"
            | "from"
            | "grant"
            | "group"
            | "having"
            | "in"
            | "initially"
            | "intersect"
            | "into"
            | "lateral"
            | "leading"
            | "limit"
            | "localtime"
            | "localtimestamp"
            | "not"
            | "null"
            | "offset"
            | "on"
            | "only"
            | "or"
            | "order"
            | "placing"
            | "primary"
            | "references"
            | "returning"
            | "select"
            | "session_user"
            | "some"
            | "symmetric"
            | "table"
            | "then"
            | "to"
            | "trailing"
            | "true"
            | "union"
            | "unique"
            | "user"
            | "using"
            | "variadic"
            | "when"
            | "where"
            | "window"
            | "with"
            | "join"
            | "left"
            | "right"
            | "inner"
            | "outer"
            | "full"
            | "cross"
            | "natural"
            | "is"
            | "isnull"
            | "notnull"
            | "like"
            | "ilike"
            | "similar"
            | "between"
            | "over"
    )
}

/// Quotes an identifier only when it needs it (upper case, spaces, reserved words …).
///
/// # Examples
/// - `"orders"` → `orders`
/// - `"Order Items"` → `"Order Items"`
/// - `"select"` → `"select"`
pub fn ident(name: &str) -> String {
    let simple = name
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c == '_')
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '$');
    if simple && !is_reserved(name) {
        name.to_owned()
    } else {
        format!("\"{}\"", name.replace('"', "\"\""))
    }
}

/// The parameter list of a signature such as `left(text text, n integer) → text`.
///
/// # Examples
/// - `"left(text text, n integer) → text"` → `["text text", "n integer"]`
/// - `"now() → timestamptz"` → `[]`
pub fn signature_params(signature: &str) -> Vec<String> {
    let Some(open) = signature.find('(') else {
        return Vec::new();
    };
    let mut depth = 0i32;
    let mut current = String::new();
    let mut out = Vec::new();
    for c in signature[open + 1..].chars() {
        match c {
            '(' | '[' => {
                depth += 1;
                current.push(c);
            }
            ')' | ']' if depth == 0 => break,
            ')' | ']' => {
                depth -= 1;
                current.push(c);
            }
            ',' if depth == 0 => out.push(std::mem::take(&mut current).trim().to_owned()),
            _ => current.push(c),
        }
    }
    let last = current.trim();
    if !last.is_empty() {
        out.push(last.to_owned());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn function_names_are_unique_and_lower_case() {
        let mut seen = std::collections::BTreeSet::new();
        for f in FUNCTIONS {
            assert!(seen.insert(f.name), "duplicate {}", f.name);
            assert_eq!(f.name, f.name.to_lowercase());
            for sig in f.signatures {
                assert!(sig.starts_with(f.name), "{sig}");
            }
        }
    }

    #[test]
    fn quotes_identifiers_only_when_needed() {
        assert_eq!(ident("orders"), "orders");
        assert_eq!(ident("order_2"), "order_2");
        assert_eq!(ident("Orders"), "\"Orders\"");
        assert_eq!(ident("order items"), "\"order items\"");
        assert_eq!(ident("user"), "\"user\"");
        assert_eq!(ident("a\"b"), "\"a\"\"b\"");
    }

    #[test]
    fn splits_signature_parameters() {
        assert_eq!(
            signature_params("left(text text, n integer) → text"),
            vec!["text text", "n integer"]
        );
        assert!(signature_params("now() → timestamptz").is_empty());
        assert_eq!(
            signature_params("trim([LEADING | TRAILING] text text) → text"),
            vec!["[LEADING | TRAILING] text text"]
        );
    }
}
