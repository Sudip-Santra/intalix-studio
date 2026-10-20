//! TLS configuration per SSL mode, matching libpq semantics:
//!
//! | mode        | encrypted          | chain verified | host name verified |
//! |-------------|--------------------|----------------|--------------------|
//! | disable     | no                 | -              | -                  |
//! | prefer      | if server offers   | no             | no                 |
//! | require     | yes                | no             | no                 |
//! | verify-ca   | yes                | yes            | no                 |
//! | verify-full | yes                | yes            | yes                |
//!
//! Trust roots: the CA file when one is configured (like libpq's `sslrootcert`), otherwise
//! the Windows certificate store, so company CAs deployed by IT are trusted automatically.
//! Verification is never silently downgraded: verify-* modes fail closed.

use std::sync::Arc;

use rustls::client::WebPkiServerVerifier;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, verify_tls12_signature, verify_tls13_signature};
use rustls::{
    CertificateError, ClientConfig, DigitallySignedStruct, RootCertStore, SignatureScheme,
};
use rustls_pki_types::pem::PemObject;
use rustls_pki_types::{CertificateDer, ServerName, UnixTime};

use crate::error::{AppError, AppResult, ErrorCategory};
use crate::model::SslMode;

/// Largest CA bundle we'll read; real bundles are a few hundred KB at most.
const MAX_CA_FILE_BYTES: u64 = 4 * 1024 * 1024;

fn provider() -> Arc<CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

fn tls_error(message: impl Into<String>) -> AppError {
    AppError::new(ErrorCategory::TlsError, message)
}

/// Builds the rustls client configuration for an SSL mode.
pub fn client_config(mode: SslMode, ca_cert_path: &str) -> AppResult<ClientConfig> {
    let provider = provider();
    let verifier: Arc<dyn ServerCertVerifier> = match mode {
        // For disable no handshake happens; prefer/require encrypt without verifying (libpq).
        SslMode::Disable | SslMode::Prefer | SslMode::Require => Arc::new(EncryptOnly {
            provider: provider.clone(),
        }),
        SslMode::VerifyCa | SslMode::VerifyFull => {
            let roots = trust_roots(ca_cert_path)?;
            let webpki =
                WebPkiServerVerifier::builder_with_provider(Arc::new(roots), provider.clone())
                    .build()
                    .map_err(|e| {
                        tls_error(format!("Could not set up certificate verification: {e}"))
                    })?;
            if mode == SslMode::VerifyCa {
                Arc::new(ChainOnly { inner: webpki })
            } else {
                webpki
            }
        }
    };

    let config = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| tls_error(format!("Could not set up TLS: {e}")))?
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth();
    Ok(config)
}

fn trust_roots(ca_cert_path: &str) -> AppResult<RootCertStore> {
    let mut roots = RootCertStore::empty();
    if ca_cert_path.is_empty() {
        let native = rustls_native_certs::load_native_certs();
        for error in &native.errors {
            tracing::warn!(%error, "skipped a certificate from the Windows store");
        }
        let (added, _ignored) = roots.add_parsable_certificates(native.certs);
        if added == 0 {
            return Err(tls_error(
                "No trusted certificate authorities were found on this computer.",
            )
            .with_hint("Provide the CA certificate file for your database server."));
        }
        return Ok(roots);
    }

    let path = std::path::Path::new(ca_cert_path);
    let meta = std::fs::metadata(path).map_err(|_| {
        tls_error("The CA certificate file could not be opened.")
            .with_hint("Check the path and that the file exists.")
    })?;
    if !meta.is_file() || meta.len() > MAX_CA_FILE_BYTES {
        return Err(tls_error(
            "The CA certificate path is not a valid certificate file.",
        ));
    }
    let pem =
        std::fs::read(path).map_err(|_| tls_error("The CA certificate file could not be read."))?;
    let certs: Vec<CertificateDer<'static>> = CertificateDer::pem_slice_iter(&pem)
        .collect::<Result<_, _>>()
        .map_err(|_| {
            tls_error("The CA certificate file is not valid PEM.")
                .with_hint("Use a .pem or .crt file.")
        })?;
    let (added, _ignored) = roots.add_parsable_certificates(certs);
    if added == 0 {
        return Err(tls_error(
            "The CA certificate file contains no usable certificates.",
        ));
    }
    Ok(roots)
}

/// prefer/require: encrypt, but (like libpq) do not verify the certificate. Handshake
/// signatures are still checked so the session keys are genuinely negotiated.
#[derive(Debug)]
struct EncryptOnly {
    provider: Arc<CryptoProvider>,
}

impl ServerCertVerifier for EncryptOnly {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// verify-ca: the full chain must verify against the trust roots; only a host-name mismatch
/// is tolerated. Every other certificate problem still fails.
#[derive(Debug)]
struct ChainOnly {
    inner: Arc<WebPkiServerVerifier>,
}

impl ServerCertVerifier for ChainOnly {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        match self.inner.verify_server_cert(
            end_entity,
            intermediates,
            server_name,
            ocsp_response,
            now,
        ) {
            // webpki checks the chain before the name, so this error means the chain was valid.
            Err(rustls::Error::InvalidCertificate(
                CertificateError::NotValidForName | CertificateError::NotValidForNameContext { .. },
            )) => Ok(ServerCertVerified::assertion()),
            other => other,
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.inner.supported_verify_schemes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_mode_builds_with_system_roots() {
        for mode in [
            SslMode::Disable,
            SslMode::Prefer,
            SslMode::Require,
            SslMode::VerifyCa,
            SslMode::VerifyFull,
        ] {
            assert!(client_config(mode, "").is_ok(), "{mode:?}");
        }
    }

    #[test]
    fn missing_ca_file_is_a_clear_tls_error() {
        let err = client_config(SslMode::VerifyFull, "Z:\\definitely\\missing.pem").err();
        assert_eq!(err.map(|e| e.category), Some(ErrorCategory::TlsError));
    }
}
