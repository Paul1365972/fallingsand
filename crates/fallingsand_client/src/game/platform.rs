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
