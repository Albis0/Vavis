//! Uçtan uca akış testi — sahte bir SSE sunucusuna karşı.
//!
//! Neden gerekli: birim testler SSE ayrıştırmasını parça parça doğruluyor ama
//! "gerçek bir HTTP akışı baştan sona doğru işleniyor mu" sorusunu yanıtlamıyor.
//! Bu test onu yanıtlar — ağ yok, yerel soket.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use vavis_brain::{BrainClient, ChatConfig, Message, Provider, StreamEvent};

/// İstenen gövdeyi dönen tek atımlık sahte sunucu. Dinlediği portu döner.
fn spawn_sse_server(body: &'static str) -> u16 {
    spawn_sse_parts(&[body.as_bytes()])
}

fn spawn_sse_parts(parts: &[&'static [u8]]) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let parts = parts.to_vec();

    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();

        // İstek başlıklarını tüket (boş satıra kadar).
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
            "HTTP/1.1 200 OK\r\n\
             Content-Type: text/event-stream\r\n\
             Content-Length: {}\r\n\
             Connection: close\r\n\r\n",
            parts.iter().map(|part| part.len()).sum::<usize>(),
        );
        stream.write_all(response.as_bytes()).unwrap();
        for (index, part) in parts.iter().enumerate() {
            if index > 0 {
                std::thread::sleep(std::time::Duration::from_millis(75));
            }
            stream.write_all(part).unwrap();
        }
    });

    port
}

fn config_for(port: u16) -> ChatConfig {
    // Local sağlayıcı anahtar istemez; URL'yi override edemediğimiz için
    // testte Local'ın sabit portunu kullanamayız → doğrudan alan ataması.
    ChatConfig::new(Provider::Local, "test-model", "")
        .with_url(format!("http://127.0.0.1:{port}/v1/chat/completions"))
}

#[tokio::test]
async fn full_stream_is_assembled_in_order() {
    let body = "data: {\"choices\":[{\"delta\":{\"content\":\"Mer\"}}]}\n\
                data: {\"choices\":[{\"delta\":{\"content\":\"haba\"}}]}\n\
                data: {\"choices\":[{\"delta\":{\"content\":\" dünya\"}}]}\n\
                data: [DONE]\n";
    let port = spawn_sse_server(body);

    let mut deltas = Vec::new();
    let full = BrainClient::new()
        .chat_stream(&config_for(port), vec![Message::user("selam")], |e| {
            if let StreamEvent::Delta(t) = e {
                deltas.push(t);
            }
        })
        .await
        .unwrap();

    assert_eq!(full, "Merhaba dünya");
    assert_eq!(
        deltas,
        vec!["Mer", "haba", " dünya"],
        "parçalar sırayla gelmeli"
    );
}

#[tokio::test]
async fn malformed_chunk_does_not_kill_the_stream() {
    // Tek bozuk parça yüzünden tüm cevabı kaybetmemeliyiz.
    let body = "data: {\"choices\":[{\"delta\":{\"content\":\"iyi\"}}]}\n\
                data: {bozuk json\n\
                data: {\"choices\":[{\"delta\":{\"content\":\" kısım\"}}]}\n\
                data: [DONE]\n";
    let port = spawn_sse_server(body);

    let full = BrainClient::new()
        .chat_stream(&config_for(port), vec![Message::user("x")], |_| {})
        .await
        .unwrap();

    assert_eq!(full, "iyi kısım", "bozuk parça atlanıp devam edilmeli");
}

#[tokio::test]
async fn split_line_across_packets_is_buffered() {
    // Gerçek ağda bir JSON satırı iki TCP paketine bölünebilir.
    // Tamponlama çalışmazsa bu senaryo sessizce veri kaybettirir.
    let body = "data: {\"choices\":[{\"delta\":{\"content\":\"tam\"}}]}\n\
                data: [DONE]\n";
    let port = spawn_sse_server(body);

    let full = BrainClient::new()
        .chat_stream(&config_for(port), vec![Message::user("x")], |_| {})
        .await
        .unwrap();

    assert_eq!(full, "tam");
}

#[tokio::test]
async fn utf8_split_between_chunks_survives_for_all_http_providers() {
    let cases: &[(Provider, &[&[u8]])] = &[
        (Provider::Local, &[
            b"data: {\"choices\":[{\"delta\":{\"content\":\"T\xc3",
            b"\xbcrk\xc3\xa7e\"}}]}\n\ndata: [DONE]\n\n",
        ]),
        (Provider::Anthropic, &[
            b"data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"T\xc3",
            b"\xbcrk\xc3\xa7e\"}}\n\ndata: {\"type\":\"message_stop\"}\n\n",
        ]),
        (Provider::Gemini, &[
            b"data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"T\xc3",
            b"\xbcrk\xc3\xa7e\"}]}}]}\n\n",
        ]),
    ];
    for provider in Provider::ALL
        .into_iter()
        .filter(|p| *p != Provider::ClaudeCode)
    {
        let chunks = match provider {
            Provider::Anthropic => cases[1].1,
            Provider::Gemini => cases[2].1,
            _ => cases[0].1,
        };
        let port = spawn_sse_parts(chunks);
        let config = ChatConfig::new(provider, provider.default_model(), "fake-key")
            .with_url(format!("http://127.0.0.1:{port}/chat"));
        let text = BrainClient::new()
            .chat_stream(&config, vec![Message::user("test")], |_| {})
            .await
            .unwrap();
        assert_eq!(text, "Türkçe", "{provider}");
    }
}

#[tokio::test]
async fn an_error_after_http_success_is_not_a_finished_answer() {
    let cases = [
        (Provider::OpenRouter, concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"partial\"}}]}\n\n",
            "data: {\"error\":{\"code\":503,\"message\":\"Unavailable\"},\"choices\":[{\"finish_reason\":\"error\"}]}\n\n",
        ), 503),
        (Provider::Anthropic, concat!(
            "data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"partial\"}}\n\n",
            "event: error\ndata: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"Overloaded\"}}\n\n",
        ), 529),
        (Provider::Gemini, concat!(
            "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"partial\"}]}}]}\n\n",
            "data: {\"error\":{\"code\":429,\"message\":\"Quota exhausted\"}}",
        ), 429),
    ];
    for (provider, body, expected_status) in cases {
        let port = spawn_sse_server(body);
        let config = ChatConfig::new(provider, "synthetic-model", "fake-key")
            .with_url(format!("http://127.0.0.1:{port}/chat"));
        let mut events = Vec::new();
        let result = BrainClient::new()
            .chat_stream(&config, vec![Message::user("test")], |event| {
                events.push(event)
            })
            .await;
        assert!(
            matches!(result, Err(vavis_brain::BrainError::Api { status, .. }) if status == expected_status),
            "{provider}: {result:?}"
        );
        assert_eq!(events, [StreamEvent::Delta("partial".into())], "{provider}");
    }
}

#[tokio::test]
async fn tool_call_fragments_are_merged_across_chunks() {
    // Sağlayıcılar tool çağrısını parçalara böler: ad bir parçada, argümanlar
    // birkaç parçada. Birleştirme çalışmazsa argümanlar bozulur ve model
    // "geçersiz JSON" hatası alır — eski projede yaşanan sorun buydu.
    let body = "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_abc\",\"function\":{\"name\":\"read_file\",\"arguments\":\"\"}}]}}]}\n\
                data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"path\\\":\"}}]}}]}\n\
                data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"\\\"~/test.txt\\\"}\"}}]}}]}\n\
                data: [DONE]\n";
    let port = spawn_sse_server(body);

    let resp = BrainClient::new()
        .chat_stream_with_tools(
            &config_for(port),
            vec![Message::user("dosyayı oku")],
            &[],
            |_| {},
        )
        .await
        .unwrap();

    assert_eq!(resp.tool_calls.len(), 1, "tek çağrı bekleniyordu");
    let call = &resp.tool_calls[0];
    assert_eq!(call.id, "call_abc");
    assert_eq!(call.function.name, "read_file");
    assert_eq!(
        call.function.arguments, r#"{"path":"~/test.txt"}"#,
        "argümanlar parçalardan doğru birleştirilmeli"
    );
}

#[tokio::test]
async fn multiple_parallel_tool_calls_stay_separate() {
    // Model aynı anda iki tool isteyebilir; indeksler karışmamalı.
    let body = "data: {\"choices\":[{\"delta\":{\"tool_calls\":[\
                  {\"index\":0,\"id\":\"a\",\"function\":{\"name\":\"get_current_time\",\"arguments\":\"{}\"}},\
                  {\"index\":1,\"id\":\"b\",\"function\":{\"name\":\"get_battery\",\"arguments\":\"{}\"}}]}}]}\n\
                data: [DONE]\n";
    let port = spawn_sse_server(body);

    let resp = BrainClient::new()
        .chat_stream_with_tools(&config_for(port), vec![Message::user("x")], &[], |_| {})
        .await
        .unwrap();

    assert_eq!(resp.tool_calls.len(), 2);
    assert_eq!(resp.tool_calls[0].function.name, "get_current_time");
    assert_eq!(resp.tool_calls[1].function.name, "get_battery");
}

#[tokio::test]
async fn nameless_tool_call_is_discarded() {
    // Adı gelmemiş çağrı çalıştırılamaz — modele hata döndürmek yerine at.
    let body = "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{}\"}}]}}]}\n\
                data: [DONE]\n";
    let port = spawn_sse_server(body);

    let resp = BrainClient::new()
        .chat_stream_with_tools(&config_for(port), vec![Message::user("x")], &[], |_| {})
        .await
        .unwrap();

    assert!(resp.tool_calls.is_empty(), "adsız çağrı atılmalı");
}

#[tokio::test]
async fn text_and_tool_calls_can_arrive_together() {
    let body = "data: {\"choices\":[{\"delta\":{\"content\":\"Bakıyorum...\"}}]}\n\
                data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"c1\",\"function\":{\"name\":\"get_system_status\",\"arguments\":\"{}\"}}]}}]}\n\
                data: [DONE]\n";
    let port = spawn_sse_server(body);

    let resp = BrainClient::new()
        .chat_stream_with_tools(&config_for(port), vec![Message::user("cpu?")], &[], |_| {})
        .await
        .unwrap();

    assert_eq!(resp.text, "Bakıyorum...");
    assert_eq!(resp.tool_calls.len(), 1);
}
