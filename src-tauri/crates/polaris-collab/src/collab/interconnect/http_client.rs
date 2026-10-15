//! One TLS policy for HTTP requests to configured authorities and relays.
use std::{error::Error, sync::Arc};
use ureq::rustls::{self, pki_types::{CertificateDer, pem::PemObject}};

/// Preserve each caller's timeout/redirect policy and ureq's unchanged default TLS
/// configuration when no extra trust is configured. Reload on construction.
pub fn agent(builder: ureq::AgentBuilder) -> Result<ureq::Agent, String> {
    with_ca(builder, super::load()?.authority.extra_ca_pem.as_deref())
}

pub fn with_ca(builder: ureq::AgentBuilder, extra: Option<&str>) -> Result<ureq::Agent, String> {
    let Some(extra) = extra else { return Ok(builder.build()); };
    Ok(builder.tls_config(Arc::new(tls_config(Some(extra))?)).build())
}

fn tls_config(extra: Option<&str>) -> Result<rustls::ClientConfig, String> {
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    for certificate in extra.map(certificates).transpose()?.into_iter().flatten() {
        roots.add(certificate).map_err(|e| format!("InterconnectCaInvalid: authority.extraCaPem: {e}"))?;
    }
    // Use ureq's crypto backend explicitly: collab-net may also enable aws-lc-rs.
    let tls = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_protocol_versions(&[&rustls::version::TLS12, &rustls::version::TLS13])
        .map_err(|e| format!("InterconnectTlsConfigInvalid: {e}"))?
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(tls)
}

fn certificates(extra: &str) -> Result<Vec<CertificateDer<'static>>, String> {
    let pem = if extra.contains("-----BEGIN CERTIFICATE-----") {
        extra.as_bytes().to_vec()
    } else {
        std::fs::read(extra).map_err(|e| format!("InterconnectCaReadFailed: authority.extraCaPem: {e}"))?
    };
    let certificates = CertificateDer::pem_slice_iter(&pem).collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("InterconnectCaInvalid: authority.extraCaPem: {e}"))?;
    if certificates.is_empty() { return Err("InterconnectCaInvalid: authority.extraCaPem contains no certificates".into()); }
    // Validate certificates for both the ureq and iroh adapters.
    let mut check = rustls::RootCertStore::empty();
    for certificate in &certificates {
        check.add(certificate.clone()).map_err(|e| format!("InterconnectCaInvalid: authority.extraCaPem: {e}"))?;
    }
    Ok(certificates)
}

/// iroh's relay HTTP/TLS uses the same additive CA input. Leave its defaults alone
/// without configuration; endpoint recreation/restart picks up CA changes.
#[cfg(feature = "collab-net")]
pub fn relay_tls_config() -> Result<Option<iroh::tls::CaTlsConfig>, String> {
    super::load()?.authority.extra_ca_pem.as_deref().map(|extra| {
        Ok(iroh::tls::CaTlsConfig::embedded().with_extra_roots(certificates(extra)?))
    }).transpose()
}

/// Transport diagnostics must not echo URLs, tokens or response bodies.
pub fn diagnostic(error: &ureq::Error) -> String {
    let mut cause: Option<&(dyn Error + 'static)> = Some(error);
    while let Some(current) = cause {
        if let Some(tls) = current.downcast_ref::<rustls::Error>() {
            return format!("InterconnectTlsCertificateError: {tls}; check authority.extraCaPem, certificate hostname and validity");
        }
        let text = current.to_string().to_ascii_lowercase();
        if text.contains("certificate") || text.contains("tls connection") {
            return "InterconnectTlsCertificateError: TLS certificate verification failed; check authority.extraCaPem, certificate hostname and validity".into();
        }
        cause = current.source();
    }
    "interconnect_upstream_unavailable".into()
}

#[cfg(test)]
mod tests;
