use ed25519_dalek::{Signer, SigningKey};
use fallingsand_protocol::PlayerUuid;

#[derive(Clone)]
pub struct Identity {
    secret: [u8; 32],
    pub uuid: PlayerUuid,
    pub name: String,
}

pub fn load_or_create() -> Identity {
    let configured_name = configured_name();
    if let Some(secret) = configured_secret() {
        let uuid = uuid_for(&public_key(&secret));
        let name = configured_name.unwrap_or_else(|| default_name(uuid));
        return Identity { secret, uuid, name };
    }
    let stored = stored_identity();
    Identity {
        secret: stored.secret,
        uuid: stored.uuid,
        name: configured_name.unwrap_or(stored.name),
    }
}

pub fn authenticate(identity: &Identity, nonce: [u8; 32]) -> ([u8; 32], [u8; 64]) {
    let signing = SigningKey::from_bytes(&identity.secret);
    let public_key = signing.verifying_key().to_bytes();
    let signature = signing
        .sign(&fallingsand_protocol::identity_message(nonce))
        .to_bytes();
    (public_key, signature)
}

fn public_key(secret: &[u8; 32]) -> [u8; 32] {
    SigningKey::from_bytes(secret).verifying_key().to_bytes()
}

fn uuid_for(public_key: &[u8; 32]) -> PlayerUuid {
    PlayerUuid::from_public_key(public_key)
}

fn default_name(uuid: PlayerUuid) -> String {
    format!("player{:04x}", uuid.0 as u16)
}

pub fn update_name(raw: &str) {
    let Some(name) = valid_name(raw.to_string()) else {
        return;
    };
    let mut identity = stored_identity();
    if identity.name != name {
        identity.name = name;
        store(&identity);
    }
}

fn stored_identity() -> Identity {
    let (stored_secret, stored_name) = load_stored();
    let secret = stored_secret.unwrap_or_else(generate_secret);
    let uuid = uuid_for(&public_key(&secret));
    let name = stored_name.unwrap_or_else(|| default_name(uuid));
    let identity = Identity { secret, uuid, name };
    if stored_secret.is_none() {
        store(&identity);
    }
    identity
}

fn generate_secret() -> [u8; 32] {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("randomness source unavailable");
    bytes
}

fn valid_name(name: String) -> Option<String> {
    let name = name.trim();
    (!name.is_empty()).then(|| name.chars().take(24).collect())
}

#[cfg(not(target_family = "wasm"))]
fn configured_secret() -> Option<[u8; 32]> {
    let path = super::platform::arg_value("--identity-key-file")?;
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) => {
            bevy::log::error!("failed to read identity key file {path}: {err}");
            return None;
        }
    };
    match text.trim().parse::<super::hex::Hex32>() {
        Ok(hex) => Some(hex.0),
        Err(err) => {
            bevy::log::error!("invalid identity key file {path}: {err}");
            None
        }
    }
}

#[cfg(target_family = "wasm")]
fn configured_secret() -> Option<[u8; 32]> {
    None
}

#[cfg(not(target_family = "wasm"))]
fn configured_name() -> Option<String> {
    super::platform::arg_value("--name").and_then(valid_name)
}

#[cfg(target_family = "wasm")]
fn configured_name() -> Option<String> {
    super::platform::query_param("name").and_then(valid_name)
}

#[cfg(not(target_family = "wasm"))]
const IDENTITY_PATH: &str = "saves/identity.txt";
#[cfg(target_family = "wasm")]
const SECRET_KEY: &str = "fallingsand.identity_key";
#[cfg(target_family = "wasm")]
const NAME_KEY: &str = "fallingsand.name";

#[cfg(target_family = "wasm")]
fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

#[cfg(not(target_family = "wasm"))]
fn load_stored() -> (Option<[u8; 32]>, Option<String>) {
    let Ok(text) = std::fs::read_to_string(IDENTITY_PATH) else {
        return (None, None);
    };
    let mut lines = text.lines();
    let secret = lines.next().and_then(decode_secret);
    let name = lines.next().map(String::from).and_then(valid_name);
    (secret, name)
}

#[cfg(target_family = "wasm")]
fn load_stored() -> (Option<[u8; 32]>, Option<String>) {
    let Some(storage) = local_storage() else {
        return (None, None);
    };
    let secret = storage
        .get_item(SECRET_KEY)
        .ok()
        .flatten()
        .and_then(|value| decode_secret(&value));
    let name = storage
        .get_item(NAME_KEY)
        .ok()
        .flatten()
        .and_then(valid_name);
    (secret, name)
}

#[cfg(not(target_family = "wasm"))]
fn store(identity: &Identity) {
    let _ = std::fs::create_dir_all("saves");
    let secret: String = identity
        .secret
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let content = format!("{secret}\n{}\n", identity.name);
    if let Err(err) = std::fs::write(IDENTITY_PATH, content) {
        bevy::log::warn!("failed to persist identity: {err}");
    }
}

#[cfg(target_family = "wasm")]
fn store(identity: &Identity) {
    let Some(storage) = local_storage() else {
        bevy::log::warn!("localStorage unavailable; identity will not persist");
        return;
    };
    let secret: String = identity
        .secret
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let _ = storage.set_item(SECRET_KEY, &secret);
    let _ = storage.set_item(NAME_KEY, &identity.name);
}

fn decode_secret(text: &str) -> Option<[u8; 32]> {
    text.parse::<super::hex::Hex32>().ok().map(|hex| hex.0)
}
