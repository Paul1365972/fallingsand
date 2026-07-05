use fallingsand_protocol::PlayerUuid;

#[derive(Clone)]
pub struct Identity {
    pub uuid: PlayerUuid,
    pub name: String,
}

pub fn load_or_create() -> Identity {
    let stored = stored_identity();
    Identity {
        uuid: configured_uuid().unwrap_or(stored.uuid),
        name: configured_name().unwrap_or(stored.name),
    }
}

pub fn update_name(raw: &str) -> Option<Identity> {
    let name = valid_name(raw.to_string())?;
    let mut identity = stored_identity();
    if identity.name != name {
        identity.name = name;
        store(&identity);
    }
    Some(identity)
}

fn stored_identity() -> Identity {
    let (stored_uuid, stored_name) = load_stored();
    let uuid = stored_uuid.unwrap_or_else(generate_uuid);
    let name = stored_name.unwrap_or_else(|| format!("player{:04x}", uuid.0 as u16));
    let identity = Identity { uuid, name };
    if stored_uuid.is_none() {
        store(&identity);
    }
    identity
}

fn generate_uuid() -> PlayerUuid {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).expect("randomness source unavailable");
    PlayerUuid(u128::from_le_bytes(bytes))
}

fn valid_name(name: String) -> Option<String> {
    let name = name.trim();
    (!name.is_empty()).then(|| name.chars().take(24).collect())
}

#[cfg(not(target_family = "wasm"))]
fn configured_uuid() -> Option<PlayerUuid> {
    arg_value("--id").and_then(|value| PlayerUuid::from_hex(&value))
}

#[cfg(not(target_family = "wasm"))]
fn configured_name() -> Option<String> {
    arg_value("--name").and_then(valid_name)
}

#[cfg(not(target_family = "wasm"))]
pub(crate) fn arg_value(flag: &str) -> Option<String> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == flag {
            return args.next();
        }
    }
    None
}

#[cfg(not(target_family = "wasm"))]
const IDENTITY_PATH: &str = "saves/identity.txt";

#[cfg(not(target_family = "wasm"))]
fn load_stored() -> (Option<PlayerUuid>, Option<String>) {
    let Ok(text) = std::fs::read_to_string(IDENTITY_PATH) else {
        return (None, None);
    };
    let mut lines = text.lines();
    let uuid = lines.next().and_then(PlayerUuid::from_hex);
    let name = lines.next().map(String::from).and_then(valid_name);
    (uuid, name)
}

#[cfg(not(target_family = "wasm"))]
fn store(identity: &Identity) {
    let _ = std::fs::create_dir_all("saves");
    let content = format!("{}\n{}\n", identity.uuid.to_hex(), identity.name);
    if let Err(err) = std::fs::write(IDENTITY_PATH, content) {
        bevy::log::warn!("failed to persist identity: {err}");
    }
}

#[cfg(all(target_family = "wasm", target_os = "unknown"))]
const UUID_KEY: &str = "fallingsand.id";
#[cfg(all(target_family = "wasm", target_os = "unknown"))]
const NAME_KEY: &str = "fallingsand.name";

#[cfg(all(target_family = "wasm", target_os = "unknown"))]
fn configured_uuid() -> Option<PlayerUuid> {
    query_param("id").and_then(|value| PlayerUuid::from_hex(&value))
}

#[cfg(all(target_family = "wasm", target_os = "unknown"))]
fn configured_name() -> Option<String> {
    query_param("name").and_then(valid_name)
}

#[cfg(all(target_family = "wasm", target_os = "unknown"))]
pub(crate) fn query_param(key: &str) -> Option<String> {
    let query = web_sys::window()?.location().search().ok()?;
    for pair in query.trim_start_matches('?').split('&') {
        let mut parts = pair.splitn(2, '=');
        if parts.next() == Some(key) {
            return parts
                .next()
                .and_then(|value| js_sys::decode_uri_component(value).ok())
                .map(String::from);
        }
    }
    None
}

#[cfg(all(target_family = "wasm", target_os = "unknown"))]
fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

#[cfg(all(target_family = "wasm", target_os = "unknown"))]
fn load_stored() -> (Option<PlayerUuid>, Option<String>) {
    let Some(storage) = local_storage() else {
        return (None, None);
    };
    let uuid = storage
        .get_item(UUID_KEY)
        .ok()
        .flatten()
        .and_then(|value| PlayerUuid::from_hex(&value));
    let name = storage
        .get_item(NAME_KEY)
        .ok()
        .flatten()
        .and_then(valid_name);
    (uuid, name)
}

#[cfg(all(target_family = "wasm", target_os = "unknown"))]
fn store(identity: &Identity) {
    let Some(storage) = local_storage() else {
        bevy::log::warn!("localStorage unavailable; identity will not persist");
        return;
    };
    let _ = storage.set_item(UUID_KEY, &identity.uuid.to_hex());
    let _ = storage.set_item(NAME_KEY, &identity.name);
}
