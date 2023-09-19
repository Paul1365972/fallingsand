use std::env;

fn main() {
    let target = env::var("TARGET").unwrap();
    if target.contains("windows") {
        embed_resource::compile("./res/windows/icon.rc", embed_resource::NONE);
    }
}
