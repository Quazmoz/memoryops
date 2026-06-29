use std::{
    env,
    error::Error,
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    process,
    time::Duration,
};

const DEFAULT_HOST: &str = "127.0.0.1";
const DEFAULT_PORT: &str = "8080";
const DEFAULT_PATH: &str = "/health/ready";
const TIMEOUT_SECS: u64 = 2;

fn main() {
    if let Err(error) = run() {
        eprintln!("memoryops healthcheck failed: {error}");
        process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut args = env::args().skip(1);
    let port = args
        .next()
        .or_else(|| env::var("MEMORYOPS_HEALTHCHECK_PORT").ok())
        .or_else(|| env::var("APP_PORT").ok())
        .or_else(|| env::var("MCP_PORT").ok())
        .unwrap_or_else(|| DEFAULT_PORT.to_owned());
    let path = args
        .next()
        .or_else(|| env::var("MEMORYOPS_HEALTHCHECK_PATH").ok())
        .unwrap_or_else(|| DEFAULT_PATH.to_owned());

    if !path.starts_with('/') {
        return Err(format!("healthcheck path must start with '/': {path}").into());
    }

    let address: SocketAddr = format!("{DEFAULT_HOST}:{port}").parse()?;
    let timeout = Duration::from_secs(TIMEOUT_SECS);
    let mut stream = TcpStream::connect_timeout(&address, timeout)?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;

    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: {DEFAULT_HOST}\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(request.as_bytes())?;

    let mut response = [0_u8; 128];
    let bytes_read = stream.read(&mut response)?;
    let response = std::str::from_utf8(&response[..bytes_read])?;

    if response.starts_with("HTTP/1.1 200") || response.starts_with("HTTP/1.0 200") {
        Ok(())
    } else {
        Err(format!("unexpected readiness response: {response:?}").into())
    }
}
