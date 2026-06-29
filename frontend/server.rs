use std::{
    env,
    error::Error,
    fs,
    io::{Read, Write},
    net::{SocketAddr, TcpStream, ToSocketAddrs},
    path::{Component, Path, PathBuf},
    process,
    thread,
    time::Duration,
};

const DEFAULT_BIND: &str = "0.0.0.0:8080";
const DEFAULT_HEALTHCHECK_ADDR: &str = "127.0.0.1:8080";
const DEFAULT_STATIC_ROOT: &str = "/usr/share/memoryops/html";
const DEFAULT_API_UPSTREAM: &str = "api:8080";
const MAX_HEADER_BYTES: usize = 64 * 1024;
const IO_TIMEOUT_SECS: u64 = 30;

fn main() {
    let args: Vec<String> = env::args().collect();
    let result = if args.get(1).map(String::as_str) == Some("--healthcheck") {
        healthcheck()
    } else {
        serve()
    };

    if let Err(error) = result {
        eprintln!("memoryops frontend server failed: {error}");
        process::exit(1);
    }
}

fn serve() -> Result<(), Box<dyn Error>> {
    let bind = env::var("MEMORYOPS_FRONTEND_BIND").unwrap_or_else(|_| DEFAULT_BIND.to_owned());
    let listener = std::net::TcpListener::bind(&bind)?;
    println!("MemoryOps frontend listening on {bind}");

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                thread::spawn(move || {
                    if let Err(error) = handle_client(stream) {
                        eprintln!("frontend request failed: {error}");
                    }
                });
            }
            Err(error) => eprintln!("frontend accept failed: {error}"),
        }
    }

    Ok(())
}

fn healthcheck() -> Result<(), Box<dyn Error>> {
    let address = env::var("MEMORYOPS_FRONTEND_HEALTHCHECK_ADDR")
        .unwrap_or_else(|_| DEFAULT_HEALTHCHECK_ADDR.to_owned());
    let mut stream = connect_with_timeout(&address, Duration::from_secs(2))?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    stream.write_all(b"GET /health HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")?;

    let mut response = [0_u8; 128];
    let bytes_read = stream.read(&mut response)?;
    let response = std::str::from_utf8(&response[..bytes_read])?;
    if response.starts_with("HTTP/1.1 200") || response.starts_with("HTTP/1.0 200") {
        Ok(())
    } else {
        Err(format!("unexpected health response: {response:?}").into())
    }
}

fn handle_client(mut stream: TcpStream) -> Result<(), Box<dyn Error>> {
    let peer_addr = stream.peer_addr().ok();
    stream.set_read_timeout(Some(Duration::from_secs(IO_TIMEOUT_SECS)))?;
    stream.set_write_timeout(Some(Duration::from_secs(IO_TIMEOUT_SECS)))?;

    let request = read_request(&mut stream)?;
    if request.target == "/health" {
        return write_response(
            &mut stream,
            200,
            "OK",
            "text/plain; charset=utf-8",
            b"ok\n",
            CachePolicy::NoStore,
            request.method == "HEAD",
        );
    }

    if request.target == "/config.json" || request.target.starts_with("/config.json?") {
        let body = runtime_config_json();
        return write_response(
            &mut stream,
            200,
            "OK",
            "application/json; charset=utf-8",
            body.as_bytes(),
            CachePolicy::NoStore,
            request.method == "HEAD",
        );
    }

    if request.target == "/api" || request.target.starts_with("/api/") || request.target.starts_with("/api?") {
        return proxy_api(stream, request, peer_addr);
    }

    if request.method != "GET" && request.method != "HEAD" {
        return write_response(
            &mut stream,
            405,
            "Method Not Allowed",
            "text/plain; charset=utf-8",
            b"method not allowed\n",
            CachePolicy::NoStore,
            false,
        );
    }

    serve_static(stream, request)
}

#[derive(Debug)]
struct Request {
    method: String,
    target: String,
    version: String,
    headers: Vec<(String, String)>,
    content_length: usize,
    body_prefix: Vec<u8>,
}

fn read_request(stream: &mut TcpStream) -> Result<Request, Box<dyn Error>> {
    let mut buffer = Vec::with_capacity(4096);
    let mut temp = [0_u8; 2048];
    let header_end = loop {
        let read = stream.read(&mut temp)?;
        if read == 0 {
            return Err("connection closed before request headers".into());
        }
        buffer.extend_from_slice(&temp[..read]);
        if buffer.len() > MAX_HEADER_BYTES {
            return Err("request headers too large".into());
        }
        if let Some(index) = find_header_end(&buffer) {
            break index;
        }
    };

    let header_bytes = &buffer[..header_end];
    let body_prefix = buffer[header_end + 4..].to_vec();
    let header_text = std::str::from_utf8(header_bytes)?;
    let mut lines = header_text.split("\r\n");
    let request_line = lines.next().ok_or("missing request line")?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().ok_or("missing method")?.to_owned();
    let target = parts.next().ok_or("missing target")?.to_owned();
    let version = parts.next().ok_or("missing version")?.to_owned();

    let mut headers = Vec::new();
    let mut content_length = 0_usize;
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let name = name.trim().to_owned();
        let value = value.trim().to_owned();
        if name.eq_ignore_ascii_case("content-length") {
            content_length = value.parse::<usize>().unwrap_or(0);
        }
        headers.push((name, value));
    }

    Ok(Request {
        method,
        target,
        version,
        headers,
        content_length,
        body_prefix,
    })
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

fn proxy_api(
    mut client: TcpStream,
    request: Request,
    peer_addr: Option<SocketAddr>,
) -> Result<(), Box<dyn Error>> {
    let upstream = env::var("MEMORYOPS_API_UPSTREAM").unwrap_or_else(|_| DEFAULT_API_UPSTREAM.to_owned());
    let mut upstream_stream = connect_with_timeout(&upstream, Duration::from_secs(5))?;
    upstream_stream.set_read_timeout(Some(Duration::from_secs(IO_TIMEOUT_SECS)))?;
    upstream_stream.set_write_timeout(Some(Duration::from_secs(IO_TIMEOUT_SECS)))?;

    let upstream_target = strip_api_prefix(&request.target);
    write!(
        upstream_stream,
        "{} {} {}\r\nHost: {}\r\nConnection: close\r\n",
        request.method, upstream_target, request.version, upstream
    )?;

    for (name, value) in &request.headers {
        if is_hop_by_hop_header(name) || name.eq_ignore_ascii_case("host") {
            continue;
        }
        if name.eq_ignore_ascii_case("x-forwarded-for") {
            continue;
        }
        write!(upstream_stream, "{name}: {value}\r\n")?;
    }

    if let Some(peer) = peer_addr {
        write!(upstream_stream, "X-Forwarded-For: {}\r\n", peer.ip())?;
    }
    write!(upstream_stream, "X-Forwarded-Proto: http\r\n")?;
    write!(upstream_stream, "\r\n")?;

    if request.content_length > 0 {
        upstream_stream.write_all(&request.body_prefix)?;
        let mut remaining = request.content_length.saturating_sub(request.body_prefix.len());
        let mut buffer = [0_u8; 16 * 1024];
        while remaining > 0 {
            let to_read = remaining.min(buffer.len());
            let read = client.read(&mut buffer[..to_read])?;
            if read == 0 {
                break;
            }
            upstream_stream.write_all(&buffer[..read])?;
            remaining -= read;
        }
    }

    std::io::copy(&mut upstream_stream, &mut client)?;
    Ok(())
}

fn connect_with_timeout(address: &str, timeout: Duration) -> Result<TcpStream, Box<dyn Error>> {
    let mut last_error = None;
    for socket_addr in address.to_socket_addrs()? {
        match TcpStream::connect_timeout(&socket_addr, timeout) {
            Ok(stream) => return Ok(stream),
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error
        .map(|error| format!("failed to connect to {address}: {error}"))
        .unwrap_or_else(|| format!("failed to resolve {address}"))
        .into())
}

fn strip_api_prefix(target: &str) -> String {
    if let Some(query) = target.strip_prefix("/api?") {
        format!("/?{query}")
    } else if target == "/api" {
        "/".to_owned()
    } else {
        target.strip_prefix("/api").unwrap_or(target).to_owned()
    }
}

fn is_hop_by_hop_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
    )
}

fn serve_static(mut stream: TcpStream, request: Request) -> Result<(), Box<dyn Error>> {
    let root = env::var("MEMORYOPS_STATIC_ROOT").unwrap_or_else(|_| DEFAULT_STATIC_ROOT.to_owned());
    let root = PathBuf::from(root);
    let request_path = request.target.split('?').next().unwrap_or("/");
    let decoded_path = match percent_decode_path(request_path) {
        Some(path) => path,
        None => {
            return write_response(
                &mut stream,
                400,
                "Bad Request",
                "text/plain; charset=utf-8",
                b"bad request\n",
                CachePolicy::NoStore,
                false,
            )
        }
    };

    let is_asset = decoded_path.starts_with("/assets/");
    let candidate = match safe_static_path(&root, &decoded_path) {
        Some(path) => path,
        None => {
            return write_response(
                &mut stream,
                400,
                "Bad Request",
                "text/plain; charset=utf-8",
                b"bad request\n",
                CachePolicy::NoStore,
                false,
            )
        }
    };
    let file_path = if candidate.is_file() {
        candidate
    } else if is_asset {
        return write_response(
            &mut stream,
            404,
            "Not Found",
            "text/plain; charset=utf-8",
            b"not found\n",
            CachePolicy::NoStore,
            false,
        );
    } else {
        root.join("index.html")
    };

    let body = fs::read(&file_path)?;
    let content_type = content_type_for(&file_path);
    let cache_policy = if decoded_path.starts_with("/assets/") {
        CachePolicy::Immutable
    } else {
        CachePolicy::NoStore
    };

    write_response(
        &mut stream,
        200,
        "OK",
        content_type,
        &body,
        cache_policy,
        request.method == "HEAD",
    )
}

fn safe_static_path(root: &Path, decoded_path: &str) -> Option<PathBuf> {
    if !decoded_path.starts_with('/') {
        return None;
    }
    let trimmed = decoded_path.trim_start_matches('/');
    if trimmed.is_empty() {
        return Some(root.join("index.html"));
    }

    let mut path = PathBuf::from(root);
    for component in Path::new(trimmed).components() {
        match component {
            Component::Normal(value) => path.push(value),
            Component::CurDir => {}
            _ => return None,
        }
    }
    Some(path)
}

fn percent_decode_path(path: &str) -> Option<String> {
    let bytes = path.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' => {
                if index + 2 >= bytes.len() {
                    return None;
                }
                let hi = hex_value(bytes[index + 1])?;
                let lo = hex_value(bytes[index + 2])?;
                decoded.push((hi << 4) | lo);
                index += 3;
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }

    let decoded = String::from_utf8(decoded).ok()?;
    if decoded.contains('\0') || decoded.contains('\\') {
        return None;
    }
    Some(decoded)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn runtime_config_json() -> String {
    let workspace_id = env::var("MEMORYOPS_WORKSPACE_ID").unwrap_or_default();
    let workspace_id = if workspace_id.is_empty() {
        String::new()
    } else if is_uuid(&workspace_id) {
        workspace_id
    } else {
        eprintln!("WARNING: MEMORYOPS_WORKSPACE_ID is not a valid UUID; ignoring");
        String::new()
    };

    format!("{{\"workspaceId\":\"{workspace_id}\"}}\n")
}

fn is_uuid(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 36 {
        return false;
    }
    for (index, byte) in bytes.iter().enumerate() {
        match index {
            8 | 13 | 18 | 23 => {
                if *byte != b'-' {
                    return false;
                }
            }
            _ => {
                if !byte.is_ascii_hexdigit() {
                    return false;
                }
            }
        }
    }
    true
}

#[derive(Clone, Copy)]
enum CachePolicy {
    Immutable,
    NoStore,
}

fn write_response(
    stream: &mut TcpStream,
    status_code: u16,
    reason: &str,
    content_type: &str,
    body: &[u8],
    cache_policy: CachePolicy,
    head_only: bool,
) -> Result<(), Box<dyn Error>> {
    write!(
        stream,
        "HTTP/1.1 {status_code} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    )?;
    write_security_headers(stream)?;
    match cache_policy {
        CachePolicy::Immutable => write!(
            stream,
            "Cache-Control: public, max-age=31536000, immutable\r\n"
        )?,
        CachePolicy::NoStore => write!(
            stream,
            "Cache-Control: no-cache, no-store, must-revalidate\r\nPragma: no-cache\r\n"
        )?,
    }
    write!(stream, "\r\n")?;
    if !head_only {
        stream.write_all(body)?;
    }
    Ok(())
}

fn write_security_headers(stream: &mut TcpStream) -> Result<(), Box<dyn Error>> {
    write!(
        stream,
        "X-Content-Type-Options: nosniff\r\n\
         Referrer-Policy: strict-origin-when-cross-origin\r\n\
         X-Frame-Options: DENY\r\n\
         Permissions-Policy: camera=(), microphone=(), geolocation=(), payment=(), usb=(), interest-cohort=()\r\n\
         Content-Security-Policy: default-src 'self'; base-uri 'self'; object-src 'none'; frame-ancestors 'none'; form-action 'self'; img-src 'self' data: blob:; font-src 'self' data:; style-src 'self' 'unsafe-inline'; script-src 'self'; connect-src 'self'\r\n"
    )?;
    Ok(())
}

fn content_type_for(path: &Path) -> &'static str {
    match path.extension().and_then(|extension| extension.to_str()).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "txt" => "text/plain; charset=utf-8",
        "wasm" => "application/wasm",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        _ => "application/octet-stream",
    }
}
