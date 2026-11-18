use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::{Method, Request, Response, StatusCode};
use axum::middleware::{self, Next};
use axum::routing::post;
use ember_server::{app, AppState, MemoryStorage, RateLimitConfig};
use tokio::net::TcpListener;

static TAMPER_NEXT_PART: AtomicBool = AtomicBool::new(false);
static FAIL_NEXT_PART_REQUESTS: AtomicUsize = AtomicUsize::new(0);

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let relay_state = AppState::with_pow_and_rate_limit(
        Arc::new(MemoryStorage::new()),
        0,
        RateLimitConfig::default(),
    );
    let router = app(relay_state)
        .route("/__test/tamper-next-part", post(arm_tamper))
        .route("/__test/fail-next-part", post(arm_part_failure))
        .layer(middleware::from_fn(tamper_part_response));

    let listener = TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], 8080))).await?;
    println!("Android integration relay ready on port 8080");
    axum::serve(listener, router).await?;
    Ok(())
}

async fn arm_tamper() -> StatusCode {
    TAMPER_NEXT_PART.store(true, Ordering::SeqCst);
    StatusCode::NO_CONTENT
}

async fn arm_part_failure() -> StatusCode {
    FAIL_NEXT_PART_REQUESTS.store(1, Ordering::SeqCst);
    StatusCode::NO_CONTENT
}

async fn tamper_part_response(request: Request<Body>, next: Next) -> Response<Body> {
    let is_part_download =
        request.method() == Method::GET && request.uri().path().contains("/parts/");
    let is_part_transfer = request.uri().path().contains("/parts/");
    if is_part_transfer
        && FAIL_NEXT_PART_REQUESTS
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |remaining| {
                if remaining > 0 {
                    Some(remaining - 1)
                } else {
                    None
                }
            })
            .is_ok()
    {
        let (_, body) = request.into_parts();
        let _ = to_bytes(body, 2_000_000).await;
        return Response::builder()
            .status(StatusCode::SERVICE_UNAVAILABLE)
            .body(Body::empty())
            .expect("empty service-unavailable response is valid");
    }
    let should_tamper = is_part_download && TAMPER_NEXT_PART.swap(false, Ordering::SeqCst);
    let response = next.run(request).await;
    if !should_tamper || !response.status().is_success() {
        return response;
    }

    let (parts, body) = response.into_parts();
    let Ok(mut bytes) = to_bytes(body, 2_000_000).await else {
        return Response::from_parts(parts, Body::empty());
    };
    if !bytes.is_empty() {
        let mut changed = bytes.to_vec();
        changed[0] ^= 0x01;
        bytes = changed.into();
    }
    Response::from_parts(parts, Body::from(bytes))
}
