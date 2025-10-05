use crate::policy_engine::{PolicyEngine, TlsPolicyInput};
use anyhow::{anyhow, Context, Result};
use rustls::crypto::ring::cipher_suite::{
    TLS13_AES_128_GCM_SHA256, TLS13_AES_256_GCM_SHA384, TLS13_CHACHA20_POLY1305_SHA256,
    TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256, TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384,
    TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256, TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384,
    TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256,
};
use rustls::crypto::ring::kx_group::{SECP256R1, SECP384R1, X25519};
use rustls::pki_types::CertificateDer;
use rustls::server::WebPkiClientVerifier;
use rustls::{version, RootCertStore, SupportedProtocolVersion};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::BufReader;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, Mutex, OnceLock};
use tonic::transport::{
    Certificate as TonicCertificate, Identity as TonicIdentity, ServerTlsConfig,
};

static TLS_PROVIDER_INSTALLED: OnceLock<()> = OnceLock::new();

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CipherPolicy {
    Strict,
    #[default]
    Balanced,
    Legacy,
}

impl FromStr for CipherPolicy {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "strict" => Ok(CipherPolicy::Strict),
            "balanced" => Ok(CipherPolicy::Balanced),
            "legacy" => Ok(CipherPolicy::Legacy),
            other => Err(format!(
                "unsupported TLS cipher policy '{other}'. Use strict|balanced|legacy"
            )),
        }
    }
}

impl std::fmt::Display for CipherPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CipherPolicy::Strict => write!(f, "strict"),
            CipherPolicy::Balanced => write!(f, "balanced"),
            CipherPolicy::Legacy => write!(f, "legacy"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct TlsPaths {
    pub cert: PathBuf,
    pub key: PathBuf,
    pub ca: Option<PathBuf>,
}

impl TlsPaths {
    pub fn watch_dirs(&self) -> Vec<PathBuf> {
        let mut dirs = vec![parent_dir(&self.cert), parent_dir(&self.key)];
        if let Some(ca) = &self.ca {
            dirs.push(parent_dir(ca));
        }
        dirs.sort();
        dirs.dedup();
        dirs
    }
}

#[derive(Clone)]
pub struct TlsSnapshot {
    pub identity: TonicIdentity,
    pub client_ca: Option<TonicCertificate>,
    pub cipher_policy: CipherPolicy,
    pub tls_versions: Vec<String>,
    pub client_auth_required: bool,
    pub certificate_fingerprint_sha256: String,
    pub ca_fingerprint_sha256: Option<String>,
}

impl TlsSnapshot {
    pub fn as_server_tls_config(&self) -> ServerTlsConfig {
        let mut config = ServerTlsConfig::new().identity(self.identity.clone());
        if let Some(ca) = &self.client_ca {
            config = config
                .client_ca_root(ca.clone())
                .client_auth_optional(false);
        }
        config
    }

    pub fn metadata_eq(&self, other: &Self) -> bool {
        self.cipher_policy == other.cipher_policy
            && self.client_auth_required == other.client_auth_required
            && self.certificate_fingerprint_sha256 == other.certificate_fingerprint_sha256
            && self.ca_fingerprint_sha256 == other.ca_fingerprint_sha256
            && self.tls_versions == other.tls_versions
    }
}

pub fn load_tls_snapshot(
    paths: &TlsPaths,
    policy: CipherPolicy,
    listener: SocketAddr,
    policy_engine: &Arc<Mutex<PolicyEngine>>,
) -> Result<TlsSnapshot> {
    let cert_pem = fs::read(&paths.cert)
        .with_context(|| format!("reading TLS certificate {}", paths.cert.display()))?;
    let key_pem = fs::read(&paths.key)
        .with_context(|| format!("reading TLS private key {}", paths.key.display()))?;

    let cert_chain = pem_certs(&cert_pem).context("parsing certificate chain")?;
    if cert_chain.is_empty() {
        return Err(anyhow!(
            "TLS certificate chain {} is empty",
            paths.cert.display()
        ));
    }

    // Ensure private key is valid and matches supported formats.
    let _key_der = {
        let mut reader = BufReader::new(&key_pem[..]);
        rustls_pemfile::private_key(&mut reader)
            .context("parsing TLS private key")?
            .ok_or_else(|| {
                anyhow!(
                    "TLS private key {} must contain a valid PKCS#1/PKCS#8/SEC1 key",
                    paths.key.display()
                )
            })?
    };

    let (client_ca, ca_fingerprint, ca_chain) = if let Some(ca_path) = &paths.ca {
        let ca_pem = fs::read(ca_path)
            .with_context(|| format!("reading TLS CA bundle {}", ca_path.display()))?;
        let ca_certs = pem_certs(&ca_pem).context("parsing CA bundle")?;
        if ca_certs.is_empty() {
            return Err(anyhow!(
                "TLS CA bundle {} contains no certificates",
                ca_path.display()
            ));
        }
        let fingerprint = hex::encode(Sha256::digest(ca_certs[0].as_ref()));
        (
            Some(TonicCertificate::from_pem(ca_pem)),
            Some(fingerprint),
            Some(ca_certs),
        )
    } else {
        (None, None, None)
    };

    let mut provider = rustls::crypto::ring::default_provider();
    provider.cipher_suites = match policy {
        CipherPolicy::Strict => vec![
            TLS13_AES_256_GCM_SHA384,
            TLS13_AES_128_GCM_SHA256,
            TLS13_CHACHA20_POLY1305_SHA256,
        ],
        CipherPolicy::Balanced => vec![
            TLS13_AES_256_GCM_SHA384,
            TLS13_AES_128_GCM_SHA256,
            TLS13_CHACHA20_POLY1305_SHA256,
            TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384,
            TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256,
            TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384,
            TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256,
        ],
        CipherPolicy::Legacy => vec![
            TLS13_AES_256_GCM_SHA384,
            TLS13_AES_128_GCM_SHA256,
            TLS13_CHACHA20_POLY1305_SHA256,
            TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384,
            TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256,
            TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384,
            TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256,
            TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256,
        ],
    };
    provider.kx_groups = match policy {
        CipherPolicy::Strict => vec![X25519, SECP256R1],
        CipherPolicy::Balanced | CipherPolicy::Legacy => vec![X25519, SECP256R1, SECP384R1],
    };

    // Install provider as process default once (best effort).
    if TLS_PROVIDER_INSTALLED.get().is_none() && provider.clone().install_default().is_ok() {
        let _ = TLS_PROVIDER_INSTALLED.set(());
    }

    let provider = Arc::new(provider);
    let protocol_versions = match policy {
        CipherPolicy::Strict => vec![&version::TLS13],
        CipherPolicy::Balanced | CipherPolicy::Legacy => vec![&version::TLS13, &version::TLS12],
    };

    if let Some(ca) = &ca_chain {
        let mut root_store = RootCertStore::empty();
        for cert in ca {
            root_store
                .add(cert.clone())
                .context("adding CA certificate to root store")?;
        }
        WebPkiClientVerifier::builder_with_provider(Arc::new(root_store), provider.clone())
            .build()
            .context("building mTLS verifier")?;
    }

    let tls_versions = protocol_versions
        .iter()
        .map(|v| version_label(v))
        .collect::<Vec<_>>();

    let identity = TonicIdentity::from_pem(cert_pem.clone(), key_pem.clone());
    let client_auth_required = client_ca.is_some();

    let snapshot = TlsSnapshot {
        identity,
        client_ca,
        cipher_policy: policy,
        tls_versions,
        client_auth_required,
        certificate_fingerprint_sha256: hex::encode(Sha256::digest(cert_chain[0].as_ref())),
        ca_fingerprint_sha256: ca_fingerprint,
    };

    let tls_policy_input = TlsPolicyInput {
        listener: listener.to_string(),
        cipher_policy: policy.to_string(),
        tls_versions: snapshot.tls_versions.clone(),
        client_auth_required,
        certificate_fingerprint_sha256: Some(snapshot.certificate_fingerprint_sha256.clone()),
        ca_fingerprint_sha256: snapshot.ca_fingerprint_sha256.clone(),
    };

    let decision = policy_engine
        .lock()
        .map_err(|err| anyhow!("failed to lock policy engine: {err}"))?
        .evaluate_tls(&tls_policy_input)
        .context("evaluating TLS policy")?;

    if !decision.is_allowed() {
        let reasons = decision.deny_reasons.join(", ");
        return Err(anyhow!("TLS policy rejected configuration: {reasons}"));
    }

    Ok(snapshot)
}

pub fn snapshots_equal(a: &Option<Arc<TlsSnapshot>>, b: &Option<Arc<TlsSnapshot>>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(left), Some(right)) => left.metadata_eq(right),
        _ => false,
    }
}

fn pem_certs(pem: &[u8]) -> Result<Vec<CertificateDer<'static>>> {
    rustls_pemfile::certs(&mut BufReader::new(pem))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| anyhow!(err))
}

fn version_label(version: &SupportedProtocolVersion) -> String {
    if version == &version::TLS13 {
        "TLS1.3".to_string()
    } else if version == &version::TLS12 {
        "TLS1.2".to_string()
    } else {
        "UNKNOWN".to_string()
    }
}

fn parent_dir(path: &Path) -> PathBuf {
    path.parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}
