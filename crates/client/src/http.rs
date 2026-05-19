use std::io::{Read, Write};
use std::net::TcpStream;

pub(crate) fn get(addr: &str, path: &str) -> Result<String, Box<dyn std::error::Error>> {
    let request = format!(
        "GET {path} HTTP/1.1\r\n\
Host: {addr}\r\n\
Connection: close\r\n\r\n"
    );
    http_request(addr, &request)
}

pub(crate) fn response_body(response: &str) -> Result<&str, Box<dyn std::error::Error>> {
    let status_line = response.lines().next().unwrap_or("<empty response>");
    if !status_line.contains(" 200 ") {
        return Err(format!("request failed: {status_line}").into());
    }

    response
        .split("\r\n\r\n")
        .nth(1)
        .ok_or_else(|| "response has no body".into())
}

fn http_request(addr: &str, request: &str) -> Result<String, Box<dyn std::error::Error>> {
    let mut stream = TcpStream::connect(addr)?;
    stream.write_all(request.as_bytes())?;
    stream.flush()?;

    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    Ok(response)
}
