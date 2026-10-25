use core::sync::atomic::Ordering;

use picoserve::extract::Json as JsonRequest;
use picoserve::response::json::Json as JsonResponse;
use picoserve::response::sse::{EventSource, EventWriter};
use picoserve::response::{self, File, StatusCode};
use picoserve::routing::{get, get_service, post};
use serde::Serialize;

use crate::desk::gpio::STOP_REQUESTED;
use crate::desk::state::{DeskCommand, DeskState, STATE};
use crate::desk::COMMANDS;
use crate::net::STATIC_IP;

/// The whole frontend is a single self-contained HTML document produced by
/// `pnpm --dir web build`. Keeping it in flash means no filesystem and no heap.
pub static INDEX_HTML: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../web/dist/index.html"
));

macro_rules! embedded_asset {
    ($name:ident, $path:literal) => {
        pub static $name: &[u8] =
            include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../web/dist/", $path));
    };
}

embedded_asset!(MANIFEST, "manifest.webmanifest");
embedded_asset!(ICON_SVG, "icon.svg");
embedded_asset!(ICON_180, "icon-180.png");
embedded_asset!(ICON_192, "icon-192.png");
embedded_asset!(ICON_512, "icon-512.png");
embedded_asset!(ICON_MASKABLE_512, "icon-maskable-512.png");
embedded_asset!(FAVICON_SVG, "favicon.svg");
embedded_asset!(FAVICON_32, "favicon-32.png");

pub const WEB_TASK_POOL_SIZE: usize = 4;

const NO_CACHE: &[(&str, &str)] = &[("Cache-Control", "no-cache")];
const CACHE_ICON: &[(&str, &str)] = &[("Cache-Control", "public, max-age=86400")];

#[derive(Serialize)]
struct Ack {
    accepted: bool,
}

#[derive(Serialize)]
struct Info {
    name: &'static str,
    version: &'static str,
    ip: &'static str,
}

pub struct AppProps;

impl picoserve::AppBuilder for AppProps {
    type PathRouter = impl picoserve::routing::PathRouter;

    fn build_app(self) -> picoserve::Router<Self::PathRouter> {
        picoserve::Router::new()
            .route(
                "/",
                get_service(File::with_content_type_and_headers(
                    "text/html; charset=utf-8",
                    INDEX_HTML,
                    NO_CACHE,
                )),
            )
            .route(
                "/manifest.webmanifest",
                get_service(File::with_content_type_and_headers(
                    "application/manifest+json",
                    MANIFEST,
                    NO_CACHE,
                )),
            )
            .route(
                "/icon.svg",
                get_service(File::with_content_type_and_headers(
                    "image/svg+xml",
                    ICON_SVG,
                    CACHE_ICON,
                )),
            )
            .route(
                "/icon-180.png",
                get_service(File::with_content_type_and_headers(
                    "image/png",
                    ICON_180,
                    CACHE_ICON,
                )),
            )
            .route(
                "/icon-192.png",
                get_service(File::with_content_type_and_headers(
                    "image/png",
                    ICON_192,
                    CACHE_ICON,
                )),
            )
            .route(
                "/icon-512.png",
                get_service(File::with_content_type_and_headers(
                    "image/png",
                    ICON_512,
                    CACHE_ICON,
                )),
            )
            .route(
                "/icon-maskable-512.png",
                get_service(File::with_content_type_and_headers(
                    "image/png",
                    ICON_MASKABLE_512,
                    CACHE_ICON,
                )),
            )
            .route(
                "/favicon.svg",
                get_service(File::with_content_type_and_headers(
                    "image/svg+xml",
                    FAVICON_SVG,
                    CACHE_ICON,
                )),
            )
            .route(
                "/favicon-32.png",
                get_service(File::with_content_type_and_headers(
                    "image/png",
                    FAVICON_32,
                    CACHE_ICON,
                )),
            )
            .route("/api/state", get(get_state))
            .route("/api/events", get(get_events))
            .route("/api/command", post(post_command))
            .route("/api/stop", post(post_stop))
            .route("/api/info", get(get_info))
    }
}

async fn get_state() -> impl response::IntoResponse {
    let mut receiver = STATE
        .receiver()
        .expect("STATE watch receiver slots exhausted");
    let state = receiver.try_get().unwrap_or(DeskState::UNKNOWN);
    JsonResponse(state)
}

struct StateEvents(
    embassy_sync::watch::Receiver<
        'static,
        embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex,
        DeskState,
        8,
    >,
);

async fn write_state<W: picoserve::io::Write>(
    writer: &mut EventWriter<'_, W>,
    state: DeskState,
) -> Result<(), W::Error> {
    let json: Result<serde_json_core::heapless::String<256>, _> =
        serde_json_core::to_string(&state);

    if let Ok(json) = json {
        writer.write_event("state", json.as_str()).await?;
    }

    Ok(())
}

impl EventSource for StateEvents {
    async fn write_events<W: picoserve::io::Write>(
        mut self,
        mut writer: EventWriter<'_, W>,
    ) -> Result<(), W::Error> {
        // Send the current snapshot immediately so a client that connects
        // between state changes does not have to wait for the next move.
        if let Some(state) = self.0.try_get() {
            write_state(&mut writer, state).await?;
        }

        loop {
            match embassy_time::with_timeout(
                embassy_time::Duration::from_secs(15),
                self.0.changed(),
            )
            .await
            {
                Ok(state) => write_state(&mut writer, state).await?,
                Err(_) => writer.write_keepalive().await?,
            }
        }
    }
}

async fn get_events() -> impl response::IntoResponse {
    let receiver = STATE
        .receiver()
        .expect("STATE watch receiver slots exhausted");
    response::EventStream(StateEvents(receiver))
}

async fn post_command(
    JsonRequest(command): JsonRequest<DeskCommand>,
) -> impl response::IntoResponse {
    let accepted = COMMANDS.try_send(command).is_ok();
    let status = if accepted {
        StatusCode::ACCEPTED
    } else {
        StatusCode::CONFLICT
    };

    JsonResponse(Ack { accepted })
        .into_response()
        .with_status_code(status)
}

async fn post_stop() -> impl response::IntoResponse {
    STOP_REQUESTED.store(true, Ordering::SeqCst);
    JsonResponse(Ack { accepted: true })
        .into_response()
        .with_status_code(StatusCode::ACCEPTED)
}

async fn get_info() -> impl response::IntoResponse {
    JsonResponse(Info {
        name: "sven",
        version: env!("CARGO_PKG_VERSION"),
        ip: STATIC_IP,
    })
}

#[embassy_executor::task(pool_size = WEB_TASK_POOL_SIZE)]
pub async fn web_task(
    task_id: usize,
    stack: embassy_net::Stack<'static>,
    app: &'static picoserve::AppRouter<AppProps>,
    config: &'static picoserve::Config<embassy_time::Duration>,
) -> ! {
    let mut tcp_rx = [0u8; 1024];
    let mut tcp_tx = [0u8; 2048];
    let mut http = [0u8; 2048];

    picoserve::Server::new(app, config, &mut http)
        .listen_and_serve(task_id, stack, 80, &mut tcp_rx, &mut tcp_tx)
        .await
        .into_never()
}
