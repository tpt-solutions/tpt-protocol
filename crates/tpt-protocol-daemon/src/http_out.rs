//! Plain-HTTP POST for the metric sinks (InfluxDB and OpenTelemetry).
//!
//! HTTP/1.1 over a TCP connection the sink opens for each batch, with
//! `Connection: close`. There is no TLS, so `https://` is refused, and no
//! keep-alive, retry or proxy support. A batch that cannot be delivered within
//! [`IO_TIMEOUT`] is an error for the caller to log.

use std::time::Duration;

use anyhow::{anyhow, bail, Result};

/// How long one request may take, from connect to the end of the reply.
pub const IO_TIMEOUT: Duration = Duration::from_secs(5);

/// Where a batch is posted.
#[derive(Debug, Clone, PartialEq)]
pub struct Target {
    /// Host name or IP address, without brackets for IPv6.
    pub host: String,
    pub port: u16,
    /// Request path and query.
    pub path: String,
}

/// Parses `http://HOST[:PORT][/PATH[?QUERY]]`. `flag` names the command-line
/// option in error messages. The port defaults to 80 and the path to `/`.
pub fn parse_http_url(url: &str, flag: &str) -> Result<Target> {
    let rest = url.strip_prefix("http://").ok_or_else(|| {
        anyhow!("{flag} wants an http:// URL (TLS is not supported), got '{url}'")
    })?;
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => {
            let port: u16 = port
                .parse()
                .map_err(|_| anyhow!("{flag} port '{port}' is not a number"))?;
            (host, port)
        }
        None => (authority, 80),
    };
    let host = host.trim_start_matches('[').trim_end_matches(']');
    if host.is_empty() {
        bail!("{flag} host is empty");
    }
    Ok(Target {
        host: host.to_string(),
        port,
        path: path.to_string(),
    })
}

/// POSTs `body` with the given content type. Succeeds on a 2xx reply.
pub async fn post(target: &Target, content_type: &str, body: &str) -> Result<()> {
    post_bytes(target, &[("Content-Type", content_type)], body.as_bytes()).await
}

/// POSTs `body` with the given extra headers (name, value). The caller sets
/// `Content-Type` and any other header a server needs. Succeeds on a 2xx reply.
pub async fn post_bytes(target: &Target, headers: &[(&str, &str)], body: &[u8]) -> Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    let mut head = format!(
        "POST {path} HTTP/1.1\r\nHost: {host}:{port}\r\n",
        path = target.path,
        host = target.host,
        port = target.port,
    );
    for (name, value) in headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str(&format!(
        "Content-Length: {len}\r\nConnection: close\r\n\r\n",
        len = body.len()
    ));

    let exchange = async {
        let mut stream = TcpStream::connect((target.host.as_str(), target.port)).await?;
        stream.write_all(head.as_bytes()).await?;
        stream.write_all(body).await?;
        let mut reply = Vec::new();
        stream.read_to_end(&mut reply).await?;
        Ok::<Vec<u8>, std::io::Error>(reply)
    };
    let reply = tokio::time::timeout(IO_TIMEOUT, exchange)
        .await
        .map_err(|_| anyhow!("no complete reply within {IO_TIMEOUT:?}"))??;

    let status_line = reply.split(|&b| b == b'\n').next().unwrap_or(&[]);
    let status_line = String::from_utf8_lossy(status_line);
    let code: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .ok_or_else(|| anyhow!("the server sent no HTTP status line"))?;
    if (200..300).contains(&code) {
        Ok(())
    } else {
        bail!("the server answered HTTP {code}")
    }
}

/// A one-shot HTTP server for tests: it reads one request, answers with
/// `status`, and returns the request text.
#[cfg(test)]
pub mod test_server {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};

    use super::Target;

    /// Reads one HTTP request (headers, then Content-Length bytes).
    async fn read_request(stream: &mut TcpStream) -> String {
        let mut buf = Vec::new();
        let mut chunk = [0u8; 1024];
        loop {
            let n = stream.read(&mut chunk).await.unwrap();
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..n]);
            if let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&buf[..end]).to_string();
                let length: usize = headers
                    .lines()
                    .find_map(|l| l.strip_prefix("Content-Length: "))
                    .and_then(|v| v.trim().parse().ok())
                    .unwrap_or(0);
                if buf.len() >= end + 4 + length {
                    break;
                }
            }
        }
        String::from_utf8_lossy(&buf).to_string()
    }

    /// Serves one request with `status`. Returns the target and a handle that
    /// yields the request text.
    pub async fn serve_once(
        status: &'static str,
        path: &str,
    ) -> (Target, tokio::task::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_request(&mut stream).await;
            let reply = format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\n\r\n");
            stream.write_all(reply.as_bytes()).await.unwrap();
            request
        });
        let target = Target {
            host: "127.0.0.1".into(),
            port,
            path: path.into(),
        };
        (target, handle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_are_parsed_with_defaults() {
        let t = parse_http_url("http://127.0.0.1:8086/write?db=tpt", "--x").unwrap();
        assert_eq!(t.host, "127.0.0.1");
        assert_eq!(t.port, 8086);
        assert_eq!(t.path, "/write?db=tpt");

        let t = parse_http_url("http://collector", "--x").unwrap();
        assert_eq!((t.port, t.path.as_str()), (80, "/"));

        let t = parse_http_url("http://[::1]:4318/v1/metrics", "--x").unwrap();
        assert_eq!(t.host, "::1");

        assert!(parse_http_url("https://example.com", "--x").is_err());
        assert!(parse_http_url("http://:8086/", "--x").is_err());
        assert!(parse_http_url("http://host:notaport/", "--x").is_err());
    }

    #[tokio::test]
    async fn post_sends_the_body_and_accepts_a_2xx_reply() {
        let (target, server) = test_server::serve_once("204 No Content", "/write").await;
        post(&target, "text/plain", "modbus value=1i")
            .await
            .unwrap();
        let request = server.await.unwrap();
        assert!(request.starts_with("POST /write HTTP/1.1\r\n"), "{request}");
        assert!(
            request.contains("Content-Type: text/plain\r\n"),
            "{request}"
        );
        assert!(request.ends_with("modbus value=1i"), "{request}");
    }

    #[tokio::test]
    async fn post_reports_a_refused_batch() {
        let (target, server) = test_server::serve_once("500 Internal Server Error", "/").await;
        let err = post(&target, "text/plain", "x").await.unwrap_err();
        assert!(err.to_string().contains("500"), "{err}");
        server.await.unwrap();
    }
}
