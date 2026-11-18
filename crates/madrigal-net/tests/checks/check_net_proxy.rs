use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::time::Duration;

use madrigal_net::{
    Config, DohProvider, DohResolver, ProxyConfig, ProxyProtocol, build_client,
    build_stream_client, test_connectivity,
};

fn isolate() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    for (key, sub) in [
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_DATA_HOME", "data"),
        ("XDG_CACHE_HOME", "cache"),
        ("XDG_STATE_HOME", "state"),
    ] {
        let path = dir.path().join(sub);
        std::fs::create_dir_all(&path).expect("xdg subdir");
        std::env::set_var(key, &path);
    }
    for key in [
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "NO_PROXY",
        "http_proxy",
        "https_proxy",
        "all_proxy",
        "no_proxy",
    ] {
        std::env::remove_var(key);
    }
    dir
}

fn base64(input: &str) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = input.as_bytes();
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let mut word: u32 = 0;
        for (i, byte) in chunk.iter().enumerate() {
            word |= (*byte as u32) << (8 * (2 - i));
        }
        for (i, slot) in (0..4).enumerate() {
            if i < chunk.len() + 1 {
                out.push(ALPHABET[((word >> (6 * (3 - slot))) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn read_request(stream: &mut std::net::TcpStream) -> String {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        let read = stream.read(&mut chunk).expect("read request");
        if read == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..read]);
        if buf.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }
    String::from_utf8_lossy(&buf).into_owned()
}

fn reply(stream: &mut std::net::TcpStream, status: u16, body: &str) {
    let reason = if status == 200 { "OK" } else { "Error" };
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).expect("write reply");
    stream.flush().expect("flush reply");
}

fn mock_endpoint(body: &'static str) -> (u16, mpsc::Receiver<String>, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock");
    let port = listener.local_addr().expect("mock addr").port();
    let (done, seen) = mpsc::channel();
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept mock");
        let request = read_request(&mut stream);
        reply(&mut stream, 200, body);
        done.send(request).expect("report request");
    });
    (port, seen, handle)
}

fn proxy_config(port: u16) -> Config {
    Config::new()
        .with_user_agent("Madrigal-test/0.1")
        .with_timeout(Duration::from_secs(5))
        .with_proxy(ProxyConfig {
            protocol: ProxyProtocol::Http,
            host: "127.0.0.1".to_owned(),
            port,
            username: Some("user".to_owned()),
            password: Some("secret".to_owned()),
        })
}

#[test]
fn requests_route_through_configured_http_proxy() {
    let _xdg = isolate();
    let (proxy_port, seen, proxy) = mock_endpoint("proxied");
    let target_port = 18081u16;
    let config = proxy_config(proxy_port);
    let client = build_client(&config).expect("build client");
    let body = client
        .get(format!("http://127.0.0.1:{target_port}/resource?q=1"))
        .send()
        .expect("send through proxy")
        .text()
        .expect("read body");
    assert_eq!(body, "proxied");
    let request = seen.recv_timeout(Duration::from_secs(5)).expect("proxy saw request");
    let lowered = request.to_lowercase();
    assert!(
        lowered.starts_with(&format!("get http://127.0.0.1:{target_port}/resource?q=1 ")),
        "proxy must receive absolute-form uri, got: {request}"
    );
    let expected = format!("proxy-authorization: basic {}", base64("user:secret").to_lowercase());
    assert!(lowered.contains(&expected), "proxy auth header missing, got: {request}");
    assert!(
        lowered.contains("user-agent: madrigal-test/0.1"),
        "user agent not applied, got: {request}"
    );
    proxy.join().expect("proxy thread");
}

#[test]
fn stream_client_bypasses_proxy() {
    let _xdg = isolate();
    let (direct_port, seen, direct) = mock_endpoint("direct");
    let closed = TcpListener::bind("127.0.0.1:0").expect("bind unused proxy port");
    let unused_proxy_port = closed.local_addr().expect("proxy addr").port();
    drop(closed);
    let mut config = proxy_config(unused_proxy_port);
    config.connectivity_url = format!("http://127.0.0.1:{direct_port}/connect");
    let client = build_stream_client(&config).expect("build stream client");
    let body = client
        .get(format!("http://127.0.0.1:{direct_port}/stream/1"))
        .send()
        .expect("send direct")
        .text()
        .expect("read body");
    assert_eq!(body, "direct");
    let request = seen.recv_timeout(Duration::from_secs(5)).expect("direct saw request");
    let lowered = request.to_lowercase();
    assert!(
        lowered.starts_with("get /stream/1 "),
        "stream client must use origin-form uri, got: {request}"
    );
    assert!(
        !lowered.contains("proxy-authorization"),
        "stream client must not send proxy auth, got: {request}"
    );
    direct.join().expect("direct thread");
}

#[test]
fn connectivity_probe_reports_latency_and_failure() {
    let _xdg = isolate();
    let (port, _seen, server) = mock_endpoint("");
    let config = Config::new()
        .with_timeout(Duration::from_secs(5))
        .with_connectivity_url(&format!("http://127.0.0.1:{port}/generate_204"));
    let probe = test_connectivity(&config).expect("connectivity ok");
    assert_eq!(probe.status, 200);
    assert!(probe.latency < Duration::from_secs(5), "latency {:?}", probe.latency);
    server.join().expect("probe server thread");

    let closed = TcpListener::bind("127.0.0.1:0").expect("bind closed");
    let closed_port = closed.local_addr().expect("closed addr").port();
    drop(closed);
    let failing = Config::new()
        .with_timeout(Duration::from_secs(2))
        .with_connectivity_url(&format!("http://127.0.0.1:{closed_port}/generate_204"));
    assert!(test_connectivity(&failing).is_err(), "closed port must fail");
}

#[test]
fn doh_providers_compile_resolver_without_network() {
    let _xdg = isolate();
    for (provider, template) in [
        (DohProvider::Cloudflare, "https://cloudflare-dns.com/dns-query"),
        (DohProvider::Google, "https://dns.google/dns-query"),
        (DohProvider::AdGuard, "https://dns.adguard-dns.com/dns-query"),
        (DohProvider::Quad9, "https://dns.quad9.net/dns-query"),
    ] {
        assert_eq!(provider.template_url(), template);
        let config = DohResolver::new(provider).resolver_config().expect("resolver config");
        assert_eq!(config.name_servers().len(), 2, "two bootstrap nameservers");
    }
    let custom_ip = DohProvider::Custom("https://192.0.2.1/dns-query".to_owned());
    assert_eq!(custom_ip.template_url(), "https://192.0.2.1/dns-query");
    let config = DohResolver::new(custom_ip).resolver_config().expect("ip-literal custom");
    assert_eq!(config.name_servers().len(), 1);

    let custom_host = DohProvider::Custom("https://doh.example.net/dns-query".to_owned());
    assert!(
        DohResolver::new(custom_host).resolver_config().is_err(),
        "custom hostname needs a bootstrap lookup, which needs network"
    );
}
