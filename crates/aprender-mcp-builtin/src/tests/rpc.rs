//! Drive a server over the fork's stateless loopback the way the platform's smoke does:
//! JSON-RPC 2.0 over HTTP POST with the proxy's identity headers.

use std::net::SocketAddr;

use aprender_mcp_decide_lambda::start_loopback;

/// Serve `server` on an ephemeral loopback port: `(url, task)`.
pub async fn serve(server: pmcp::Server) -> (String, tokio::task::JoinHandle<()>) {
    let addr: SocketAddr = "127.0.0.1:0".parse().expect("addr");
    let (bound, handle) = start_loopback(server, addr).await.expect("bind loopback");
    (format!("http://{bound}/"), handle)
}

/// POST one JSON-RPC request and return the parsed response object.
pub async fn call(
    url: &str,
    method: &str,
    params: serde_json::Value,
    headers: &[(&str, &str)],
) -> serde_json::Value {
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params,
    });
    let mut req = reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("client")
        .post(url)
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .body(body.to_string());
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    let resp = req.send().await.expect("the server answers");
    let status = resp.status().as_u16();
    let text = resp.text().await.expect("body");
    assert_eq!(status, 200, "{method}: HTTP {status}: {text}");
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{method}: not JSON ({e}): {text}"))
}

/// `tools/call`: the `result` object, or the `error` object when the call failed.
pub async fn tool(
    url: &str,
    name: &str,
    args: serde_json::Value,
    headers: &[(&str, &str)],
) -> serde_json::Value {
    let resp = call(
        url,
        "tools/call",
        serde_json::json!({ "name": name, "arguments": args }),
        headers,
    )
    .await;
    if resp["error"].is_null() {
        resp["result"].clone()
    } else {
        resp["error"].clone()
    }
}
