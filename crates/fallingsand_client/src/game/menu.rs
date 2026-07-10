pub struct MenuState {
    pub worlds: Vec<String>,
    pub pending_delete: Option<String>,
}

impl MenuState {
    pub(super) fn scan() -> Self {
        Self {
            worlds: list_worlds(),
            pending_delete: None,
        }
    }

    pub(super) fn rescan(&mut self) {
        self.worlds = list_worlds();
        self.pending_delete = None;
    }

    pub(super) fn confirm_delete(&mut self, name: String) {
        if self.pending_delete.as_deref() == Some(&name) {
            delete_world(&name);
            self.pending_delete = None;
            self.worlds = list_worlds();
        } else {
            self.pending_delete = Some(name);
        }
    }
}

pub fn sanitize_world_name(raw: &str) -> Option<String> {
    let name: String = raw
        .trim()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ' '))
        .collect();
    let name = name.trim().to_string();
    (!name.is_empty()).then_some(name)
}

#[cfg(not(target_family = "wasm"))]
fn list_worlds() -> Vec<String> {
    let Ok(entries) = std::fs::read_dir("saves") else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|entry| entry.path().join("world.redb").is_file())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    names.sort();
    names
}

#[cfg(target_family = "wasm")]
fn list_worlds() -> Vec<String> {
    Vec::new()
}

#[cfg(not(target_family = "wasm"))]
fn delete_world(name: &str) {
    if sanitize_world_name(name).as_deref() != Some(name) {
        return;
    }
    let path = std::path::Path::new("saves").join(name);
    if path.join("world.redb").is_file()
        && let Err(err) = std::fs::remove_dir_all(&path)
    {
        bevy::log::error!("failed to delete world {name}: {err}");
    }
}

#[cfg(target_family = "wasm")]
fn delete_world(_name: &str) {}
