//! SPKI certificate pinning for stream WebSocket TLS connections.

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::client::WebPkiServerVerifier;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, Error, SignatureScheme};
use sha2::{Digest, Sha256};
use std::sync::Arc;

/// Build a rustls `ClientConfig` that verifies standard WebPKI trust **and**
/// requires at least one certificate in the chain to match a pinned SPKI hash.
pub(crate) fn build_pinned_tls_config(spki_hashes: &[String]) -> ClientConfig {
    let provider = Arc::new(rustls::crypto::ring::default_provider());

    let mut root_store = rustls::RootCertStore::empty();
    root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    let default_verifier = WebPkiServerVerifier::builder_with_provider(
        Arc::new(root_store),
        provider.clone(),
    )
    .build()
    .expect("Failed to build WebPKI verifier");

    ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .expect("TLS protocol versions")
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(PinningVerifier {
            inner: default_verifier,
            spki_hashes: spki_hashes.to_vec(),
        }))
        .with_no_client_auth()
}

#[derive(Debug)]
struct PinningVerifier {
    inner: Arc<dyn ServerCertVerifier>,
    spki_hashes: Vec<String>,
}

impl ServerCertVerifier for PinningVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        self.inner
            .verify_server_cert(end_entity, intermediates, server_name, ocsp_response, now)?;

        if !chain_matches_pin(end_entity, intermediates, &self.spki_hashes) {
            return Err(Error::General(
                "Certificate pinning failed: no certificate in chain matches pinned SPKI hash"
                    .into(),
            ));
        }

        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        self.inner.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        self.inner.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.inner.supported_verify_schemes()
    }
}

fn chain_matches_pin(
    end_entity: &CertificateDer<'_>,
    intermediates: &[CertificateDer<'_>],
    spki_hashes: &[String],
) -> bool {
    let all_certs = std::iter::once(end_entity).chain(intermediates.iter());
    for cert_der in all_certs {
        if let Some(spki_bytes) = extract_spki_der(cert_der.as_ref()) {
            let hash = Sha256::digest(spki_bytes);
            let hash_b64 = B64.encode(hash);
            if spki_hashes.iter().any(|h| h == &hash_b64) {
                return true;
            }
        }
    }
    false
}

// ─── Minimal DER parser for SPKI extraction ──────────────────

fn extract_spki_der(cert_der: &[u8]) -> Option<&[u8]> {
    let mut r = DerReader::new(cert_der);
    r.enter_sequence()?; // Certificate
    r.enter_sequence()?; // TBSCertificate
    if r.peek_tag()? == 0xa0 {
        r.skip_element()?; // version
    }
    r.skip_element()?; // serialNumber
    r.skip_element()?; // signature algorithm
    r.skip_element()?; // issuer
    r.skip_element()?; // validity
    r.skip_element()?; // subject
    r.read_element_raw() // subjectPublicKeyInfo
}

struct DerReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> DerReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn peek_tag(&self) -> Option<u8> {
        self.data.get(self.pos).copied()
    }

    fn read_tag_and_length(&mut self) -> Option<(u8, usize)> {
        let tag = *self.data.get(self.pos)?;
        self.pos += 1;
        let first = *self.data.get(self.pos)? as usize;
        self.pos += 1;
        let length = if first < 0x80 {
            first
        } else {
            let n = first & 0x7f;
            if n > 4 {
                return None;
            }
            let mut len = 0usize;
            for _ in 0..n {
                len = len.checked_shl(8)? | (*self.data.get(self.pos)? as usize);
                self.pos += 1;
            }
            len
        };
        Some((tag, length))
    }

    fn enter_sequence(&mut self) -> Option<()> {
        let (tag, _) = self.read_tag_and_length()?;
        if tag != 0x30 {
            return None;
        }
        Some(())
    }

    fn skip_element(&mut self) -> Option<()> {
        let (_, length) = self.read_tag_and_length()?;
        if self.pos + length > self.data.len() {
            return None;
        }
        self.pos += length;
        Some(())
    }

    fn read_element_raw(&mut self) -> Option<&'a [u8]> {
        let start = self.pos;
        let (_, length) = self.read_tag_and_length()?;
        let end = self.pos + length;
        if end > self.data.len() {
            return None;
        }
        self.pos = end;
        Some(&self.data[start..end])
    }
}
