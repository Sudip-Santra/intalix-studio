import type { Environment, SslMode } from "@/services/client";

export const ENVIRONMENT_META: Record<Environment, { label: string; color: string }> = {
  development: { label: "Development", color: "var(--env-development)" },
  staging: { label: "Staging", color: "var(--env-staging)" },
  production: { label: "Production", color: "var(--env-production)" },
};

export type SslLevel = "insecure" | "weak" | "secure";

export const SSL_MODE_META: Record<SslMode, { label: string; level: SslLevel; summary: string; detail: string }> = {
  "verify-full": {
    label: "Verify full",
    level: "secure",
    summary: "Encrypted and verified",
    detail: "Recommended. Checks the server certificate and that it matches the host name.",
  },
  "verify-ca": {
    label: "Verify CA",
    level: "secure",
    summary: "Encrypted, CA verified",
    detail: "Checks the certificate was issued by a trusted CA, but not that it matches the host name.",
  },
  require: {
    label: "Require",
    level: "weak",
    summary: "Encrypted, not verified",
    detail: "Traffic is encrypted, but the server's identity is not checked, so it could be impersonated.",
  },
  prefer: {
    label: "Prefer",
    level: "weak",
    summary: "Encrypted if available",
    detail: "Silently falls back to plain text if the server does not offer TLS. Identity is not checked.",
  },
  disable: {
    label: "Disable",
    level: "insecure",
    summary: "Not encrypted",
    detail: "Everything, including your password, is sent in plain text. Use only for a local database.",
  },
};

export { ERROR_TITLES } from "@/lib/error-titles";
