import type { ErrorCategory } from "@/services/client";

/** Short headline for each backend error category. */
export const ERROR_TITLES: Record<ErrorCategory, string> = {
  connection_failed: "Server unreachable",
  authentication_failed: "Authentication failed",
  database_not_found: "Database not found",
  tls_error: "Secure connection failed",
  timeout: "Timed out",
  query_rejected: "Query not allowed",
  syntax_error: "Syntax error",
  undefined_object: "Object not found",
  permission_denied: "Permission denied",
  query_failed: "Query failed",
  connection_lost: "Connection lost",
  cancelled: "Cancelled",
  not_found: "Not found",
  invalid_input: "Check the details",
  credential_error: "Password storage problem",
  storage_error: "Local data problem",
  internal: "Something went wrong",
};
