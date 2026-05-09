//! HTTP load testing command for rszeroctl.
//!
//! Sends concurrent HTTP requests and reports latency statistics.

use anyhow::Result;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

/// Run an HTTP load test.
///
/// # Arguments
/// - `url`: Target URL (e.g. `http://localhost:8080/health`)
/// - `workers`: Number of concurrent workers
/// - `total`: Total number of requests (0 = unlimited)
/// - `duration_secs`: Test duration in seconds (0 = until `total`)
/// - `method`: HTTP method
pub async fn execute(
    url: &str,
    workers: usize,
    total: u64,
    duration_secs: u64,
    method: &str,
) -> Result<()> {
    let parsed = parse_url(url)?;
    let host = parsed.host;
    let port = parsed.port;
    let path = parsed.path;
    let duration = if duration_secs > 0 { Some(Duration::from_secs(duration_secs)) } else { None };

    println!("═══ rszero loadtest ═══");
    println!("URL:      {} {}", method.to_uppercase(), url);
    println!("Workers:  {}", workers);
    if total > 0 {
        println!("Total:    {} requests", total);
    }
    if let Some(d) = duration {
        println!("Duration: {}s", d.as_secs());
    }
    println!();

    let total_sent = Arc::new(AtomicU64::new(0));
    let total_ok = Arc::new(AtomicU64::new(0));
    let total_err = Arc::new(AtomicU64::new(0));
    let latencies = Arc::new(std::sync::Mutex::new(Vec::<u64>::new()));
    let start = Instant::now();
    let should_stop = Arc::new(std::sync::atomic::AtomicBool::new(false));

    let mut handles = Vec::new();

    for _ in 0..workers {
        let host = host.clone();
        let path = path.clone();
        let method = method.to_string();
        let sent = total_sent.clone();
        let ok = total_ok.clone();
        let err = total_err.clone();
        let lat = latencies.clone();
        let stop = should_stop.clone();

        let handle = tokio::spawn(async move {
            loop {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                if total > 0 && sent.load(Ordering::Relaxed) >= total {
                    break;
                }

                let req_start = Instant::now();
                match send_http_request(&host, port, &path, &method).await {
                    Ok(status) => {
                        let elapsed = req_start.elapsed().as_millis() as u64;
                        lat.lock().unwrap().push(elapsed);
                        sent.fetch_add(1, Ordering::Relaxed);
                        if (200..300).contains(&status) {
                            ok.fetch_add(1, Ordering::Relaxed);
                        } else {
                            err.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                    Err(e) => {
                        sent.fetch_add(1, Ordering::Relaxed);
                        err.fetch_add(1, Ordering::Relaxed);
                        if err.load(Ordering::Relaxed) <= 5 {
                            eprintln!("request error: {}", e);
                        }
                    }
                }
            }
        });
        handles.push(handle);
    }

    // Duration timeout
    if let Some(d) = duration {
        let stop = should_stop.clone();
        tokio::spawn(async move {
            tokio::time::sleep(d).await;
            stop.store(true, Ordering::Relaxed);
        });
    }

    // Total request limit
    if total > 0 {
        let stop = should_stop.clone();
        let sent = total_sent.clone();
        tokio::spawn(async move {
            loop {
                if sent.load(Ordering::Relaxed) >= total {
                    stop.store(true, Ordering::Relaxed);
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        });
    }

    // Progress display
    let sent_display = total_sent.clone();
    let ok_display = total_ok.clone();
    let err_display = total_err.clone();
    let progress_handle = tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let s = sent_display.load(Ordering::Relaxed);
            let o = ok_display.load(Ordering::Relaxed);
            let e = err_display.load(Ordering::Relaxed);
            if s == 0 { continue; }
            let elapsed = start.elapsed().as_secs_f64();
            let qps = s as f64 / elapsed;
            eprint!("\r  sent: {:>6} | ok: {:>6} | err: {:>6} | qps: {:.1}", s, o, e, qps);
        }
    });

    for h in handles {
        let _ = h.await;
    }
    should_stop.store(true, Ordering::Relaxed);
    let _ = progress_handle.await;

    let elapsed = start.elapsed();
    let sent = total_sent.load(Ordering::Relaxed);
    let ok = total_ok.load(Ordering::Relaxed);
    let err_count = total_err.load(Ordering::Relaxed);
    let qps = sent as f64 / elapsed.as_secs_f64();

    let lat_guard = latencies.lock().unwrap();
    let mut sorted = lat_guard.clone();
    drop(lat_guard);
    sorted.sort_unstable();

    println!("\n\n═══ Results ═══");
    println!("Duration:     {:.2}s", elapsed.as_secs_f64());
    println!("Total:        {}", sent);
    println!("OK:           {}", ok);
    println!("Errors:       {}", err_count);
    println!("QPS:          {:.1}", qps);

    if !sorted.is_empty() {
        println!("Latency (ms):");
        println!("  Min:    {}", sorted.first().unwrap());
        println!("  Max:    {}", sorted.last().unwrap());
        println!("  Avg:    {:.1}", sorted.iter().sum::<u64>() as f64 / sorted.len() as f64);
        println!("  P50:    {}", percentile(&sorted, 0.50));
        println!("  P90:    {}", percentile(&sorted, 0.90));
        println!("  P99:    {}", percentile(&sorted, 0.99));
    }

    Ok(())
}

struct ParsedUrl {
    host: String,
    port: u16,
    path: String,
}

fn parse_url(url: &str) -> Result<ParsedUrl> {
    // Simple parser for http://host:port/path
    let url = url.strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))
        .unwrap_or(url);

    let (host_port, path) = url.split_once('/').map(|(h, p)| (h, format!("/{}", p)))
        .unwrap_or_else(|| (url, "/".to_string()));

    let (host, port) = if let Some((h, p)) = host_port.split_once(':') {
        (h.to_string(), p.parse::<u16>()?)
    } else {
        (host_port.to_string(), 80u16)
    };

    Ok(ParsedUrl { host, port, path })
}

async fn send_http_request(host: &str, port: u16, path: &str, method: &str) -> Result<u16> {
    let addr = format!("{}:{}", host, port);
    let mut stream = timeout(Duration::from_secs(5), TcpStream::connect(&addr)).await??;

    let request = format!(
        "{} {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        method.to_uppercase(),
        path,
        host
    );

    stream.write_all(request.as_bytes()).await?;

    let mut buf = vec![0u8; 4096];
    let n = timeout(Duration::from_secs(10), stream.read(&mut buf)).await??;
    buf.truncate(n);

    let response = String::from_utf8_lossy(&buf);
    let status_line = response.lines().next().unwrap_or("");
    let status = status_line.split_whitespace().nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(0);

    Ok(status)
}

fn percentile(sorted: &[u64], p: f64) -> u64 {
    if sorted.is_empty() { return 0; }
    let idx = ((sorted.len() - 1) as f64 * p).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_url() {
        let p = parse_url("http://localhost:8080/health").unwrap();
        assert_eq!(p.host, "localhost");
        assert_eq!(p.port, 8080);
        assert_eq!(p.path, "/health");
    }

    #[test]
    fn test_parse_url_no_port() {
        let p = parse_url("http://localhost/api").unwrap();
        assert_eq!(p.host, "localhost");
        assert_eq!(p.port, 80);
    }

    #[test]
    fn test_percentile() {
        let data = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        assert_eq!(percentile(&data, 0.0), 1);
        assert_eq!(percentile(&data, 0.5), 6);
        assert_eq!(percentile(&data, 1.0), 10);
    }
}
