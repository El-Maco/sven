fn main() {
    println!("cargo:rustc-link-arg=-Tlinkall.x");
    println!("cargo:rerun-if-changed=../web/dist/index.html");

    let index = std::path::Path::new("../web/dist/index.html");
    if !index.exists() {
        panic!(
            "web/dist/index.html is missing. Run `pnpm --dir web build` (or `just web`) before building the firmware."
        );
    }
}
