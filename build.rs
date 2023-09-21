use std::env;

fn main() {
    println!("cargo:rerun-if-changed=./res/windows/");

    let target = env::var("TARGET").unwrap();
    if target.contains("windows") {
        embed_resource::compile("./res/windows/icon.rc", embed_resource::NONE);
    }
}
