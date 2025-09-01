use aws_config::BehaviorVersion;
use aws_sdk_s3::presigning::PresigningConfig;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::Client;
use chromiumoxide::cdp::browser_protocol::page::PrintToPdfParams;
use chromiumoxide::page::ScreenshotParams;
use chromiumoxide::{Browser, BrowserConfig};
use futures::StreamExt;
use lambda_runtime::{service_fn, Error, LambdaEvent};
use rayon::prelude::*;
use serde::Deserialize;
use serde_json::{json, Value};
use std::cmp::PartialEq;
use std::fmt::Display;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use std::{env, fs, io};
use uuid::Uuid;

// const GS: &str = "/opt/gs";
const GS: &str = "gswin64c";

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

    let url = if event.capture_type == CaptureType::PDF {
        upload(split_compress_pdf(data).await?, &bucket, event.capture_type).await?
    } else {
        upload(data, &bucket, event.capture_type).await?
    };

    Ok(json!({ "url": url }))
}

async fn split_compress_pdf(data: Vec<u8>) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let file_path = "/tmp/output.pdf";

    let mut file = File::create(file_path)?;
    file.write_all(&data)?;

    let cores = num_cpus::get();
    let page_count = page_count(file_path);

    let compressed_chunk: Vec<_> =
        split_pdf(file_path, page_count, (page_count + cores - 1) / cores)
            .await?
            .par_iter()
            .map(|path| compress_pdf(path).unwrap().to_string_lossy().into_owned())
            .collect();

    merge_pdfs(&compressed_chunk, file_path).unwrap();

    Ok(fs::read(file_path)?)
}

fn merge_pdfs(input_files: &[String], output_file: &str) -> io::Result<()> {
    let files = format!("-sOutputFile={}", output_file);
    let mut args = vec!["-dBATCH", "-dNOPAUSE", "-q", "-sDEVICE=pdfwrite", &files];

    for file in input_files {
        args.push(file);
    }

    let status = Command::new(GS).args(&args).status()?;

    if status.success() {
        println!("Merged PDFs into {}", output_file);
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::Other,
            "Ghostscript merge failed",
        ))
    }
}

fn compress_pdf(file_path: &PathBuf) -> io::Result<PathBuf> {
    println!("compressing {}", file_path.to_string_lossy());
    let compressed_dir = Path::new("/tmp/compressed_chunks");

    if !compressed_dir.exists() {
        fs::create_dir_all(compressed_dir)?;
    }

    let compressed_file = compressed_dir.join(file_path.file_name().unwrap());

    let status = Command::new(GS)
        .args([
            "-sDEVICE=pdfwrite",
            "-dCompatibilityLevel=1.4",
            "-dPDFSETTINGS=/ebook",
            "-dNOPAUSE",
            "-dQUIET",
            "-dBATCH",
            &format!("-sOutputFile={}", compressed_file.to_string_lossy()),
            file_path.to_str().unwrap(),
        ])
        .status()?;

    if status.success() {
        println!("compressed {}", file_path.to_string_lossy());
        Ok(compressed_file)
    } else {
        Err(io::Error::new(
            io::ErrorKind::Other,
            "Ghostscript compression failed",
        ))
    }
}

async fn split_pdf(file_path: &str, pages: usize, chunk: usize) -> io::Result<Vec<PathBuf>> {
    let out_dir = Path::new("/tmp/temp_chunks");
    if !out_dir.exists() {
        fs::create_dir_all(out_dir)?;
    }

    let jobs: Vec<(usize, usize, PathBuf)> = (1..=pages)
        .step_by(chunk)
        .map(|start| {
            let end = usize::min(start + chunk - 1, pages);
            let out_file = out_dir.join(format!("chunk_{}-{}.pdf", start, end));

            (start, end, out_file)
        })
        .collect();

    let results: Vec<_> = jobs
        .par_iter()
        .map(|(start, end, out_file)| {
            let status = Command::new(GS)
                .args([
                    "-sDEVICE=pdfwrite",
                    "-dNOPAUSE",
                    "-dBATCH",
                    "-dSAFER",
                    "-dQUIET",
                    &format!("-dFirstPage={}", start),
                    &format!("-dLastPage={}", end),
                    &format!("-sOutputFile={}", out_file.to_string_lossy()),
                    file_path,
                ])
                .status();

            match status {
                Ok(s) if s.success() => Ok(out_file.clone()),
                Ok(_) => Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!("Ghostscript failed for pages {}-{}", start, end),
                )),
                Err(e) => Err(e),
            }
        })
        .collect();

    let mut chunks = Vec::new();

    for r in results {
        chunks.push(r?);
    }

    Ok(chunks)
}

fn page_count(filepath: &str) -> usize {
    let output = Command::new(GS)
        .args([
            "-q",
            "-dNOSAFER",
            "-dNODISPLAY",
            "-c",
            &format!("({}) (r) file runpdfbegin pdfpagecount = quit", filepath),
        ])
        .output()
        .unwrap();

    let page_count_str = String::from_utf8_lossy(&output.stdout);

    page_count_str.trim().parse().unwrap_or(0)
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
            // .chrome_executable("/opt/chromium")
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

        let _ = upload(
            split_compress_pdf(data).await.unwrap(),
            &env::var("BUCKET").unwrap(),
            CaptureType::PDF,
        )
        .await
        .unwrap();
    }
}
