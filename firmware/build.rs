use std::path::Path;

/// Variables the firmware reads at compile time.
const ENV_KEYS: [&str; 9] = [
    "SSID",
    "PASSWORD",
    "STATIC_IP",
    "PREFIX",
    "GATEWAY",
    "MQTT_HOST",
    "MQTT_PORT",
    "MQTT_USERNAME",
    "MQTT_PASSWORD",
];
/// Variables that must be present, otherwise compilation fails.
const REQUIRED_KEYS: [&str; 2] = ["SSID", "PASSWORD"];

fn main() {
    println!("cargo:rustc-link-arg=-Tlinkall.x");

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is unset");
    let env_file = Path::new(&manifest_dir).join(".env");
    println!("cargo:rerun-if-changed={}", env_file.display());
    println!("cargo:rerun-if-changed=../web/dist");
    for key in ENV_KEYS {
        println!("cargo:rerun-if-env-changed={key}");
    }

    // Load firmware/.env for the build script. Values already present in the
    // caller's environment win, so exported overrides still work.
    match dotenvy::from_path(&env_file) {
        Ok(()) => {}
        Err(dotenvy::Error::Io(err)) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => panic!("failed to read {}: {err}", env_file.display()),
    }

    // A build script's environment changes do not reach rustc on their own.
    // Forward each value explicitly so env!/option_env! can see it.
    for key in ENV_KEYS {
        match std::env::var(key) {
            Ok(value) => println!("cargo:rustc-env={key}={value}"),
            Err(_) => {
                if REQUIRED_KEYS.contains(&key) {
                    panic!(
                        "{key} is not set. Copy firmware/.env.example to firmware/.env \
                         and fill in your WiFi details."
                    );
                }
            }
        }
    }

    let dist = Path::new("../web/dist");
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
