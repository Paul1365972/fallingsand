use fallingsand_core::MaterialRegistry;
use fallingsand_net::wt_native::WtListener;
use fallingsand_server::{Server, ServerConfig, ServerControl, WorldConfig};
use rcgen::{CertificateParams, KeyPair};
use rustls_pki_types::PrivateKeyDer;
use sha2::{Digest, Sha256};
use std::sync::Arc;

const DEFAULT_ADDR: &str = "0.0.0.0:4433";
const CERT_VALIDITY_DAYS: i64 = 13;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let addr: std::net::SocketAddr = args
        .next()
        .unwrap_or_else(|| DEFAULT_ADDR.to_string())
        .parse()?;

    let materials = include_str!("../../../data/materials.ron");
    let biomes = include_str!("../../../data/biomes.ron");
    let registry = Arc::new(MaterialRegistry::from_ron(materials)?);

    let mut params = CertificateParams::new(vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        addr.ip().to_string(),
    ])?;
    params.not_before = time::OffsetDateTime::now_utc() - time::Duration::hours(1);
    params.not_after = time::OffsetDateTime::now_utc() + time::Duration::days(CERT_VALIDITY_DAYS);
    let key = KeyPair::generate()?;
    let cert = params.self_signed(&key)?;
    let cert_hash = Sha256::digest(cert.der().as_ref());
    let cert_hash_hex: String = cert_hash.iter().map(|b| format!("{b:02x}")).collect();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let listener = WtListener::bind(
        runtime.handle().clone(),
        addr,
        vec![cert.der().clone()],
        PrivateKeyDer::try_from(key.serialize_der()).map_err(anyhow::Error::msg)?,
    )?;

    let mut server = Server::new(ServerConfig {
        registry,
        listener: Box::new(listener),
        stats_sink: None,
        world: WorldConfig {
            name: "dedicated".into(),
            seed: 0x5EED,
            save_path: Some("saves/dedicated/world.redb".into()),
            biomes_source: biomes.into(),
        },
    })?;

    println!("fallingsand_server listening on https://{addr} (WebTransport)");
    println!("certificate sha-256: {cert_hash_hex}");
    let host = if addr.ip().is_unspecified() {
        "127.0.0.1".to_string()
    } else {
        addr.ip().to_string()
    };
    println!(
        "connect: fallingsand --connect https://{host}:{} --cert-hash {cert_hash_hex}",
        addr.port()
    );

    let control = Arc::new(ServerControl::default());
    let ctrlc_control = control.clone();
    ctrlc::set_handler(move || ctrlc_control.request_stop())?;
    server.run_blocking(control);
    println!("world saved, bye");
    Ok(())
}
