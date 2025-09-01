use aws_config::BehaviorVersion;
use aws_sdk_s3::presigning::PresigningConfig;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::Client;
use chromiumoxide::cdp::browser_protocol::page::PrintToPdfParams;
use chromiumoxide::page::ScreenshotParams;
use chromiumoxide::{Browser, BrowserConfig};
use futures::StreamExt;
use lambda_runtime::{service_fn, Error, LambdaEvent};
use serde::Deserialize;
use serde_json::{json, Value};
use std::cmp::PartialEq;
use std::env;
use std::fmt::Display;
use std::time::Duration;
use uuid::Uuid;

#[derive(PartialEq, Eq, Deserialize, Clone)]
enum CaptureType {
    PDF,
    PNG,
}

impl Display for CaptureType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            CaptureType::PDF => "pdf".to_string(),
            CaptureType::PNG => "png".to_string(),
        };
        write!(f, "{}", str)
    }
}

#[derive(Deserialize)]
struct Event {
    url: String,
    capture_type: CaptureType,
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let func = service_fn(func);
    lambda_runtime::run(func).await?;

    Ok(())
}

async fn func(event: LambdaEvent<Value>) -> Result<Value, Box<dyn std::error::Error>> {
    let (payload, _) = event.into_parts();
    let event: Event = serde_json::from_value(payload)?;

    let bucket = env::var("BUCKET").unwrap();

    let data = capture(&event.url, event.capture_type.clone()).await?;

    let url = upload(data, &bucket, event.capture_type).await?;

    Ok(json!({ "url": url }))
}

async fn upload(
    data: Vec<u8>,
    bucket: &str,
    capture_type: CaptureType,
) -> Result<String, Box<dyn std::error::Error>> {
    let config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let client = Client::new(&config);

    let key = format!("temp/{}.{}", Uuid::new_v4(), capture_type);

    client
        .put_object()
        .bucket(bucket)
        .key(&key)
        .body(ByteStream::from(data))
        .send()
        .await?;

    let presign_config = PresigningConfig::expires_in(Duration::from_secs(3600))?;

    let presigned_req = client
        .get_object()
        .bucket(bucket)
        .key(&key)
        .presigned(presign_config)
        .await?;

    Ok(presigned_req.uri().to_string())
}

async fn capture(
    url: &str,
    capture_type: CaptureType,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let (browser, mut handler) = Browser::launch(
        BrowserConfig::builder()
            .chrome_executable("/opt/chromium")
            // .no_sandbox()
            .args([
                "--allow-pre-commit-input",
                "--disable-background-networking",
                "--disable-background-timer-throttling",
                "--disable-backgrounding-occluded-windows",
                "--disable-breakpad",
                "--disable-client-side-phishing-detection",
                "--disable-component-extensions-with-background-pages",
                "--disable-component-update",
                "--disable-default-apps",
                "--disable-dev-shm-usage",
                "--disable-extensions",
                "--disable-hang-monitor",
                "--disable-ipc-flooding-protection",
                "--disable-popup-blocking",
                "--disable-prompt-on-repost",
                "--disable-renderer-backgrounding",
                "--disable-sync",
                "--enable-automation",
                "--enable-blink-features=IdleDetection",
                "--export-tagged-pdf",
                "--force-color-profile=srgb",
                "--metrics-recording-only",
                "--no-first-run",
                "--password-store=basic",
                "--use-mock-keychain",
                "--disable-domain-reliability",
                "--disable-print-preview",
                "--disable-speech-api",
                "--disk-cache-size=33554432",
                "--mute-audio",
                "--no-default-browser-check",
                "--no-pings",
                "--single-process",
                "--font-render-hinting=none",
                "--disable-features=Translate,BackForwardCache,AcceptCHFrame,MediaRouter,OptimizationHints,AudioServiceOutOfProcess,IsolateOrigins,site-per-process",
                "--enable-features=NetworkServiceInProcess2,SharedArrayBuffer",
                "--hide-scrollbars",
                "--ignore-gpu-blocklist",
                "--in-process-gpu",
                "--window-size=1920,1080",
                "--use-gl=angle",
                "--use-angle=swiftshader",
                "--allow-running-insecure-content",
                "--disable-setuid-sandbox",
                "--disable-site-isolation-trials",
                "--disable-web-security",
                "--no-sandbox",
                "--no-zygote",
                "--headless=shell"
            ])
            .build()?,
    )
    .await?;

    tokio::spawn(async move { while let Some(_) = handler.next().await {} });

    let page = browser.new_page(url).await?;
    page.wait_for_navigation().await?;

    if capture_type == CaptureType::PDF {
        Ok(page
            .pdf(
                PrintToPdfParams::builder()
                    .print_background(true)
                    .prefer_css_page_size(true)
                    .build(),
            )
            .await?)
    } else {
        Ok(page
            .screenshot(
                ScreenshotParams::builder()
                    .capture_beyond_viewport(true)
                    .full_page(true)
                    .build(),
            )
            .await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_generate_pdf() {
        let data = capture("https://www.rust-lang.org/", CaptureType::PDF)
            .await
            .unwrap();

        let _ = upload(data, &env::var("BUCKET").unwrap(), CaptureType::PDF)
            .await
            .unwrap();
    }
}
