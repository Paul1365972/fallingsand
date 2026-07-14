use ed25519_dalek::{Signer, SigningKey};
use fallingsand_protocol::PlayerUuid;

#[derive(Clone)]
pub struct Identity {
    secret: [u8; 32],
    secret_external: bool,
    pub uuid: PlayerUuid,
    pub name: String,
}

impl Identity {
    pub fn resolve() -> Self {
        let (stored_secret, stored_name) = load_stored();
        let (secret, secret_external) = match configured_secret() {
            Some(secret) => (secret, true),
            None => (stored_secret.unwrap_or_else(generate_secret), false),
        };
        let uuid = uuid_for(&secret);
        let name = configured_name()
            .or(stored_name.clone())
            .unwrap_or_else(|| default_name(uuid));
        let identity = Self {
            secret,
            secret_external,
            uuid,
            name,
        };
        let fresh_secret = !secret_external && stored_secret != Some(secret);
        if fresh_secret || stored_name.as_deref() != Some(&identity.name) {
            identity.persist();
        }
        identity
    }

    pub fn set_name(&mut self, raw: &str) {
        let Some(name) = valid_name(raw) else {
            return;
        };
        if self.name == name {
            return;
        }
        self.name = name;
        self.persist();
    }

    pub fn authenticate(&self, nonce: [u8; 32]) -> ([u8; 32], [u8; 64]) {
        let signing = SigningKey::from_bytes(&self.secret);
        let public_key = signing.verifying_key().to_bytes();
        let signature = signing
            .sign(&fallingsand_protocol::identity_message(nonce))
            .to_bytes();
        (public_key, signature)
    }

    fn persist(&self) {
        let secret = (!self.secret_external).then_some(self.secret);
        store(secret, &self.name);
    }
}

fn uuid_for(secret: &[u8; 32]) -> PlayerUuid {
    let public_key = SigningKey::from_bytes(secret).verifying_key().to_bytes();
    PlayerUuid::from_public_key(&public_key)
}

fn default_name(uuid: PlayerUuid) -> String {
    format!("player{:04x}", uuid.0 as u16)
}

fn generate_secret() -> [u8; 32] {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("randomness source unavailable");
    bytes
}

fn valid_name(name: &str) -> Option<String> {
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
    super::platform::arg_value("--name").and_then(|name| valid_name(&name))
}

#[cfg(target_family = "wasm")]
fn configured_name() -> Option<String> {
    super::platform::query_param("name").and_then(|name| valid_name(&name))
}

#[cfg(not(target_family = "wasm"))]
const SECRET_PATH: &str = "saves/identity.txt";
#[cfg(not(target_family = "wasm"))]
const NAME_PATH: &str = "saves/name.txt";
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
    let secret = std::fs::read_to_string(SECRET_PATH)
        .ok()
        .and_then(|text| text.lines().next().and_then(decode_secret));
    let name = std::fs::read_to_string(NAME_PATH)
        .ok()
        .and_then(|text| valid_name(&text));
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
        .and_then(|value| valid_name(&value));
    (secret, name)
}

#[cfg(not(target_family = "wasm"))]
fn store(secret: Option<[u8; 32]>, name: &str) {
    let _ = std::fs::create_dir_all("saves");
    if let Some(secret) = secret {
        let hex: String = secret.iter().map(|byte| format!("{byte:02x}")).collect();
        if let Err(err) = std::fs::write(SECRET_PATH, hex) {
            bevy::log::warn!("failed to persist identity: {err}");
        }
    }
    if let Err(err) = std::fs::write(NAME_PATH, name) {
        bevy::log::warn!("failed to persist name: {err}");
    }
}

#[cfg(target_family = "wasm")]
fn store(secret: Option<[u8; 32]>, name: &str) {
    let Some(storage) = local_storage() else {
        bevy::log::warn!("localStorage unavailable; identity will not persist");
        return;
    };
    if let Some(secret) = secret {
        let secret: String = secret.iter().map(|byte| format!("{byte:02x}")).collect();
        let _ = storage.set_item(SECRET_KEY, &secret);
    }
    let _ = storage.set_item(NAME_KEY, name);
}

fn decode_secret(text: &str) -> Option<[u8; 32]> {
    text.parse::<super::hex::Hex32>().ok().map(|hex| hex.0)
}
