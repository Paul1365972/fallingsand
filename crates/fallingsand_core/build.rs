fn main() {
    let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR is set"))
        .join("content.rs");
    let generated = fallingsand_content::compile().expect("content must compile");
    std::fs::write(output, generated).expect("generated content can be written");
}
