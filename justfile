set shell := ["bash", "-euo", "pipefail", "-c"]

# espup writes this file. Sourcing it puts the Xtensa GCC linker in PATH.
esp_env := "if [ -f \"$HOME/export-esp.sh\" ]; then . \"$HOME/export-esp.sh\"; fi"

# Build the single-file web UI into web/dist/index.html.
web:
    pnpm --dir web install --frozen-lockfile
    pnpm --dir web build

# Run the Vite dev server against the real desk.
dev-web:
    SVEN_DESK_HOST=http://192.168.1.10 pnpm --dir web dev

# Compile the firmware. Requires firmware/.env for SSID and PASSWORD.
firmware: web
    {{esp_env}}; cd firmware; set -a; . .env; set +a; cargo build --release

# Build the UI, build the firmware, and flash it.
flash: web
    {{esp_env}}; cd firmware; set -a; . .env; set +a; cargo run --release

check:
    {{esp_env}}; cd firmware; set -a; . .env; set +a; cargo check

fmt:
    {{esp_env}}; cd firmware; cargo fmt

typecheck:
    pnpm --dir web typecheck
