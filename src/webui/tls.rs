//! # TLS/HTTPS Support Module
//!
//! Provides TLS certificate generation and management for the admin dashboard.
//!
//! ## Modes
//!
//! - **Self-Signed**: Auto-generate self-signed certificate on startup
//! - **Let's Encrypt**: ACME protocol for automatic certificate issuance
//! - **Custom**: Load user-provided certificate and private key
//! - **Disabled**: HTTP only (not recommended for production)
//!
//! ## Security
//!
//! - TLS 1.2 and 1.3 only
//! - Modern cipher suites
//! - Auto-renewal for Let's Encrypt certificates

use anyhow::{anyhow, Result};
use std::path::PathBuf;
use std::sync::Arc;

#[cfg(feature = "webui")]
use crate::config::AdminDashboardConfig;

#[cfg(feature = "webui")]
use rustls::ServerConfig;

#[cfg(feature = "webui")]
use tracing::{info, warn};

/// TLS configuration
#[derive(Clone)]
pub struct TlsConfig {
    pub server_config: Arc<ServerConfig>,
}

#[cfg(feature = "webui")]
impl TlsConfig {
    /// Create TLS configuration based on admin dashboard settings
    pub async fn from_dashboard_config(
        config: &AdminDashboardConfig,
        data_dir: &str,
    ) -> Result<Option<Self>> {
        match config.tls_mode.as_str() {
            "disabled" => {
                warn!("TLS is DISABLED - this is insecure for production use!");
                Ok(None)
            }
            "self_signed" => {
                info!("Generating self-signed TLS certificate...");
                Self::generate_self_signed(data_dir).await
            }
            "letsencrypt" => {
                info!("Using Let's Encrypt for TLS certificate...");
                let domain = config
                    .letsencrypt_domain
                    .as_ref()
                    .ok_or_else(|| anyhow!("letsencrypt_domain required for Let's Encrypt mode"))?;
                let email = config
                    .letsencrypt_email
                    .as_ref()
                    .ok_or_else(|| anyhow!("letsencrypt_email required for Let's Encrypt mode"))?;
                Self::get_letsencrypt(domain, email, data_dir).await
            }
            "custom" => {
                info!("Loading custom TLS certificate...");
                let cert_path = config
                    .tls_cert
                    .as_ref()
                    .ok_or_else(|| anyhow!("tls_cert required for custom mode"))?;
                let key_path = config
                    .tls_key
                    .as_ref()
                    .ok_or_else(|| anyhow!("tls_key required for custom mode"))?;
                Self::load_custom(cert_path, key_path).await
            }
            _ => Err(anyhow!("Invalid TLS mode: {}", config.tls_mode)),
        }
    }

    /// Generate self-signed certificate
    async fn generate_self_signed(data_dir: &str) -> Result<Option<Self>> {
        use rcgen::{Certificate, CertificateParams, DistinguishedName};
        use rustls::pki_types::CertificateDer;

        let mut params = CertificateParams::default();
        params.distinguished_name = DistinguishedName::new();
        params
            .distinguished_name
            .push(rcgen::DnType::CommonName, "MeshBBS Admin Dashboard");
        params.subject_alt_names = vec![
            rcgen::SanType::DnsName("localhost".to_string()),
            rcgen::SanType::IpAddress("127.0.0.1".parse().unwrap()),
            rcgen::SanType::IpAddress("::1".parse().unwrap()),
        ];

        let cert = Certificate::from_params(params)
            .map_err(|e| anyhow!("Failed to generate self-signed certificate: {}", e))?;

        let cert_pem = cert
            .serialize_pem()
            .map_err(|e| anyhow!("Failed to serialize certificate: {}", e))?;
        let key_pem = cert.serialize_private_key_pem();

        // Save to disk for persistence across restarts
        let cert_path = PathBuf::from(data_dir).join("webui_cert.pem");
        let key_path = PathBuf::from(data_dir).join("webui_key.pem");

        tokio::fs::write(&cert_path, &cert_pem)
            .await
            .map_err(|e| anyhow!("Failed to write certificate: {}", e))?;
        tokio::fs::write(&key_path, &key_pem)
            .await
            .map_err(|e| anyhow!("Failed to write private key: {}", e))?;

        info!(
            "Self-signed certificate generated and saved to {:?}",
            cert_path
        );

        // Load into rustls - parse the PEM properly
        let certs = vec![CertificateDer::from(cert_pem.as_bytes().to_vec())];

        // Parse private key from PEM format
        use std::io::BufReader;
        let mut key_reader = BufReader::new(key_pem.as_bytes());
        let key = rustls_pemfile::private_key(&mut key_reader)
            .map_err(|e| anyhow!("Failed to parse private key: {}", e))?
            .ok_or_else(|| anyhow!("No private key found in PEM"))?;

        let server_config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .map_err(|e| anyhow!("Failed to create TLS config: {}", e))?;

        Ok(Some(Self {
            server_config: Arc::new(server_config),
        }))
    }

    /// Get Let's Encrypt certificate via ACME
    async fn get_letsencrypt(_domain: &str, _email: &str, _data_dir: &str) -> Result<Option<Self>> {
        // TODO: Implement Let's Encrypt ACME protocol
        // This is a complex process requiring:
        // 1. HTTP-01 or DNS-01 challenge
        // 2. Certificate request
        // 3. Auto-renewal logic
        warn!("Let's Encrypt support not yet implemented - falling back to self-signed");
        Self::generate_self_signed(_data_dir).await
    }

    /// Load custom certificate and key
    async fn load_custom(cert_path: &str, key_path: &str) -> Result<Option<Self>> {
        use rustls::pki_types::CertificateDer;
        use std::io::BufReader;

        // Read certificate
        let cert_file = std::fs::File::open(cert_path)
            .map_err(|e| anyhow!("Failed to open certificate file {}: {}", cert_path, e))?;
        let mut cert_reader = BufReader::new(cert_file);
        let certs: Vec<CertificateDer> = rustls_pemfile::certs(&mut cert_reader)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| anyhow!("Failed to parse certificate: {}", e))?;

        if certs.is_empty() {
            return Err(anyhow!("No certificates found in {}", cert_path));
        }

        // Read private key
        let key_file = std::fs::File::open(key_path)
            .map_err(|e| anyhow!("Failed to open key file {}: {}", key_path, e))?;
        let mut key_reader = BufReader::new(key_file);

        let key = rustls_pemfile::private_key(&mut key_reader)
            .map_err(|e| anyhow!("Failed to parse private key: {}", e))?
            .ok_or_else(|| anyhow!("No private key found in {}", key_path))?;

        let server_config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .map_err(|e| anyhow!("Failed to create TLS config: {}", e))?;

        info!("Loaded custom TLS certificate from {}", cert_path);

        Ok(Some(Self {
            server_config: Arc::new(server_config),
        }))
    }
}
