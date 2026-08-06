//! Shared rustls client configurations (ring provider) for the IMAP, SMTP,
//! Sieve and discovery code paths.
//!
//! Three trust models exist in Mailquill:
//! - [`webpki_client_config`]: normal WebPKI verification against the bundled
//!   Mozilla roots — the default for all connections.
//! - [`pinned_client_config`]: certificate pinning for user-approved trust
//!   exceptions (self-signed or private-CA mail servers).
//! - [`insecure_probe_client_config`]: no verification at all — exclusively
//!   for fetching a peer certificate so the user can review and approve it.

use std::sync::Arc;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::CryptoProvider;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, SignatureScheme};

fn ring_provider() -> Arc<CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

/// Client config verifying against the bundled WebPKI (Mozilla) roots.
pub fn webpki_client_config() -> ClientConfig {
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    ClientConfig::builder_with_provider(ring_provider())
        .with_safe_default_protocol_versions()
        .expect("ring provider supports the default protocol versions")
        .with_root_certificates(roots)
        .with_no_client_auth()
}

/// Client config that accepts exactly one user-approved certificate
/// (byte-identical DER), rejecting everything else including WebPKI-valid
/// certificates. Used for stored trust exceptions. Handshake signatures are
/// still verified against the pinned certificate's key, so possession of the
/// certificate alone is not enough to impersonate the server.
pub fn pinned_client_config(pinned_der: Vec<u8>) -> ClientConfig {
    let provider = ring_provider();
    ClientConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()
        .expect("ring provider supports the default protocol versions")
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(PinnedCertVerifier {
            pinned: pinned_der,
            provider,
        }))
        .with_no_client_auth()
}

/// INSECURE: accepts any server certificate. Only for reachability probes and
/// for fetching the peer certificate to present a trust-exception prompt —
/// never for authenticated traffic.
pub fn insecure_probe_client_config() -> ClientConfig {
    let provider = ring_provider();
    ClientConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()
        .expect("ring provider supports the default protocol versions")
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(AcceptAnyCertVerifier { provider }))
        .with_no_client_auth()
}

#[derive(Debug)]
struct PinnedCertVerifier {
    pinned: Vec<u8>,
    provider: Arc<CryptoProvider>,
}

impl ServerCertVerifier for PinnedCertVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        if end_entity.as_ref() == self.pinned.as_slice() {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::InvalidCertificate(
                rustls::CertificateError::ApplicationVerificationFailure,
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
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
        rustls::crypto::verify_tls13_signature(
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

#[derive(Debug)]
struct AcceptAnyCertVerifier {
    provider: Arc<CryptoProvider>,
}

impl ServerCertVerifier for AcceptAnyCertVerifier {
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
        rustls::crypto::verify_tls12_signature(
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
        rustls::crypto::verify_tls13_signature(
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

#[cfg(test)]
mod tests {
    use super::*;

    fn verify(verifier: &dyn ServerCertVerifier, presented: &[u8]) -> Result<(), rustls::Error> {
        let cert = CertificateDer::from(presented.to_vec());
        let name = ServerName::try_from("mail.example.com").unwrap();
        verifier
            .verify_server_cert(
                &cert,
                &[],
                &name,
                &[],
                UnixTime::since_unix_epoch(std::time::Duration::from_secs(1_700_000_000)),
            )
            .map(|_| ())
    }

    #[test]
    fn pinned_verifier_accepts_only_the_exact_certificate() {
        let pinned = PinnedCertVerifier {
            pinned: vec![1, 2, 3, 4],
            provider: ring_provider(),
        };
        assert!(verify(&pinned, &[1, 2, 3, 4]).is_ok());
        assert!(verify(&pinned, &[1, 2, 3, 5]).is_err());
        // A truncated or extended DER blob must not pass either.
        assert!(verify(&pinned, &[1, 2, 3]).is_err());
        assert!(verify(&pinned, &[1, 2, 3, 4, 5]).is_err());
    }

    #[test]
    fn probe_verifier_accepts_anything() {
        let probe = AcceptAnyCertVerifier {
            provider: ring_provider(),
        };
        assert!(verify(&probe, &[42]).is_ok());
    }
}
