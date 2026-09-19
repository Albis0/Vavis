//! Gemini'ye telde ne gönderildiği.
//!
//! Birim testleri `gemini::build_body`'nin doğru şekli ürettiğini gösteriyor.
//! Bu testler ayrı bir soruyu cevaplıyor: **istemci onu gerçekten kullanıyor
//! mu.** Groq tarafında öğrenilen ders — karar fonksiyonu doğru cevap verse
//! bile çağıran taraf onu atlayabilir ve birim testi bunu göremez.
//!
//! Gerçek bir soket açılıyor, gövde ve başlıklar okunuyor.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use vavis_brain::{message::Message, BrainClient, ChatConfig, Provider};

struct Seen {
    body: serde_json::Value,
    headers: Vec<(String, String)>,
    target: String,
}

fn recording_server(sse: &'static str) -> (String, mpsc::Receiver<Seen>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("port açılamadı");
    let url = format!("http://{}/v1beta/x", listener.local_addr().unwrap());
    let (tx, rx) = mpsc::channel();

    std::thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        let mut reader = BufReader::new(stream.try_clone().unwrap());

        let mut request_line = String::new();
        reader.read_line(&mut request_line).ok();
        let target = request_line
            .split_whitespace()
            .nth(1)
            .unwrap_or_default()
            .to_string();

        let mut len = 0usize;
        let mut headers = Vec::new();
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) == 0 {
                return;
            }
            if line == "\r\n" || line == "\n" {
                break;
            }
            if let Some((k, v)) = line.split_once(':') {
                let k = k.trim().to_ascii_lowercase();
                let v = v.trim().to_string();
                if k == "content-length" {
                    len = v.parse().unwrap_or(0);
                }
                headers.push((k, v));
            }
        }

        let mut body = vec![0u8; len];
        reader.read_exact(&mut body).ok();
        let _ = tx.send(Seen {
            body: serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null),
            headers,
            target,
        });

        let _ = write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\
             Content-Length: {}\r\nConnection: close\r\n\r\n{sse}",
            sse.len()
        );
        let _ = stream.flush();
    });

    (url, rx)
}

const ONE_WORD: &str = "data: {\"candidates\":[{\"content\":{\"role\":\"model\",\
                        \"parts\":[{\"text\":\"merhaba\"}]}}]}\n\n";

async fn run(sse: &'static str, tools: &[serde_json::Value]) -> (Seen, vavis_brain::ChatResponse) {
    let (url, rx) = recording_server(sse);
    let cfg = ChatConfig::new(Provider::Gemini, "gemini-2.5-flash", "GIZLI-ANAHTAR").with_url(url);
    let out = BrainClient::new()
        .chat_stream_with_tools(
            &cfg,
            vec![Message::system("sen vavis'sin"), Message::user("selam")],
            tools,
            |_| {},
        )
        .await
        .expect("akış başarısız");
    let seen = rx
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("istek gelmedi");
    (seen, out)
}

/// Gemini yolu gerçekten kullanılıyor mu — yoksa OpenAI gövdesi mi gidiyor?
#[tokio::test]
async fn the_body_is_geminis_shape_not_openais() {
    let (seen, _) = run(ONE_WORD, &[]).await;
    assert!(seen.body.get("contents").is_some(), "{}", seen.body);
    assert!(
        seen.body.get("messages").is_none(),
        "OpenAI gövdesi gitmiş: {}",
        seen.body
    );
    assert!(seen.body.get("max_tokens").is_none(), "{}", seen.body);
    assert!(
        seen.body["generationConfig"]["maxOutputTokens"].is_number(),
        "{}",
        seen.body
    );
}

/// Sistem istemi dizide değil, ayrı alanda olmalı.
#[tokio::test]
async fn the_system_prompt_rides_in_its_own_field() {
    let (seen, _) = run(ONE_WORD, &[]).await;
    assert_eq!(
        seen.body["systemInstruction"]["parts"][0]["text"],
        "sen vavis'sin"
    );
    // Ve dizide kalmamalı: Gemini `system` diye bir rol tanımıyor.
    let contents = seen.body["contents"].as_array().unwrap();
    assert_eq!(contents.len(), 1, "{}", seen.body);
    assert_eq!(contents[0]["role"], "user");
}

/// Anahtar başlıkta gitmeli, URL'de değil.
///
/// URL'e girerse istek kaydına, vekil sunucuya ve kullanıcıya gösterdiğimiz
/// hata gövdesine sızar. Ayrıca Google'ın yeni anahtarları sorgu
/// parametresini zaten kabul etmiyor.
#[tokio::test]
async fn the_key_travels_in_a_header_and_never_in_the_url() {
    let (seen, _) = run(ONE_WORD, &[]).await;

    let header = seen
        .headers
        .iter()
        .find(|(k, _)| k == "x-goog-api-key")
        .map(|(_, v)| v.as_str());
    assert_eq!(
        header,
        Some("GIZLI-ANAHTAR"),
        "başlıklar: {:?}",
        seen.headers
    );

    assert!(
        !seen.target.contains("GIZLI-ANAHTAR") && !seen.target.contains("key="),
        "anahtar URL'e sızdı: {}",
        seen.target
    );
    // Ve Bearer olarak da gitmemeli — Google onu okumuyor, ama sızıntı yüzeyi.
    assert!(
        !seen.headers.iter().any(|(k, _)| k == "authorization"),
        "{:?}",
        seen.headers
    );
}

/// Araçlar tek bir `functionDeclarations` listesine sarılmalı.
#[tokio::test]
async fn tools_arrive_as_function_declarations() {
    let tools = vec![serde_json::json!({
        "type": "function",
        "function": {
            "name": "saat",
            "description": "saati söyler",
            "parameters": { "type": "object", "properties": {}, "required": [] }
        }
    })];
    let (seen, _) = run(ONE_WORD, &tools).await;

    let decls = seen.body["tools"][0]["functionDeclarations"]
        .as_array()
        .unwrap_or_else(|| panic!("beklenen şekil yok: {}", seen.body));
    assert_eq!(decls[0]["name"], "saat");
    // OpenAI sarmalayıcısı sızmamalı.
    assert!(seen.body["tools"][0].get("function").is_none());
    assert_eq!(
        seen.body["toolConfig"]["functionCallingConfig"]["mode"],
        "AUTO"
    );
}

/// Araçsız turda `toolConfig` hiç olmamalı.
#[tokio::test]
async fn no_tools_means_no_tool_fields_on_the_wire() {
    let (seen, _) = run(ONE_WORD, &[]).await;
    assert!(seen.body.get("tools").is_none(), "{}", seen.body);
    assert!(seen.body.get("toolConfig").is_none(), "{}", seen.body);
}

/// Metin gerçekten okunuyor mu.
#[tokio::test]
async fn the_reply_text_is_read_back_out_of_the_candidates() {
    let (_, out) = run(ONE_WORD, &[]).await;
    assert_eq!(out.text, "merhaba");
    assert!(out.tool_calls.is_empty());
}

/// Gemini'de bitişi bildiren bir olay yok — akış kapanınca bitiyor. Araç
/// çağrısı o kapanışta kaybolmamalı.
#[tokio::test]
async fn a_function_call_survives_a_stream_that_just_ends() {
    const CALL: &str = "data: {\"candidates\":[{\"content\":{\"parts\":[\
                        {\"functionCall\":{\"name\":\"saat\",\"args\":{\"bolge\":\"tr\"}}}]}}]}\n\n";
    let (_, out) = run(CALL, &[]).await;
    assert_eq!(out.tool_calls.len(), 1, "çağrı kayboldu");
    assert_eq!(out.tool_calls[0].function.name, "saat");
    assert!(out.tool_calls[0].function.arguments.contains("bolge"));
}

/// Gemini hands a "thought signature" back with each function call and
/// **requires** it on the next turn. Measured against the live API: without
/// it the whole request is refused with
/// `Function call is missing a thought_signature in functionCall parts`.
#[tokio::test]
async fn a_thought_signature_is_carried_out_of_the_stream() {
    const CALL: &str = "data: {\"candidates\":[{\"content\":{\"parts\":[{                        \"functionCall\":{\"name\":\"saat\",\"args\":{}},                        \"thoughtSignature\":\"IMZA-123\"}]}}]}

";
    let (_, out) = run(CALL, &[]).await;
    assert_eq!(out.tool_calls.len(), 1);
    assert_eq!(
        out.tool_calls[0].provider_state.as_deref(),
        Some("IMZA-123"),
        "imza akıştan çıkarılmadı"
    );
}

/// And it must go back in the same place it came from: beside the call, not
/// inside it.
#[tokio::test]
async fn the_signature_returns_beside_the_call_it_came_with() {
    use vavis_brain::message::{FunctionCall, ToolCall};

    let (url, rx) = recording_server(ONE_WORD);
    let cfg = ChatConfig::new(Provider::Gemini, "gemini-3.5-flash", "k").with_url(url);

    let mut assistant = Message::assistant("");
    assistant.tool_calls = Some(vec![ToolCall {
        id: "saat-1".into(),
        kind: "function".into(),
        provider_state: Some("IMZA-123".into()),
        function: FunctionCall {
            name: "saat".into(),
            arguments: "{}".into(),
        },
    }]);

    let _ = BrainClient::new()
        .chat_stream_with_tools(
            &cfg,
            vec![
                Message::user("saat kaç"),
                assistant,
                Message::tool_result("saat", "16:20"),
            ],
            &[],
            |_| {},
        )
        .await
        .expect("akış başarısız");

    let seen = rx
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("istek gelmedi");

    let model_turn = seen.body["contents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["role"] == "model")
        .unwrap_or_else(|| panic!("model turu yok: {}", seen.body));
    let part = &model_turn["parts"][0];

    // Yanında, içinde değil.
    assert_eq!(part["thoughtSignature"], "IMZA-123", "{part}");
    assert!(
        part["functionCall"].get("thoughtSignature").is_none(),
        "imza çağrının içine konmuş: {part}"
    );
}

/// A call with no signature must not grow an empty one -- every other
/// provider sends none, and a null there is a different request.
#[tokio::test]
async fn no_signature_means_no_field() {
    use vavis_brain::message::{FunctionCall, ToolCall};

    let (url, rx) = recording_server(ONE_WORD);
    let cfg = ChatConfig::new(Provider::Gemini, "gemini-3.5-flash", "k").with_url(url);

    let mut assistant = Message::assistant("");
    assistant.tool_calls = Some(vec![ToolCall {
        id: "saat-1".into(),
        kind: "function".into(),
        provider_state: None,
        function: FunctionCall {
            name: "saat".into(),
            arguments: "{}".into(),
        },
    }]);

    let _ = BrainClient::new()
        .chat_stream_with_tools(&cfg, vec![Message::user("x"), assistant], &[], |_| {})
        .await
        .expect("akış başarısız");

    let seen = rx
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("istek gelmedi");
    let model_turn = seen.body["contents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["role"] == "model")
        .unwrap();
    assert!(
        model_turn["parts"][0].get("thoughtSignature").is_none(),
        "{}",
        seen.body
    );
}
