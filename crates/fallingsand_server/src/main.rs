mod cloudflare;

use fallingsand_net::wt_native::WtListener;
use fallingsand_server::{Server, ServerConfig, ServerControl, WorldConfig};
use rcgen::{CertificateParams, KeyPair};
use rustls_pki_types::pem::PemObject;
use rustls_pki_types::{CertificateDer, PrivateKeyDer};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;
use tracing::{info, warn};

const DEFAULT_ADDR: &str = "0.0.0.0:4433";
const CERT_VALIDITY_DAYS: i64 = 13;
// cwd-relative; keep the working directory stable so cached ACME certs are reused
const CERT_DIR: &str = "saves/certs";

fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_target(false)
        .init();
    let started = Instant::now();
    info!("fallingsand server v{}", env!("CARGO_PKG_VERSION"));
    let mut args = std::env::args().skip(1);
    let mut addr: std::net::SocketAddr = DEFAULT_ADDR.parse()?;
    let mut cert_path: Option<String> = None;
    let mut key_path: Option<String> = None;
    let mut domain: Option<String> = None;
    let mut token: Option<String> = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--cert" => cert_path = args.next(),
            "--key" => key_path = args.next(),
            "--domain" => domain = args.next(),
            "--cloudflare-token" => token = args.next(),
            other => addr = other.parse()?,
        }
    }
    let token = token.or_else(|| std::env::var("CLOUDFLARE_API_TOKEN").ok());
    let domain = domain.or_else(|| std::env::var("FALLINGSAND_DOMAIN").ok());
    let web_client_url = std::env::var("FALLINGSAND_WEB_CLIENT_URL").ok();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    let (cert_chain, key, cert_hash_hex) =
        if let (Some(cert_path), Some(key_path)) = (&cert_path, &key_path) {
            let (chain, key) = load_pem(cert_path, key_path)?;
            info!("using certificate from {cert_path}");
            (chain, key, None)
        } else if cert_path.is_some() || key_path.is_some() {
            anyhow::bail!("--cert and --key must be given together");
        } else if let (Some(token), Some(domain)) = (&token, &domain) {
            let host = Arc::new(runtime.block_on(cloudflare::CloudflareHost::new(
                token.clone(),
                domain.clone(),
            ))?);
            let ip = runtime.block_on(host.ensure_dns())?;
            info!("dns record set: {domain} -> {ip}");
            let dir = Path::new(CERT_DIR).join(domain);
            let (cert_file, key_file) = runtime.block_on(host.ensure_certificate(&dir))?;
            host.clone().spawn_ddns(runtime.handle(), ip);
            let (chain, key) = load_pem(cert_file, key_file)?;
            (chain, key, None)
        } else if token.is_some() {
            anyhow::bail!("--cloudflare-token requires --domain");
        } else {
            warn!("no certificate configured, generating a self-signed one (13 day validity)");
            let mut sans = vec![
                "localhost".to_string(),
                "127.0.0.1".to_string(),
                addr.ip().to_string(),
            ];
            sans.extend(domain.clone());
            let mut params = CertificateParams::new(sans)?;
            params.not_before = time::OffsetDateTime::now_utc() - time::Duration::hours(1);
            params.not_after =
                time::OffsetDateTime::now_utc() + time::Duration::days(CERT_VALIDITY_DAYS);
            let key = KeyPair::generate()?;
            let cert = params.self_signed(&key)?;
            let cert_hash = Sha256::digest(cert.der().as_ref());
            let cert_hash_hex: String = cert_hash.iter().map(|b| format!("{b:02x}")).collect();
            (
                vec![cert.der().clone()],
                PrivateKeyDer::try_from(key.serialize_der()).map_err(anyhow::Error::msg)?,
                Some(cert_hash_hex),
            )
        };

    let listener = WtListener::bind(runtime.handle().clone(), addr, cert_chain, key)?;

    let mut server = Server::new(ServerConfig {
        listener: Box::new(listener),
        stats_sink: None,
        world: WorldConfig {
            name: "dedicated".into(),
            seed: 0x5EED,
            save_path: Some("saves/dedicated/world.redb".into()),
        },
    })?;

    let host = domain.unwrap_or_else(|| {
        if addr.ip().is_unspecified() {
            "127.0.0.1".to_string()
        } else {
            addr.ip().to_string()
        }
    });
    let target = if addr.port() == fallingsand_net::DEFAULT_PORT {
        host
    } else {
        format!("{host}:{}", addr.port())
    };
    info!("listening on {addr} (webtransport over udp)");
    println!();
    if let Some(url) = &web_client_url {
        println!("  web:  {url}/?server={target}");
    }
    println!("  host: {target}");
    if let Some(hash) = &cert_hash_hex {
        println!("  cert: {hash}");
    }
    println!();

    let control = Arc::new(ServerControl::default());
    let ctrlc_control = control.clone();
    ctrlc::set_handler(move || {
        info!("shutdown requested");
        ctrlc_control.request_stop();
    })?;
    server.run_blocking(control);
    let uptime = started.elapsed().as_secs();
    info!(
        "goodbye (uptime {}h {}m {}s)",
        uptime / 3600,
        uptime % 3600 / 60,
        uptime % 60
    );
    Ok(())
}

fn load_pem(
    cert_path: impl AsRef<Path>,
    key_path: impl AsRef<Path>,
) -> anyhow::Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>)> {
    let chain = CertificateDer::pem_file_iter(&cert_path)?.collect::<Result<Vec<_>, _>>()?;
    anyhow::ensure!(
        !chain.is_empty(),
        "no certificates in {}",
        cert_path.as_ref().display()
    );
    let key = PrivateKeyDer::from_pem_file(&key_path)?;
    Ok((chain, key))
}
