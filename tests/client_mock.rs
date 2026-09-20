//! Client HTTP 层集成测试：本机起临时 TCP 假服务器，不依赖真实 usememos。
//! 验证：请求路径/Auth 头、JSON 序列化、分页 pageToken、错误与畸形响应。

use memos_cli::api::Client;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;

struct MockServer {
    addr: String,
    requests: Arc<Mutex<Vec<String>>>,
}

impl MockServer {
    fn start(responses: Vec<&'static str>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let reqs = requests.clone();

        thread::spawn(move || {
            // 每个响应处理一个连接；测试发多少个请求就配多少个响应
            for resp in responses {
                let (mut stream, _) = listener.accept().unwrap();
                let request = read_http_request(&mut stream);
                reqs.lock().unwrap().push(request);
                write_response(&mut stream, "200 OK", "application/json", resp);
            }
        });

        MockServer { addr, requests }
    }

    fn start_with_status(responses: Vec<(u16, &'static str, &'static str)>) -> Self {
        // (status, content_type, body)
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let reqs = requests.clone();

        thread::spawn(move || {
            for (code, ctype, body) in responses {
                let (mut stream, _) = listener.accept().unwrap();
                let request = read_http_request(&mut stream);
                reqs.lock().unwrap().push(request);
                let reason = if code == 404 { "Not Found" } else { "OK" };
                write_response(&mut stream, &format!("{code} {reason}"), ctype, body);
            }
        });

        MockServer { addr, requests }
    }

    fn take_requests(&self) -> Vec<String> {
        std::mem::take(&mut *self.requests.lock().unwrap())
    }
}

fn read_http_request(stream: &mut TcpStream) -> String {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    // 读到完整头部（\r\n\r\n）；POST 带 body，继续读 Content-Length 字节
    let mut content_length = 0usize;
    loop {
        let n = stream.read(&mut chunk).unwrap();
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
        let s = String::from_utf8_lossy(&buf);
        if let Some(pos) = s.find("\r\n\r\n") {
            let head = &s[..pos];
            for line in head.lines() {
                if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    content_length = v.trim().parse().unwrap_or(0);
                }
            }
            let have = buf.len() - pos - 4;
            if have >= content_length {
                break;
            }
        }
    }
    String::from_utf8_lossy(&buf).into_owned()
}

fn write_response(stream: &mut TcpStream, status: &str, ctype: &str, body: &str) {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes()).unwrap();
    stream.write_all(body.as_bytes()).unwrap();
    stream.flush().unwrap();
}

#[test]
fn list_sends_path_and_auth_header() {
    let srv = MockServer::start(vec![
        r#"{"memos":[{"name":"memos/m1","content":"hi","tags":[]}],"nextPageToken":""}"#,
    ]);
    let client = Client::new(&srv.addr, "tok-abc");

    let memos = client.list(20).unwrap();

    assert_eq!(memos.len(), 1);
    assert_eq!(memos[0].uid(), "m1");
    let req = &srv.take_requests()[0];
    assert!(req.starts_with("GET /api/v1/memos?pageSize=20 HTTP/1.1"), "{req}");
    let req_lower = req.to_lowercase();
    assert!(req_lower.contains("authorization: bearer tok-abc"), "{req}");
    assert!(req_lower.contains("accept: application/json"), "{req}");
}

#[test]
fn list_all_paginates_with_page_token() {
    let srv = MockServer::start(vec![
        r#"{"memos":[{"name":"memos/a"}],"nextPageToken":"tok-1"}"#,
        r#"{"memos":[{"name":"memos/b"}],"nextPageToken":""}"#,
    ]);
    let client = Client::new(&srv.addr, "t");

    let all = client.list_all().unwrap();

    assert_eq!(all.len(), 2);
    let reqs = srv.take_requests();
    assert_eq!(reqs.len(), 2);
    assert!(reqs[0].starts_with("GET /api/v1/memos?pageSize=100"), "{}", reqs[0]);
    assert!(reqs[1].contains("pageToken=tok-1"), "{}", reqs[1]);
}

#[test]
fn create_serializes_content_and_visibility() {
    let srv = MockServer::start(vec![r#"{"name":"memos/new","content":"hi","tags":[]}"#]);
    let client = Client::new(&srv.addr, "t");

    let memo = client.create("hello world", "PRIVATE").unwrap();

    assert_eq!(memo.name, "memos/new");
    let req = &srv.take_requests()[0];
    assert!(req.starts_with("POST /api/v1/memos HTTP/1.1"), "{req}");
    assert!(req.contains("\"content\":\"hello world\""), "{req}");
    assert!(req.contains("\"visibility\":\"PRIVATE\""), "{req}");
    assert!(req.contains("content-type: application/json"), "{req}");
}

#[test]
fn patch_and_delete_use_correct_method_and_path() {
    let srv = MockServer::start(vec!["{}", ""]);
    let client = Client::new(&srv.addr, "t");

    client.patch("uid-1", "new content").unwrap();
    client.delete("uid-1").unwrap();

    let reqs = srv.take_requests();
    assert!(reqs[0].starts_with("PATCH /api/v1/memos/uid-1 HTTP/1.1"), "{}", reqs[0]);
    assert!(reqs[0].contains("\"content\":\"new content\""), "{}", reqs[0]);
    assert!(reqs[1].starts_with("DELETE /api/v1/memos/uid-1 HTTP/1.1"), "{}", reqs[1]);
}

#[test]
fn http_error_surfaces_status_and_body() {
    let srv = MockServer::start_with_status(vec![(404, "text/plain", "memo not found")]);
    let client = Client::new(&srv.addr, "t");

    let err = client.get("uid-1").unwrap_err().to_string();

    assert!(err.contains("404"), "{err}");
    assert!(err.contains("memo not found"), "{err}");
}

#[test]
fn non_json_success_body_is_parse_error() {
    let srv = MockServer::start_with_status(vec![(200, "text/plain", "<html>oops</html>")]);
    let client = Client::new(&srv.addr, "t");

    let err = client.list(20).unwrap_err().to_string();

    assert!(err.contains("解析响应失败"), "{err}");
}

#[test]
fn empty_body_on_list_is_explicit_error() {
    let srv = MockServer::start_with_status(vec![(200, "application/json", "")]);
    let client = Client::new(&srv.addr, "t");

    let err = client.list(20).unwrap_err().to_string();

    assert!(err.contains("空响应"), "{err}");
}

#[test]
fn list_response_with_items_alias_field_parses() {
    // RPC 风格响应：items 代替 memos 字段（ListResponse 声明 alias）
    let srv = MockServer::start(vec![
        r#"{"items":[{"name":"memos/x","content":"c"}],"nextPageToken":""}"#,
    ]);
    let client = Client::new(&srv.addr, "t");

    let memos = client.list(20).unwrap();

    assert_eq!(memos.len(), 1);
    assert_eq!(memos[0].content, "c");
}