use std::time::Duration;

use floe_agent_contract::{AgentFailure};
use floe_execution::{Cancellation};
use floe_provider_adapters::control::RemotePairingClient;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const PAIRING_ID: &str = "00000000-0000-4000-8000-000000000001";

#[tokio::test]
async fn cancelled_pairing_poll_never_opens_a_connection() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let client =
        RemotePairingClient::new(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
    let cancellation = Cancellation::default();
    cancellation.cancel();
    assert_eq!(
        client
            .status(
                PAIRING_ID,
                "polling-proof",
                tokio::time::Instant::now() + Duration::from_secs(5),
                &cancellation
            )
            .await,
        Err(AgentFailure::Cancelled)
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(30), listener.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn pairing_poll_uses_body_proof_and_cancels_a_stalled_response() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let client =
        RemotePairingClient::new(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
    let (headers_sent, headers_received) = tokio::sync::oneshot::channel();
    let (finish_server, server_finished) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut received = Vec::new();
        let header_end = loop {
            let mut chunk = [0u8; 1024];
            let count = socket.read(&mut chunk).await.unwrap();
            assert!(count > 0);
            received.extend_from_slice(&chunk[..count]);
            assert!(received.len() <= 8192);
            if let Some(position) = received.windows(4).position(|value| value == b"\r\n\r\n") {
                break position + 4;
            }
        };
        let headers = std::str::from_utf8(&received[..header_end]).unwrap();
        assert!(headers.starts_with("POST /pair/poll HTTP/1.1\r\n"));
        assert!(!headers.to_ascii_lowercase().contains("authorization:"));
        let content_length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().unwrap())
            })
            .unwrap();
        assert!(content_length <= 4096);
        while received.len() < header_end + content_length {
            let mut chunk = [0u8; 1024];
            let count = socket.read(&mut chunk).await.unwrap();
            assert!(count > 0);
            received.extend_from_slice(&chunk[..count]);
        }
        let body: serde_json::Value =
            serde_json::from_slice(&received[header_end..header_end + content_length]).unwrap();
        assert_eq!(
            body,
            serde_json::json!({"schema_version": 1, "pairing_id": PAIRING_ID, "proof": "polling-proof"})
        );
        socket
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 4096\r\n\r\n{")
            .await
            .unwrap();
        headers_sent.send(()).unwrap();
        server_finished.await.unwrap();
    });
    let cancellation = Cancellation::default();
    let child = cancellation.clone();
    let request = tokio::spawn(async move {
        client
            .status(
                PAIRING_ID,
                "polling-proof",
                tokio::time::Instant::now() + Duration::from_secs(5),
                &child,
            )
            .await
    });
    tokio::time::timeout(Duration::from_secs(2), headers_received)
        .await
        .unwrap()
        .unwrap();
    cancellation.cancel();
    let result = tokio::time::timeout(Duration::from_secs(2), request)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result, Err(AgentFailure::Cancelled));
    finish_server.send(()).unwrap();
    server.await.unwrap();
}
