fn main() {
    println!("cargo:rerun-if-changed=res/windows");

    let target = std::env::var("TARGET").unwrap();
    if target.contains("windows") {
        embed_resource::compile("res/windows/icon.rc", embed_resource::NONE)
            .manifest_optional()
            .unwrap();
    }
}
