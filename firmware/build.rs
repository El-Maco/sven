fn main() {
    println!("cargo:rustc-link-arg=-Tlinkall.x");
    println!("cargo:rerun-if-changed=../web/dist");
    println!("cargo:rerun-if-env-changed=SSID");
    println!("cargo:rerun-if-env-changed=PASSWORD");
    println!("cargo:rerun-if-env-changed=STATIC_IP");
    println!("cargo:rerun-if-env-changed=PREFIX");
    println!("cargo:rerun-if-env-changed=GATEWAY");

    let dist = std::path::Path::new("../web/dist");
    let required = [
        "index.html",
        "manifest.webmanifest",
        "icon.svg",
        "icon-180.png",
        "icon-192.png",
        "icon-512.png",
        "icon-maskable-512.png",
        "favicon.svg",
        "favicon-32.png",
    ];

    for file in required {
        if !dist.join(file).exists() {
            panic!(
                "web/dist/{file} is missing. Run `pnpm --dir web build` (or `just web`) before building the firmware."
            );
        }
    }
}
