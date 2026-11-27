//! Anthropic akışının uçtan uca testi — sahte SSE sunucusuna karşı.
//!
//! Anthropic'in olay tipleri OpenAI'dan tamamen farklı; birim testler
//! ayrıştırmayı doğruluyor, bu test tam HTTP yolunu doğruluyor.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use vavis_brain::{BrainClient, ChatConfig, Message, Provider, StreamEvent};

fn spawn_server(body: &'static str) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();

    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        {
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            while reader.read_line(&mut line).unwrap() > 0 {
                if line == "\r\n" || line == "\n" {
                    break;
                }
                line.clear();
            }
        }
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\
             Content-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream.write_all(response.as_bytes()).unwrap();
        stream.flush().unwrap();
    });
    port
}

fn config(port: u16) -> ChatConfig {
    ChatConfig::new(Provider::Anthropic, "claude-sonnet-5", "sk-ant-test")
        .with_url(format!("http://127.0.0.1:{port}/v1/messages"))
}

#[tokio::test]
async fn text_stream_is_assembled() {
    let body = "event: message_start\n\
                data: {\"type\":\"message_start\"}\n\
                event: content_block_delta\n\
                data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Mer\"}}\n\
                event: content_block_delta\n\
                data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"haba\"}}\n\
                event: message_stop\n\
                data: {\"type\":\"message_stop\"}\n";
    let port = spawn_server(body);

    let mut deltas = Vec::new();
    let resp = BrainClient::new()
        .chat_stream_with_tools(&config(port), vec![Message::user("selam")], &[], |e| {
            if let StreamEvent::Delta(t) = e {
                deltas.push(t);
            }
        })
        .await
        .unwrap();

    assert_eq!(resp.text, "Merhaba");
    assert_eq!(deltas, vec!["Mer", "haba"], "parçalar sırayla gelmeli");
}

#[tokio::test]
async fn tool_use_is_assembled_from_json_deltas() {
    let body = "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"toolu_1\",\"name\":\"get_system_status\"}}\n\
                data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{}\"}}\n\
                data: {\"type\":\"message_stop\"}\n";
    let port = spawn_server(body);

    let resp = BrainClient::new()
        .chat_stream_with_tools(&config(port), vec![Message::user("cpu?")], &[], |_| {})
        .await
        .unwrap();

    assert_eq!(resp.tool_calls.len(), 1);
    assert_eq!(resp.tool_calls[0].function.name, "get_system_status");
    assert_eq!(resp.tool_calls[0].id, "toolu_1");
}

#[tokio::test]
async fn unknown_events_do_not_break_the_stream() {
    // ping, message_delta gibi olaylar sessizce atlanmalı.
    let body = "data: {\"type\":\"ping\"}\n\
                data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"tamam\"}}\n\
                data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"}}\n\
                data: {\"type\":\"message_stop\"}\n";
    let port = spawn_server(body);

    let resp = BrainClient::new()
        .chat_stream_with_tools(&config(port), vec![Message::user("x")], &[], |_| {})
        .await
        .unwrap();

    assert_eq!(resp.text, "tamam");
}

#[tokio::test]
async fn missing_key_fails_before_network() {
    let cfg = ChatConfig::new(Provider::Anthropic, "claude-sonnet-5", "");
    let err = BrainClient::new()
        .chat_stream(&cfg, vec![Message::user("x")], |_| {})
        .await
        .unwrap_err();
    assert!(matches!(err, vavis_brain::BrainError::MissingKey { .. }));
}

#[tokio::test]
async fn model_listing_uses_anthropic_key_and_version_headers() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request: Vec<_> = BufReader::new(stream.try_clone().unwrap())
            .lines()
            .map(|line| line.unwrap().to_ascii_lowercase())
            .take_while(|line| !line.is_empty())
            .collect();
        let correct = request[0].starts_with("get /v1/models ")
            && request.iter().any(|line| line == "x-api-key: fake-key")
            && request
                .iter()
                .any(|line| line == "anthropic-version: 2023-06-01");
        let (status, body) = if correct {
            ("200 OK", r#"{"data":[{"id":"claude-test"}]}"#)
        } else {
            ("401 Unauthorized", "{}")
        };
        write!(stream, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
    });
    let address = format!("http://127.0.0.1:{port}/v1/chat/completions");
    let models = BrainClient::new()
        .list_models_at(Provider::Anthropic, "fake-key", Some(&address))
        .await;
    server.join().unwrap();
    assert_eq!(models.unwrap(), ["claude-test"]);
}
