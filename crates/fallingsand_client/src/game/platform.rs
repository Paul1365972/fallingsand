#[cfg(not(target_family = "wasm"))]
pub(super) fn arg_value(flag: &str) -> Option<String> {
    std::env::args()
        .skip_while(|arg| arg.as_str() != flag)
        .nth(1)
}

#[cfg(target_family = "wasm")]
pub(super) fn query_param(key: &str) -> Option<String> {
    let query = web_sys::window()?.location().search().ok()?;
    web_sys::UrlSearchParams::new_with_str(&query)
        .ok()?
        .get(key)
}

pub(crate) fn cli_world_name() -> Option<String> {
    #[cfg(not(target_family = "wasm"))]
    {
        arg_value("--world")
    }
    #[cfg(target_family = "wasm")]
    {
        None
    }
}

pub(crate) fn default_server() -> String {
    #[cfg(target_family = "wasm")]
    if let Some(server) = query_param("server") {
        return server;
    }
    option_env!("FALLINGSAND_SERVER")
        .unwrap_or_default()
        .to_string()
}
