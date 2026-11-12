//! Shared helpers for the CLI integration tests.

use std::io::Read;
use std::net::TcpStream;

/// Read one complete HTTP/1.1 request: the headers, then exactly
/// `Content-Length` bytes of body.
///
/// A single `read` is allowed to return only part of the request. If the fake
/// server answers and drops the socket while the client is still sending the
/// rest, the kernel answers the outstanding bytes with a reset, which the
/// client reports as `error sending request`. Reading the whole request before
/// responding keeps that race out of the test suite.
pub fn read_full_request(stream: &mut TcpStream) -> String {
    let mut raw = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => raw.extend_from_slice(&chunk[..n]),
        }
        let Some(body_start) = raw
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .map(|headers_end| headers_end + 4)
        else {
            continue;
        };
        let head = String::from_utf8_lossy(&raw[..body_start]).to_ascii_lowercase();
        let body_len = head
            .lines()
            .find_map(|line| line.strip_prefix("content-length:"))
            .and_then(|value| value.trim().parse::<usize>().ok())
            .unwrap_or(0);
        if raw.len() >= body_start + body_len {
            break;
        }
    }
    String::from_utf8_lossy(&raw).to_string()
}

/// Write one complete HTTP/1.1 response and close the connection.
///
/// `Connection: close` is the load-bearing part: without it the client pools
/// the keep-alive connection, and when the fake server drops the socket the
/// pool can hand the dying connection back for the next request, whose write
/// meets the close with a reset (`error sending request`). The fake server
/// answers one request per connection, so it says so.
pub fn respond(stream: &mut TcpStream, status_line: &str, body: &str) {
    use std::io::Write;
    let response = format!(
        "HTTP/1.1 {status_line}\r\ncontent-length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

#[cfg(test)]
mod tests {
    use super::{read_full_request, respond};
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::thread;
    use std::time::Duration;

    /// A request can arrive in more than one segment. The helper must keep
    /// reading until the declared body has landed: a response written over the
    /// client's unsent bytes is a reset on the wire, not a served request.
    #[test]
    fn reads_a_request_split_across_segments() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let addr = listener.local_addr().expect("local addr");
        let client = thread::spawn(move || {
            let mut stream = TcpStream::connect(addr).expect("connect");
            stream
                .write_all(b"POST /v1/chat/completions HTTP/1.1\r\ncontent-length: 5\r\n\r\n")
                .expect("write headers");
            thread::sleep(Duration::from_millis(50));
            stream.write_all(b"hello").expect("write body");
        });

        let (mut server, _) = listener.accept().expect("accept");
        let request = read_full_request(&mut server);
        assert!(request.ends_with("hello"), "{request}");
        client.join().expect("client thread");
    }

    /// The server answers one request per connection, so it must say so:
    /// without `Connection: close` the client pools the connection and a
    /// later reuse meets the server's close with a reset.
    #[test]
    fn respond_says_close_and_declares_the_body() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let addr = listener.local_addr().expect("local addr");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            respond(&mut stream, "200 OK", "hello body");
        });

        let mut client = TcpStream::connect(addr).expect("connect");
        let mut response = String::new();
        client.read_to_string(&mut response).expect("read response");
        server.join().expect("server thread");

        let (head, body) = response.split_once("\r\n\r\n").expect("head/body split");
        assert!(head.starts_with("HTTP/1.1 200 OK"), "{head}");
        assert!(
            head.to_ascii_lowercase().contains("connection: close"),
            "the fake server must not be pooled: {head}"
        );
        assert_eq!(body, "hello body");
    }
}
