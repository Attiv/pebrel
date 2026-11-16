//! Owned loopback HTTP fixtures; no provider files or credentials are touched.
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde_json::Value;

pub(crate) struct Captured {
    pub request_line: String,
    pub authorization: Option<String>,
    pub body: Value,
}

pub(crate) fn reply(
    value: Value,
    status: &'static str,
    on_request: impl FnOnce() + Send + 'static,
) -> (String, JoinHandle<Captured>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < deadline, "HTTP fixture received no request");
                    std::thread::sleep(Duration::from_millis(2));
                },
                Err(error) => panic!("HTTP fixture accept: {error}"),
            }
        };
        // Darwin can inherit the listener's nonblocking mode on accepted sockets.
        stream.set_nonblocking(false).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut request_line = String::new();
        reader.read_line(&mut request_line).unwrap();
        let mut length = None;
        let mut authorization = None;
        loop {
            let mut line = String::new();
            assert_ne!(reader.read_line(&mut line).unwrap(), 0);
            if line == "\r\n" {
                break;
            }
            if let Some((name, value)) = line.split_once(':') {
                if name.eq_ignore_ascii_case("content-length") {
                    length = Some(value.trim().parse::<usize>().unwrap());
                } else if name.eq_ignore_ascii_case("authorization") {
                    authorization = Some(value.trim().to_owned());
                }
            }
        }
        let mut body = vec![0; length.unwrap()];
        reader.read_exact(&mut body).unwrap();
        let body = serde_json::from_slice(&body).unwrap();
        on_request();
        let value = value.to_string();
        // A bounded-response client may close early; that is a valid fixture outcome.
        let _ = write!(
            stream,
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{value}",
            value.len()
        );
        Captured { request_line, authorization, body }
    });
    (url, server)
}

#[test]
fn fragmented_http_requests_wait_for_headers_and_body() {
    let body = r#"{"fragmented":true}"#;
    let (url, server) = reply(serde_json::json!({"ok": true}), "200 OK", || {});
    let mut stream = std::net::TcpStream::connect(url.strip_prefix("http://").unwrap()).unwrap();
    stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    stream.write_all(b"POST /fragmented HTTP/1.1\r\n").unwrap();
    std::thread::sleep(Duration::from_millis(100));
    write!(stream, "Content-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    stream.write_all(body.as_bytes()).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    assert!(response.starts_with("HTTP/1.1 200 OK\r\n"));
    let captured = server.join().unwrap();
    assert_eq!(captured.request_line, "POST /fragmented HTTP/1.1\r\n");
    assert_eq!(captured.body, serde_json::json!({"fragmented": true}));
}
