//! RTMPS certificate checks with Apple's Security framework.
//!
//! Android hands the relay its system CAs as a PEM file; iOS keeps its trust store to itself.
//! There, OpenSSL does the handshake and the chain the platform sent is then evaluated with
//! `SecTrustEvaluateWithError` for the platform's host name, like any iOS app's TLS: the
//! system roots, the user's trust settings, revocation and Apple's certificate policies apply.
//! Nothing is sent over the connection before the chain is trusted.

use security_framework::certificate::SecCertificate;
use security_framework::policy::SecPolicy;
use security_framework::secure_transport::SslProtocolSide;
use security_framework::trust::SecTrust;

/// Makes client connections without a CA file use the system's trust store. Idempotent.
pub(crate) fn install() {
    let _ = librtmp2::transport::set_peer_chain_verifier(verify_chain);
}

/// True when the chain (leaf first) is trusted for `host`.
fn verify_chain(host: &str, chain: &[Vec<u8>]) -> bool {
    let Ok(certificates) = chain
        .iter()
        .map(|der| SecCertificate::from_der(der))
        .collect::<Result<Vec<_>, _>>()
    else {
        return false;
    };
    if certificates.is_empty() {
        return false;
    }
    let policy = SecPolicy::create_ssl(SslProtocolSide::SERVER, Some(host));
    SecTrust::create_with_certificates(&certificates, &[policy])
        .is_ok_and(|trust| trust.evaluate_with_error().is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn garbage_and_empty_chains_are_not_trusted() {
        assert!(!verify_chain("example.com", &[]));
        assert!(!verify_chain("example.com", &[vec![1, 2, 3]]));
    }
}
