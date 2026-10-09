use crate::media::MediaState;
use std::{io::SeekFrom, path::PathBuf, sync::Arc, time::Duration};
use tauri::Manager;
use tokio::{
    io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
pub trait MediaResolver: Send + Sync {
    fn media(&self) -> Option<PathBuf>;
    fn thumbnail(&self, index: usize) -> Option<PathBuf>;
}
struct StateResolver {
    app: tauri::AppHandle,
}
impl MediaResolver for StateResolver {
    fn media(&self) -> Option<PathBuf> {
        self.app
            .state::<MediaState>()
            .current
            .lock()
            .ok()
            .and_then(|guard| guard.as_ref().map(|media| media.preview.clone()))
    }
    fn thumbnail(&self, index: usize) -> Option<PathBuf> {
        self.app
            .state::<MediaState>()
            .current
            .lock()
            .ok()
            .and_then(|guard| {
                guard
                    .as_ref()
                    .and_then(|media| media.thumbnails.lock().ok()?.get(index).cloned().flatten())
            })
    }
}
pub struct PreviewServer {
    media_url: String,
    runtime: std::sync::Mutex<Option<tokio::runtime::Runtime>>,
}
impl PreviewServer {
    pub fn start(app: tauri::AppHandle) -> std::io::Result<Self> {
        let std_listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        std_listener.set_nonblocking(true)?;
        let port = std_listener.local_addr()?.port();
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_io()
            .enable_time()
            .build()?;
        let resolver: Arc<dyn MediaResolver> = Arc::new(StateResolver { app });
        let listener = {
            let _guard = runtime.enter();
            TcpListener::from_std(std_listener)?
        };
        runtime.spawn(serve(listener, resolver));
        Ok(Self {
            media_url: format!("http://127.0.0.1:{port}/media"),
            runtime: std::sync::Mutex::new(Some(runtime)),
        })
    }
    pub fn media_url(&self) -> &str {
        &self.media_url
    }
    pub fn stop(&self) {
        if let Ok(mut guard) = self.runtime.lock() {
            if let Some(runtime) = guard.take() {
                runtime.shutdown_background();
            }
        }
    }
}
async fn serve(listener: TcpListener, resolver: Arc<dyn MediaResolver>) {
    loop {
        let Ok((stream, _)) = listener.accept().await else {
            break;
        };
        let resolver = Arc::clone(&resolver);
        tokio::spawn(async move {
            let _ = handle_connection(stream, resolver.as_ref()).await;
        });
    }
}
const MAX_HEADER_BYTES: usize = 8 * 1024;
const IDLE_TIMEOUT: Duration = Duration::from_secs(30);
struct RequestHead {
    method: String,
    target: String,
    range: Option<String>,
    keep_alive: bool,
}
enum ReadOutcome {
    Head(RequestHead),
    /// The client closed or went idle before starting another request.
    Closed,
    Malformed,
}
/// Reads one request head; bytes past its end stay in `buf` for the next request.
async fn read_request_head(
    stream: &mut TcpStream,
    buf: &mut Vec<u8>,
) -> std::io::Result<ReadOutcome> {
    let mut chunk = [0u8; 1024];
    let end = loop {
        if let Some(pos) = find_header_end(buf) {
            break pos;
        }
        if buf.len() > MAX_HEADER_BYTES {
            return Ok(ReadOutcome::Malformed);
        }
        let n = match tokio::time::timeout(IDLE_TIMEOUT, stream.read(&mut chunk)).await {
            Ok(result) => result?,
            Err(_) => return Ok(ReadOutcome::Closed),
        };
        if n == 0 {
            return Ok(if buf.is_empty() {
                ReadOutcome::Closed
            } else {
                ReadOutcome::Malformed
            });
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    let text = String::from_utf8_lossy(&buf[..end]).into_owned();
    buf.drain(..end + 4);
    let mut lines = text.lines();
    let Some(request_line) = lines.next() else {
        return Ok(ReadOutcome::Malformed);
    };
    let mut parts = request_line.split_whitespace();
    let (Some(method), Some(target), version) = (parts.next(), parts.next(), parts.next()) else {
        return Ok(ReadOutcome::Malformed);
    };
    // HTTP/1.1 keeps connections open by default; HTTP/1.0 closes them.
    let mut keep_alive = version != Some("HTTP/1.0");
    let mut range = None;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        if name.eq_ignore_ascii_case("range") {
            range = Some(value.to_string());
        } else if name.eq_ignore_ascii_case("connection") {
            for token in value.split(',').map(str::trim) {
                if token.eq_ignore_ascii_case("close") {
                    keep_alive = false;
                } else if token.eq_ignore_ascii_case("keep-alive") {
                    keep_alive = true;
                }
            }
        }
    }
    Ok(ReadOutcome::Head(RequestHead {
        method: method.to_ascii_uppercase(),
        target: target.to_owned(),
        range,
        keep_alive,
    }))
}
fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}
fn connection_header(keep_alive: bool) -> &'static str {
    if keep_alive {
        "keep-alive"
    } else {
        "close"
    }
}
async fn handle_connection(
    mut stream: TcpStream,
    resolver: &dyn MediaResolver,
) -> std::io::Result<()> {
    let mut buf = Vec::new();
    loop {
        let head = match read_request_head(&mut stream, &mut buf).await? {
            ReadOutcome::Head(head) => head,
            ReadOutcome::Closed => return Ok(()),
            ReadOutcome::Malformed => {
                return write_error(&mut stream, 400, "Bad Request", false).await
            }
        };
        if head.method != "GET" && head.method != "HEAD" {
            return write_error(&mut stream, 405, "Method Not Allowed", false).await;
        }
        respond(&mut stream, resolver, &head).await?;
        if !head.keep_alive {
            return Ok(());
        }
    }
}
async fn respond(
    stream: &mut TcpStream,
    resolver: &dyn MediaResolver,
    head: &RequestHead,
) -> std::io::Result<()> {
    // Each load gets its own `?load=<id>` URL; the query only busts caches.
    let target = head.target.split('?').next().unwrap_or_default();
    let path = if target == "/media" {
        resolver.media()
    } else if let Some(index) = target
        .strip_prefix("/thumb/")
        .and_then(|v| v.parse::<usize>().ok())
    {
        resolver.thumbnail(index)
    } else {
        None
    };
    let Some(path) = path else {
        return write_error(stream, 404, "Not Found", head.keep_alive).await;
    };
    serve_file(
        stream,
        &path,
        head.range.as_deref(),
        head.method == "GET",
        head.keep_alive,
    )
    .await
}
enum RangeSpec {
    Full,
    Slice(u64, u64),
    Unsatisfiable,
}
fn parse_range(header: Option<&str>, len: u64) -> RangeSpec {
    let Some(spec) = header.and_then(|v| v.trim().strip_prefix("bytes=")) else {
        return RangeSpec::Full;
    };
    let first = spec.split(',').next().unwrap_or("").trim();
    if let Some(suffix) = first.strip_prefix('-') {
        let Ok(n) = suffix.parse::<u64>() else {
            return RangeSpec::Full;
        };
        if n == 0 || len == 0 {
            return RangeSpec::Unsatisfiable;
        }
        return RangeSpec::Slice(len.saturating_sub(n), len - 1);
    }
    let Some((start_text, end_text)) = first.split_once('-') else {
        return RangeSpec::Full;
    };
    let Ok(start) = start_text.parse::<u64>() else {
        return RangeSpec::Full;
    };
    if start >= len {
        return RangeSpec::Unsatisfiable;
    }
    let end = end_text.parse::<u64>().unwrap_or(u64::MAX).min(len - 1);
    if end < start {
        return RangeSpec::Unsatisfiable;
    }
    RangeSpec::Slice(start, end)
}
fn content_type(path: &std::path::Path) -> &'static str {
    match path
        .extension()
        .and_then(|v| v.to_str())
        .map(|v| v.to_ascii_lowercase())
        .as_deref()
    {
        Some("mp4") | Some("m4v") => "video/mp4",
        Some("mov") => "video/quicktime",
        Some("mkv") => "video/x-matroska",
        Some("webm") => "video/webm",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        _ => "application/octet-stream",
    }
}
async fn serve_file(
    stream: &mut TcpStream,
    path: &std::path::Path,
    range_header: Option<&str>,
    include_body: bool,
    keep_alive: bool,
) -> std::io::Result<()> {
    let connection = connection_header(keep_alive);
    let mut file = match tokio::fs::File::open(path).await {
        Ok(v) => v,
        Err(_) => return write_error(stream, 404, "Not Found", keep_alive).await,
    };
    let len = file.metadata().await?.len();
    let (status, start, end) = match parse_range(range_header, len) {
        RangeSpec::Full => (200, 0, len.saturating_sub(1)),
        RangeSpec::Slice(start, end) => (206, start, end),
        RangeSpec::Unsatisfiable => {
            let head = format!(
                "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */{len}\r\nConnection: {connection}\r\nContent-Length: 0\r\n\r\n"
            );
            stream.write_all(head.as_bytes()).await?;
            return stream.flush().await;
        }
    };
    let nbytes = if len == 0 { 0 } else { end + 1 - start };
    let reason = if status == 206 {
        "Partial Content"
    } else {
        "OK"
    };
    let mut head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {}\r\nAccept-Ranges: bytes\r\nContent-Length: {nbytes}\r\nConnection: {connection}\r\n",
        content_type(path)
    );
    if status == 206 {
        head.push_str(&format!("Content-Range: bytes {start}-{end}/{len}\r\n"));
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes()).await?;
    if !include_body || nbytes == 0 {
        return stream.flush().await;
    }
    file.seek(SeekFrom::Start(start)).await?;
    let mut remaining = nbytes;
    let mut chunk = vec![0u8; 64 * 1024];
    while remaining > 0 {
        let want = (remaining as usize).min(chunk.len());
        let n = file.read(&mut chunk[..want]).await?;
        if n == 0 {
            // A short body breaks keep-alive framing, so end the connection.
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }
        stream.write_all(&chunk[..n]).await?;
        remaining -= n as u64;
    }
    stream.flush().await
}
async fn write_error(
    stream: &mut TcpStream,
    status: u16,
    reason: &str,
    keep_alive: bool,
) -> std::io::Result<()> {
    let body = format!("{status} {reason}\n");
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: {}\r\n\r\n",
        body.len(),
        connection_header(keep_alive)
    );
    stream.write_all(head.as_bytes()).await?;
    stream.write_all(body.as_bytes()).await?;
    stream.flush().await
}
#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpStream as TokioStream;
    struct Fixed {
        media: Option<PathBuf>,
        thumbnails: Vec<PathBuf>,
    }
    impl MediaResolver for Fixed {
        fn media(&self) -> Option<PathBuf> {
            self.media.clone()
        }
        fn thumbnail(&self, index: usize) -> Option<PathBuf> {
            self.thumbnails.get(index).cloned()
        }
    }
    async fn start_server(
        media: Option<PathBuf>,
        thumbnails: Vec<PathBuf>,
    ) -> std::net::SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let resolver: Arc<dyn MediaResolver> = Arc::new(Fixed { media, thumbnails });
        tokio::spawn(serve(listener, resolver));
        addr
    }
    /// Reads one response, using `Content-Length` to find its end so that a
    /// kept-open connection does not block.
    async fn read_response(
        stream: &mut TokioStream,
        buf: &mut Vec<u8>,
        head_request: bool,
    ) -> (u16, String, Vec<u8>) {
        let mut chunk = [0u8; 1024];
        let end = loop {
            if let Some(end) = find_header_end(buf) {
                break end;
            }
            let n = stream.read(&mut chunk).await.unwrap();
            assert!(n > 0, "connection closed before the response head");
            buf.extend_from_slice(&chunk[..n]);
        };
        let head = String::from_utf8_lossy(&buf[..end]).into_owned();
        let status = head
            .split_whitespace()
            .nth(1)
            .and_then(|v| v.parse::<u16>().ok())
            .unwrap_or(0);
        let len = if head_request {
            0
        } else {
            head.lines()
                .find_map(|line| line.strip_prefix("Content-Length:"))
                .and_then(|v| v.trim().parse::<usize>().ok())
                .unwrap_or(0)
        };
        while buf.len() < end + 4 + len {
            let n = stream.read(&mut chunk).await.unwrap();
            assert!(n > 0, "connection closed before the response body");
            buf.extend_from_slice(&chunk[..n]);
        }
        let body = buf[end + 4..end + 4 + len].to_vec();
        buf.drain(..end + 4 + len);
        (status, head, body)
    }
    async fn raw_request(addr: std::net::SocketAddr, request: &str) -> (u16, String, Vec<u8>) {
        let mut stream = TokioStream::connect(addr).await.unwrap();
        stream.write_all(request.as_bytes()).await.unwrap();
        read_response(&mut stream, &mut Vec::new(), request.starts_with("HEAD")).await
    }
    fn write_media(dir: &std::path::Path) -> PathBuf {
        let path = dir.join("media.mp4");
        std::fs::write(&path, b"0123456789abcdef").unwrap();
        path
    }
    #[tokio::test]
    async fn full_request_returns_entire_body_with_accept_ranges() {
        let dir = tempfile::tempdir().unwrap();
        let addr = start_server(Some(write_media(dir.path())), vec![]).await;
        let (status, head, body) =
            raw_request(addr, "GET /media HTTP/1.1\r\nHost: localhost\r\n\r\n").await;
        assert_eq!(status, 200);
        assert!(head.contains("Accept-Ranges: bytes"));
        assert!(head.contains("Content-Length: 16"));
        assert!(head.contains("Content-Type: video/mp4"));
        assert_eq!(body, b"0123456789abcdef");
    }
    #[tokio::test]
    async fn open_ended_range_returns_tail_slice() {
        let dir = tempfile::tempdir().unwrap();
        let addr = start_server(Some(write_media(dir.path())), vec![]).await;
        let (status, head, body) = raw_request(
            addr,
            "GET /media HTTP/1.1\r\nHost: localhost\r\nRange: bytes=10-\r\n\r\n",
        )
        .await;
        assert_eq!(status, 206);
        assert!(head.contains("Content-Range: bytes 10-15/16"));
        assert_eq!(body, b"abcdef");
    }
    #[tokio::test]
    async fn probe_and_bounded_ranges_return_exact_slices() {
        let dir = tempfile::tempdir().unwrap();
        let addr = start_server(Some(write_media(dir.path())), vec![]).await;
        let (status, head, body) = raw_request(
            addr,
            "GET /media HTTP/1.1\r\nHost: localhost\r\nRange: bytes=0-1\r\n\r\n",
        )
        .await;
        assert_eq!(status, 206);
        assert!(head.contains("Content-Range: bytes 0-1/16"));
        assert_eq!(body, b"01");
        let (_, head, body) = raw_request(
            addr,
            "GET /media HTTP/1.1\r\nHost: localhost\r\nRange: bytes=4-99\r\n\r\n",
        )
        .await;
        assert!(head.contains("Content-Range: bytes 4-15/16"));
        assert_eq!(body, b"456789abcdef");
    }
    #[tokio::test]
    async fn unsatisfiable_range_returns_416() {
        let dir = tempfile::tempdir().unwrap();
        let addr = start_server(Some(write_media(dir.path())), vec![]).await;
        let (status, head, body) = raw_request(
            addr,
            "GET /media HTTP/1.1\r\nHost: localhost\r\nRange: bytes=16-\r\n\r\n",
        )
        .await;
        assert_eq!(status, 416);
        assert!(head.contains("Content-Range: bytes */16"));
        assert!(body.is_empty());
    }
    #[tokio::test]
    async fn unknown_paths_and_missing_media_are_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let media = write_media(dir.path());
        let addr = start_server(Some(media.clone()), vec![]).await;
        let (status, _, _) = raw_request(addr, "GET /etc/passwd HTTP/1.1\r\n\r\n").await;
        assert_eq!(status, 404);
        let (status, _, _) = raw_request(addr, "GET /thumb/0 HTTP/1.1\r\n\r\n").await;
        assert_eq!(status, 404);
        let addr = start_server(None, vec![]).await;
        let (status, _, _) = raw_request(addr, "GET /media HTTP/1.1\r\n\r\n").await;
        assert_eq!(status, 404);
    }
    #[tokio::test]
    async fn thumbnails_are_served_by_index_only() {
        let dir = tempfile::tempdir().unwrap();
        let thumb = dir.path().join("frame-00.jpg");
        std::fs::write(&thumb, b"jpegbytes").unwrap();
        let addr = start_server(Some(write_media(dir.path())), vec![thumb]).await;
        let (status, head, body) = raw_request(addr, "GET /thumb/0 HTTP/1.1\r\n\r\n").await;
        assert_eq!(status, 200);
        assert!(head.contains("Content-Type: image/jpeg"));
        assert_eq!(body, b"jpegbytes");
        let (status, _, _) = raw_request(addr, "GET /thumb/1 HTTP/1.1\r\n\r\n").await;
        assert_eq!(status, 404);
    }
    #[tokio::test]
    async fn routing_ignores_the_load_query() {
        let dir = tempfile::tempdir().unwrap();
        let thumb = dir.path().join("frame-00.jpg");
        std::fs::write(&thumb, b"jpegbytes").unwrap();
        let addr = start_server(Some(write_media(dir.path())), vec![thumb]).await;
        let (status, head, body) = raw_request(
            addr,
            "GET /media?load=7 HTTP/1.1\r\nRange: bytes=2-4\r\n\r\n",
        )
        .await;
        assert_eq!(status, 206);
        assert!(head.contains("Content-Range: bytes 2-4/16"));
        assert_eq!(body, b"234");
        let (status, _, body) = raw_request(addr, "GET /thumb/0?load=7 HTTP/1.1\r\n\r\n").await;
        assert_eq!(status, 200);
        assert_eq!(body, b"jpegbytes");
        let (status, _, body) = raw_request(addr, "GET /other?load=7 HTTP/1.1\r\n\r\n").await;
        assert_eq!(status, 404);
        assert!(body.is_empty() || !body.starts_with(b"0123"));
    }
    #[tokio::test]
    async fn head_requests_send_headers_without_body() {
        let dir = tempfile::tempdir().unwrap();
        let addr = start_server(Some(write_media(dir.path())), vec![]).await;
        let (status, head, body) = raw_request(addr, "HEAD /media HTTP/1.1\r\n\r\n").await;
        assert_eq!(status, 200);
        assert!(head.contains("Content-Length: 16"));
        assert!(body.is_empty());
    }
    #[tokio::test]
    async fn sequential_range_requests_share_one_connection() {
        let dir = tempfile::tempdir().unwrap();
        let addr = start_server(Some(write_media(dir.path())), vec![]).await;
        let mut stream = TokioStream::connect(addr).await.unwrap();
        let mut buf = Vec::new();
        stream
            .write_all(b"GET /media HTTP/1.1\r\nHost: localhost\r\nRange: bytes=0-1\r\n\r\n")
            .await
            .unwrap();
        let (status, head, body) = read_response(&mut stream, &mut buf, false).await;
        assert_eq!(status, 206);
        assert!(head.contains("Connection: keep-alive"));
        assert_eq!(body, b"01");
        stream
            .write_all(b"GET /media HTTP/1.1\r\nHost: localhost\r\nRange: bytes=10-\r\n\r\n")
            .await
            .unwrap();
        let (status, head, body) = read_response(&mut stream, &mut buf, false).await;
        assert_eq!(status, 206);
        assert!(head.contains("Content-Range: bytes 10-15/16"));
        assert_eq!(body, b"abcdef");
    }
    #[tokio::test]
    async fn pipelined_requests_are_answered_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let addr = start_server(Some(write_media(dir.path())), vec![]).await;
        let mut stream = TokioStream::connect(addr).await.unwrap();
        let mut buf = Vec::new();
        stream
            .write_all(
                b"GET /media HTTP/1.1\r\nRange: bytes=0-1\r\n\r\nGET /media HTTP/1.1\r\nRange: bytes=2-3\r\n\r\n",
            )
            .await
            .unwrap();
        let (_, _, body) = read_response(&mut stream, &mut buf, false).await;
        assert_eq!(body, b"01");
        let (status, _, body) = read_response(&mut stream, &mut buf, false).await;
        assert_eq!(status, 206);
        assert_eq!(body, b"23");
    }
    #[tokio::test]
    async fn connection_close_ends_the_connection_after_the_response() {
        let dir = tempfile::tempdir().unwrap();
        let addr = start_server(Some(write_media(dir.path())), vec![]).await;
        let mut stream = TokioStream::connect(addr).await.unwrap();
        stream
            .write_all(b"GET /media HTTP/1.1\r\nConnection: Close\r\nRange: bytes=0-1\r\n\r\n")
            .await
            .unwrap();
        let mut buf = Vec::new();
        tokio::time::timeout(Duration::from_secs(5), stream.read_to_end(&mut buf))
            .await
            .expect("server closes the connection")
            .unwrap();
        let end = find_header_end(&buf).unwrap();
        let head = String::from_utf8_lossy(&buf[..end]);
        assert!(head.starts_with("HTTP/1.1 206"));
        assert!(head.contains("Connection: close"));
        assert_eq!(&buf[end + 4..], b"01");
    }
    #[test]
    fn range_parser_covers_gstreamer_patterns() {
        assert!(matches!(parse_range(None, 10), RangeSpec::Full));
        assert!(matches!(
            parse_range(Some("bytes=0-1"), 10),
            RangeSpec::Slice(0, 1)
        ));
        assert!(matches!(
            parse_range(Some("bytes=5-"), 10),
            RangeSpec::Slice(5, 9)
        ));
        assert!(matches!(
            parse_range(Some("bytes=-3"), 10),
            RangeSpec::Slice(7, 9)
        ));
        assert!(matches!(
            parse_range(Some("bytes=10-"), 10),
            RangeSpec::Unsatisfiable
        ));
        assert!(matches!(
            parse_range(Some("bytes=2-1"), 10),
            RangeSpec::Unsatisfiable
        ));
        assert!(matches!(
            parse_range(Some("items=0-1"), 10),
            RangeSpec::Full
        ));
        assert!(matches!(
            parse_range(Some("bytes=abc-"), 10),
            RangeSpec::Full
        ));
        assert!(matches!(
            parse_range(Some("bytes=0-1,3-4"), 10),
            RangeSpec::Slice(0, 1)
        ));
        assert!(matches!(
            parse_range(Some("bytes=0-"), 0),
            RangeSpec::Unsatisfiable
        ));
    }
    #[tokio::test]
    async fn concurrent_requests_do_not_interleave() {
        let dir = tempfile::tempdir().unwrap();
        let addr = start_server(Some(write_media(dir.path())), vec![]).await;
        let mut handles = vec![];
        for _ in 0..8 {
            handles.push(tokio::spawn(async move {
                let (status, _, body) =
                    raw_request(addr, "GET /media HTTP/1.1\r\nHost: localhost\r\n\r\n").await;
                assert_eq!(status, 200);
                assert_eq!(body, b"0123456789abcdef");
            }));
        }
        for h in handles {
            h.await.unwrap();
        }
    }
}
