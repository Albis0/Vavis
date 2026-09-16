//! Yerleşik araçlı modellere ne gönderildiği — **gövde üzerinden** ölçülür.
//!
//! Neden birim testi yetmiyor: `builtin::tool_support` doğru cevabı verse
//! bile istemci onu çağırmayı unutabilir. Asıl soru "fonksiyon ne dönüyor"
//! değil, "telde ne var". Bu yüzden testler gerçek bir HTTP sunucusuna
//! bağlanıyor ve isteğin gövdesini okuyor.
//!
//! Canlı Groq'a gitmiyor: tek ölçülen şey bizim ne gönderdiğimiz, ve karşı
//! tarafın 400 verdiği zaten ölçülüp `builtin.rs` başlığına yazıldı.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use vavis_brain::{BrainClient, ChatConfig, Provider};

/// Bir istek kabul eden, gövdesini geri yollayan ve boş bir akışla cevap
/// veren asgari sunucu.
fn recording_server() -> (String, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("port açılamadı");
    let url = format!("http://{}/v1/chat", listener.local_addr().unwrap());
    let (tx, rx) = mpsc::channel();

    std::thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        let mut reader = BufReader::new(stream.try_clone().unwrap());

        // Başlıklar: sadece gövde uzunluğu ilgilendiriyor.
        let mut len = 0usize;
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) == 0 {
                return;
            }
            if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                len = v.trim().parse().unwrap_or(0);
            }
            if line == "\r\n" || line == "\n" {
                break;
            }
        }

        let mut body = vec![0u8; len];
        reader.read_exact(&mut body).ok();
        let _ = tx.send(String::from_utf8_lossy(&body).into_owned());

        let sse = "data: {\"choices\":[{\"delta\":{\"content\":\"tamam\"}}]}\n\n\
                   data: [DONE]\n\n";
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

fn a_tool() -> serde_json::Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": "web_search",
            "description": "internette arar",
            "parameters": { "type": "object", "properties": {}, "required": [] }
        }
    })
}

/// Bir tur çalıştırır ve gönderilen gövdeyi döner.
async fn body_sent_for(model: &str) -> serde_json::Value {
    let (url, rx) = recording_server();
    let cfg = ChatConfig::new(Provider::Groq, model, "anahtar").with_url(url);
    let _ = BrainClient::new()
        .chat_stream_with_tools(
            &cfg,
            vec![vavis_brain::message::Message::user("merhaba")],
            &[a_tool()],
            |_| {},
        )
        .await;
    let raw = rx.recv_timeout(std::time::Duration::from_secs(10)).expect("istek gelmedi");
    serde_json::from_str(&raw).expect("gövde JSON değil")
}

/// Compound tek bir şema görürse isteğin tamamını 400 ile reddediyor.
/// Dolayısıyla `tools` alanı gövdede **hiç** bulunmamalı — boş dizi de değil,
/// alanın kendisi olmamalı.
#[tokio::test]
async fn compound_gets_no_tools_field_at_all() {
    let body = body_sent_for("groq/compound").await;
    assert!(
        body.get("tools").is_none(),
        "compound'a tools gönderildi: {body}"
    );
    assert!(body.get("tool_choice").is_none(), "{body}");
}

#[tokio::test]
async fn compound_mini_is_treated_the_same() {
    let body = body_sent_for("groq/compound-mini").await;
    assert!(body.get("tools").is_none(), "{body}");
}

/// Sıradan bir Groq modeli şemalarımızı almaya devam etmeli — süzgeç fazla
/// geniş olursa asistan sessizce araçsız kalır ve kimse fark etmez.
#[tokio::test]
async fn an_ordinary_groq_model_still_receives_our_schemas() {
    let body = body_sent_for("qwen/qwen3.8-27b").await;
    let tools = body["tools"].as_array().expect("tools yok");
    assert_eq!(tools.len(), 1, "{body}");
    assert_eq!(tools[0]["function"]["name"], "web_search");
    // Ölçüldü: qwen yerleşik tipleri reddediyor, aralarına karışmamalı.
    assert!(
        tools.iter().all(|t| t["type"] == "function"),
        "qwen'e yerleşik tip gitti: {body}"
    );
}

/// GPT-OSS ikisini birden kabul ediyor (ölçüldü, 200) — yerleşik arama
/// eklenirken bizimkiler korunmalı.
#[tokio::test]
async fn gpt_oss_gets_the_builtin_search_next_to_our_schemas() {
    let body = body_sent_for("openai/gpt-oss-120b").await;
    let tools = body["tools"].as_array().expect("tools yok");
    assert!(
        tools.iter().any(|t| t["type"] == "browser_search"),
        "yerleşik arama eklenmedi: {body}"
    );
    assert!(
        tools.iter().any(|t| t["function"]["name"] == "web_search"),
        "kendi şemamız düştü: {body}"
    );
}
