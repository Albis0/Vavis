//! Gemini — kendi yerel API'si (`generateContent`).
//!
//! Google'ın bir OpenAI-uyumlu uç noktası var ve bir süre onu kullandık.
//! Bırakma sebebi: **uyumluluk katmanı bir alt küme.** Düşünme (thinking)
//! ayarları, güvenlik eşikleri, `systemInstruction`, çok parçalı içerik,
//! `usageMetadata` — hiçbiri o kapıdan geçmiyor. Yerel API'ye geçmek bunların
//! hepsini erişilebilir kılıyor ve Google'ın kendi belgelediği yol bu.
//!
//! Şekil olarak OpenAI'dan **tamamen** farklı, bu yüzden Anthropic gibi ayrı
//! bir modül:
//!
//! | | OpenAI | Gemini |
//! |---|---|---|
//! | Mesaj dizisi | `messages` | `contents` |
//! | Rol adları | `assistant` | `model` |
//! | Sistem istemi | dizide bir mesaj | ayrı `systemInstruction` alanı |
//! | Metin | `content` (düz metin) | `parts[].text` |
//! | Araç şeması | `tools[].function` | `tools[].functionDeclarations[]` |
//! | Araç çağrısı | `tool_calls[]` | `parts[].functionCall` |
//! | Araç sonucu | `role: "tool"` | `role: "user"` + `functionResponse` |
//! | Cevap sınırı | `max_tokens` | `generationConfig.maxOutputTokens` |
//! | Model adı | gövdede | **URL'de** |
//!
//! Son satır önemli: modelin adı gövdede değil yolda duruyor, o yüzden
//! `chat_url()` tek bir sabit olamıyor — [`chat_url`] modele göre kuruluyor.

use crate::message::{FunctionCall, Message, Role, ToolCall};
use serde_json::{json, Value};

/// Yerel API tabanı. `v1beta`, çünkü araç çağırma ve düşünme ayarları hâlâ
/// orada yayınlanıyor; `v1` bunların bir kısmını tanımıyor.
pub const API_BASE: &str = "https://generativelanguage.googleapis.com/v1beta";

/// Model listesi uç noktası.
pub const MODELS_URL: &str = "https://generativelanguage.googleapis.com/v1beta/models";

/// Varsayılan model.
///
/// `gemini-2.5-flash` **değil**: Google onu emekli etti ve kullanmaya
/// çalışınca 404 dönüyor —
///
/// ```text
/// This model models/gemini-2.5-flash is no longer available to new users.
/// Please update your code to use models/gemini-3.6-flash
/// ```
///
/// Model hâlâ `/models` listesinde görünüyor, yani "listede var" ile
/// "çalışıyor" aynı şey değil. Ölçüldü (2026-09-16, canlı API): `2.5-flash`,
/// `2.5-pro` ve `2.5-flash-lite` listede duruyor ama üçü de 404 veriyor.
pub const DEFAULT_MODEL: &str = "gemini-3.6-flash";

/// Anahtarın gideceği başlık.
///
/// **Sorgu parametresi (`?key=...`) kullanılmıyor.** İki sebep var ve ikisi de
/// yeterli:
///
/// 1. Anahtar URL'e girerse her yere sızar — istek kaydı, vekil sunucu logu,
///    hata mesajı, iz kaydı. Bir hata gövdesini kullanıcıya gösterdiğimizde
///    içinde anahtarın kendisi olabilir.
/// 2. Google'ın yeni biçimli anahtarları sorgu parametresini zaten kabul
///    etmiyor — o yolla **404** dönüyor.
pub const KEY_HEADER: &str = "x-goog-api-key";

/// Akan sohbet için tam URL.
///
/// Model adı yolda geçiyor. Google model adlarını `models/` önekiyle
/// yayınlıyor ama kullanıcı ayarlara çıplak adı yazıyor; ikisi de kabul
/// edilsin diye önek varsa korunuyor, yoksa ekleniyor. İki kez eklemek 404
/// veriyor.
///
/// `alt=sse` olmadan Google akışı bir JSON **dizisi** olarak gönderiyor, SSE
/// olarak değil — satır satır çözümleyen kodumuz o biçimi okuyamaz.
pub fn chat_url(model: &str) -> String {
    format!(
        "{API_BASE}/{}:streamGenerateContent?alt=sse",
        qualified(model)
    )
}

/// Akışsız URL — bağlantı denemesi gibi tek seferlik çağrılar için.
pub fn generate_url(model: &str) -> String {
    format!("{API_BASE}/{}:generateContent", qualified(model))
}

/// `gemini-2.5-flash` → `models/gemini-2.5-flash`, zaten önekliyse dokunmaz.
fn qualified(model: &str) -> String {
    let m = model.trim();
    if m.starts_with("models/") {
        m.to_string()
    } else {
        format!("models/{m}")
    }
}

/// Sohbet geçmişini Gemini'nin `contents` dizisine çevirir.
///
/// Dönen: `(systemInstruction metni, contents dizisi)`. Sistem istemi ayrı
/// çıkıyor çünkü Gemini onu dizide bir mesaj olarak değil, gövdenin üstünde
/// ayrı bir alanda bekliyor — dizinin içine `role: "system"` koymak hata
/// veriyor, çünkü geçerli roller sadece `user` ve `model`.
pub fn build_contents(messages: &[Message]) -> (String, Vec<Value>) {
    let mut system = String::new();
    let mut out: Vec<Value> = Vec::new();

    for m in messages {
        match m.role {
            Role::System => {
                if !system.is_empty() {
                    system.push_str("\n\n");
                }
                system.push_str(&m.content);
            }

            // Araç sonucu Gemini'de ayrı bir rol değil: `user` rolünde bir
            // `functionResponse` parçası olarak dönüyor.
            //
            // Bağ **ada** göre kuruluyor, kimliğe göre değil — Gemini'de
            // `tool_call_id` diye bir şey yok. Bu yüzden adı kaybetmemek
            // gerekiyor; bulunamazsa sonuç sahipsiz kalır ve model kendi
            // çağırdığı şeyin cevabını göremez.
            Role::Tool => {
                let name = m.tool_call_id.clone().unwrap_or_default();
                let part = json!({
                    "functionResponse": {
                        "name": name,
                        // Gemini burada bir **nesne** bekliyor, düz metin
                        // değil. Araçlarımız metin döndürüyor, o yüzden tek
                        // alanlı bir nesneye sarılıyor.
                        "response": { "result": m.content },
                    }
                });

                // Ardışık araç sonuçları tek `user` içeriğinde toplanmalı.
                // Ayrı içeriklere bölmek modeli paralel araç kullanmaktan
                // caydırıyor — Anthropic tarafında öğrenilen aynı ders.
                match out.last_mut() {
                    Some(last) if last["role"] == "user" && has_function_response(last) => {
                        if let Some(parts) = last["parts"].as_array_mut() {
                            parts.push(part);
                        }
                    }
                    _ => out.push(json!({ "role": "user", "parts": [part] })),
                }
            }

            Role::User => out.push(json!({
                "role": "user",
                "parts": user_parts(m),
            })),

            Role::Assistant => {
                let mut parts: Vec<Value> = Vec::new();
                if !m.content.trim().is_empty() {
                    parts.push(json!({ "text": m.content }));
                }
                for call in m.tool_calls.iter().flatten() {
                    parts.push(json!({
                        "functionCall": {
                            "name": call.function.name,
                            // Argümanlar bizde JSON **metni**, Gemini'de
                            // nesne. Çözülemezse boş nesne: bozuk bir metni
                            // olduğu gibi göndermek isteğin tamamını düşürür.
                            "args": parse_args(&call.function.arguments),
                        }
                    }));
                }
                // Tamamen boş bir içerik reddediliyor.
                if parts.is_empty() {
                    parts.push(json!({ "text": "" }));
                }
                out.push(json!({ "role": "model", "parts": parts }));
            }
        }
    }

    (system, out)
}

fn has_function_response(content: &Value) -> bool {
    content["parts"]
        .as_array()
        .is_some_and(|p| p.iter().any(|part| part.get("functionResponse").is_some()))
}

/// Kullanıcı mesajının parçaları — görüntü varsa yanına eklenir.
fn user_parts(m: &Message) -> Vec<Value> {
    let mut parts = vec![json!({ "text": m.content })];
    if let Some(img) = &m.image {
        // Gemini `inline_data` bekliyor; veri URI öneki olmadan ham base64.
        parts.push(json!({
            "inline_data": { "mime_type": "image/png", "data": img }
        }));
    }
    parts
}

fn parse_args(raw: &str) -> Value {
    serde_json::from_str(raw).unwrap_or_else(|_| json!({}))
}

/// OpenAI araç şemasını Gemini biçimine çevirir.
///
/// Hepsi **tek** bir `tools` girdisinin içindeki `functionDeclarations`
/// dizisine giriyor — her araç için ayrı girdi açmak değil.
pub fn convert_tools(tools: &[Value]) -> Vec<Value> {
    let declarations: Vec<Value> = tools
        .iter()
        .filter_map(|t| {
            let f = t.get("function")?;
            Some(json!({
                "name": f.get("name")?,
                "description": f.get("description").cloned().unwrap_or(json!("")),
                "parameters": clean_schema(
                    f.get("parameters").cloned().unwrap_or(json!({
                        "type": "object", "properties": {}
                    }))
                ),
            }))
        })
        .collect();

    if declarations.is_empty() {
        Vec::new()
    } else {
        vec![json!({ "functionDeclarations": declarations })]
    }
}

/// Gemini'nin şema ayrıştırıcısının tanımadığı anahtarları atar.
///
/// Google OpenAPI şemasının bir alt kümesini kabul ediyor ve tanımadığı bir
/// anahtar gördüğünde aracı sessizce yok saymak yerine **isteğin tamamını**
/// reddediyor. `additionalProperties` ve `$schema` bizim şemalarımızda
/// bulunuyor ve ikisi de o listede değil.
fn clean_schema(mut schema: Value) -> Value {
    const UNSUPPORTED: [&str; 4] = ["additionalProperties", "$schema", "exclusiveMinimum", "exclusiveMaximum"];

    if let Some(map) = schema.as_object_mut() {
        for key in UNSUPPORTED {
            map.remove(key);
        }
        // İç içe şemalar da temizlenmeli: bir aracın parametresi nesne ya da
        // dizi olabilir ve yasak anahtar orada da durabilir.
        for (_, v) in map.iter_mut() {
            if v.is_object() || v.is_array() {
                *v = clean_schema(v.take());
            }
        }
    } else if let Some(items) = schema.as_array_mut() {
        for v in items.iter_mut() {
            *v = clean_schema(v.take());
        }
    }
    schema
}

/// Tam istek gövdesi.
pub fn build_body(
    messages: &[Message],
    tools: &[Value],
    max_output: usize,
    temperature: f32,
) -> Value {
    let (system, contents) = build_contents(messages);

    let mut body = json!({
        "contents": contents,
        "generationConfig": {
            "temperature": temperature,
            "maxOutputTokens": max_output,
        },
    });

    if !system.is_empty() {
        body["systemInstruction"] = json!({ "parts": [{ "text": system }] });
    }

    let declarations = convert_tools(tools);
    if !declarations.is_empty() {
        body["tools"] = json!(declarations);
        // AUTO: modelin araç çağırıp çağırmayacağına kendisi karar versin.
        // ANY onu her turda bir araç çağırmaya zorlar ki bu, basit bir
        // "merhaba" için bile araç aramasına yol açar.
        body["toolConfig"] = json!({ "functionCallingConfig": { "mode": "AUTO" } });
    }

    body
}

// ── Akış çözümleme ──────────────────────────────────────────────────────────

/// Bir SSE veri satırından çıkan sonuç.
#[derive(Debug, PartialEq)]
pub enum Chunk {
    Text(String),
    /// Model bir araç çağırmak istedi.
    Call(ToolCall),
    /// Bu satırda işlenecek bir şey yok.
    Nothing,
}

/// Akış boyunca biriken durum.
///
/// Anthropic'ten farkı: Gemini araç çağrısını **tek parçada, tam** gönderiyor,
/// argümanları harf harf akıtmıyor. Yani birleştirme derdi yok. Sayaç sadece
/// çağrılara kimlik üretmek için: Gemini kimlik vermiyor ama bizim üst
/// katmanımız cevabı çağrıya bağlamak için bir tanesine ihtiyaç duyuyor.
#[derive(Default)]
pub struct StreamState {
    seen: usize,
}

impl StreamState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Bir `data:` satırının içeriğini işler.
    pub fn feed(&mut self, data: &str) -> Vec<Chunk> {
        let Ok(v) = serde_json::from_str::<Value>(data) else {
            return vec![Chunk::Nothing];
        };

        let Some(parts) = v["candidates"][0]["content"]["parts"].as_array() else {
            return vec![Chunk::Nothing];
        };

        let mut out = Vec::new();
        for part in parts {
            if let Some(text) = part["text"].as_str() {
                if !text.is_empty() {
                    out.push(Chunk::Text(text.to_string()));
                }
            }
            if let Some(call) = part.get("functionCall") {
                let name = call["name"].as_str().unwrap_or_default().to_string();
                if name.is_empty() {
                    continue;
                }
                self.seen += 1;
                out.push(Chunk::Call(ToolCall {
                    // Gemini kimlik vermiyor; sonucu geri bağlarken **ad**
                    // kullanılacak (bkz. `build_contents`, `Role::Tool`).
                    // Buradaki kimlik sadece üst katmanın şeması için.
                    id: format!("{name}-{}", self.seen),
                    kind: "function".into(),
                    function: FunctionCall {
                        arguments: call
                            .get("args")
                            .map(|a| a.to_string())
                            .unwrap_or_else(|| "{}".into()),
                        name,
                    },
                }));
            }
        }

        if out.is_empty() {
            out.push(Chunk::Nothing);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_model_name_travels_in_the_url() {
        let url = chat_url("gemini-2.5-flash");
        assert!(url.contains("models/gemini-2.5-flash:streamGenerateContent"), "{url}");
        // Akış SSE olarak istenmezse Google bir JSON dizisi gönderiyor.
        assert!(url.contains("alt=sse"), "{url}");
    }

    /// Öneki iki kez eklemek 404 veriyor.
    #[test]
    fn an_already_qualified_name_is_not_prefixed_twice() {
        assert!(!chat_url("models/gemini-2.5-pro").contains("models/models/"));
    }

    /// Anahtar URL'e **girmemeli** — oraya girerse loglara, vekil sunucuya
    /// ve kullanıcıya gösterdiğimiz hata gövdesine sızar.
    #[test]
    fn the_key_never_appears_in_the_url() {
        let url = chat_url("gemini-2.5-flash");
        assert!(!url.contains("key="), "anahtar URL'e konmuş: {url}");
        assert_eq!(KEY_HEADER, "x-goog-api-key");
    }

    #[test]
    fn the_system_prompt_leaves_the_message_list() {
        let msgs = vec![Message::system("sen vavis'sin"), Message::user("merhaba")];
        let (system, contents) = build_contents(&msgs);
        assert_eq!(system, "sen vavis'sin");
        assert_eq!(contents.len(), 1, "sistem istemi dizide kalmış");
        // Gemini yalnızca "user" ve "model" rollerini tanıyor.
        assert_eq!(contents[0]["role"], "user");
    }

    #[test]
    fn several_system_messages_become_one_instruction() {
        let msgs = vec![
            Message::system("bir"),
            Message::system("iki"),
            Message::user("merhaba"),
        ];
        assert_eq!(build_contents(&msgs).0, "bir\n\niki");
    }

    #[test]
    fn the_assistant_is_called_model() {
        let msgs = vec![Message::user("selam"), Message::assistant("merhaba")];
        let (_, contents) = build_contents(&msgs);
        assert_eq!(contents[1]["role"], "model");
        assert_eq!(contents[1]["parts"][0]["text"], "merhaba");
    }

    fn call(name: &str, args: &str) -> ToolCall {
        ToolCall {
            id: "yok".into(),
            kind: "function".into(),
            function: FunctionCall {
                name: name.into(),
                arguments: args.into(),
            },
        }
    }

    /// Argümanlar bizde metin, Gemini'de nesne.
    #[test]
    fn tool_arguments_are_sent_as_an_object_not_a_string() {
        let mut m = Message::assistant("");
        m.tool_calls = Some(vec![call("saat", r#"{"bolge":"tr"}"#)]);
        let (_, contents) = build_contents(&[m]);
        let args = &contents[0]["parts"][0]["functionCall"]["args"];
        assert!(args.is_object(), "metin olarak gitti: {args}");
        assert_eq!(args["bolge"], "tr");
    }

    /// Bozuk argüman metni isteğin tamamını düşürmemeli.
    #[test]
    fn unparseable_arguments_become_an_empty_object() {
        let mut m = Message::assistant("");
        m.tool_calls = Some(vec![call("saat", "{bozuk")]);
        let (_, contents) = build_contents(&[m]);
        assert_eq!(contents[0]["parts"][0]["functionCall"]["args"], json!({}));
    }

    /// Gemini'de araç sonucu ayrı bir rol değil.
    #[test]
    fn a_tool_result_comes_back_as_a_user_content() {
        // Kimlik alanı Gemini'de **ad** taşıyor: functionResponse'u
        // çağrıya bağlayan tek şey o.
        let m = Message::tool_result("saat", "13:00");
        let (_, contents) = build_contents(&[m]);
        assert_eq!(contents[0]["role"], "user");
        let r = &contents[0]["parts"][0]["functionResponse"];
        assert_eq!(r["name"], "saat");
        // Gemini burada nesne bekliyor, düz metin değil.
        assert!(r["response"].is_object(), "{r}");
    }

    /// Ardışık sonuçlar tek içerikte toplanmalı — ayırmak modeli paralel
    /// araç kullanmaktan caydırıyor.
    #[test]
    fn consecutive_tool_results_share_one_content() {
        let a = Message::tool_result("saat", "13:00");
        let b = Message::tool_result("hava", "güneşli");
        let (_, contents) = build_contents(&[a, b]);
        assert_eq!(contents.len(), 1, "ayrı içeriklere bölünmüş");
        assert_eq!(contents[0]["parts"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn tools_are_wrapped_in_a_single_declaration_list() {
        let tools = vec![
            json!({"type":"function","function":{"name":"a","parameters":{"type":"object"}}}),
            json!({"type":"function","function":{"name":"b","parameters":{"type":"object"}}}),
        ];
        let out = convert_tools(&tools);
        assert_eq!(out.len(), 1, "her araç için ayrı girdi açılmış");
        assert_eq!(out[0]["functionDeclarations"].as_array().unwrap().len(), 2);
    }

    /// Google tanımadığı bir şema anahtarı görünce isteğin tamamını
    /// reddediyor — aracı yok saymıyor.
    #[test]
    fn schema_keys_google_rejects_are_stripped() {
        let tools = vec![json!({
            "type": "function",
            "function": {
                "name": "yaz",
                "parameters": {
                    "type": "object",
                    "additionalProperties": false,
                    "$schema": "http://json-schema.org/draft-07/schema#",
                    "properties": {
                        "hedef": { "type": "object", "additionalProperties": false }
                    }
                }
            }
        })];
        let params = &convert_tools(&tools)[0]["functionDeclarations"][0]["parameters"];
        assert!(params.get("additionalProperties").is_none(), "{params}");
        assert!(params.get("$schema").is_none(), "{params}");
        // İç içe olan da temizlenmeli.
        assert!(
            params["properties"]["hedef"]
                .get("additionalProperties")
                .is_none(),
            "iç içe şema temizlenmemiş: {params}"
        );
    }

    #[test]
    fn the_reply_limit_lives_under_generation_config() {
        let body = build_body(&[Message::user("selam")], &[], 4096, 0.7);
        assert_eq!(body["generationConfig"]["maxOutputTokens"], 4096);
        // f32 -> JSON, so compare with a tolerance rather than for equality.
        let temp = body["generationConfig"]["temperature"].as_f64().unwrap();
        assert!((temp - 0.7).abs() < 1e-6, "{temp}");
        // OpenAI adları gövdeye sızmamalı.
        assert!(body.get("max_tokens").is_none());
        assert!(body.get("messages").is_none());
    }

    #[test]
    fn no_tools_means_no_tool_config() {
        let body = build_body(&[Message::user("selam")], &[], 1024, 0.7);
        assert!(body.get("tools").is_none());
        assert!(body.get("toolConfig").is_none());
    }

    #[test]
    fn text_chunks_are_read_from_the_candidate_parts() {
        let mut s = StreamState::new();
        let out = s.feed(r#"{"candidates":[{"content":{"role":"model","parts":[{"text":"mer"}]}}]}"#);
        assert_eq!(out, vec![Chunk::Text("mer".into())]);
    }

    /// Gemini araç çağrısını tek parçada tam gönderiyor — birleştirme yok.
    #[test]
    fn a_function_call_arrives_complete_in_one_chunk() {
        let mut s = StreamState::new();
        let out = s.feed(
            r#"{"candidates":[{"content":{"parts":[{"functionCall":{"name":"saat","args":{"bolge":"tr"}}}]}}]}"#,
        );
        match &out[0] {
            Chunk::Call(c) => {
                assert_eq!(c.function.name, "saat");
                assert!(c.function.arguments.contains("\"bolge\""), "{}", c.function.arguments);
                assert!(!c.id.is_empty(), "üst katman bir kimlik bekliyor");
            }
            other => panic!("çağrı beklenmişti: {other:?}"),
        }
    }

    /// Aynı isimli iki çağrı ayırt edilebilmeli.
    #[test]
    fn two_calls_to_one_tool_get_distinct_ids() {
        let mut s = StreamState::new();
        let line = r#"{"candidates":[{"content":{"parts":[{"functionCall":{"name":"oku","args":{}}}]}}]}"#;
        let a = s.feed(line);
        let b = s.feed(line);
        let (Chunk::Call(a), Chunk::Call(b)) = (&a[0], &b[0]) else {
            panic!("iki çağrı beklenmişti");
        };
        assert_ne!(a.id, b.id);
    }

    /// Bir bozuk satır akışı öldürmemeli.
    #[test]
    fn a_malformed_line_is_skipped_not_fatal() {
        let mut s = StreamState::new();
        assert_eq!(s.feed("{bozuk"), vec![Chunk::Nothing]);
        assert_eq!(s.feed(""), vec![Chunk::Nothing]);
        // Ve akış bundan sonra çalışmaya devam etmeli.
        assert_eq!(
            s.feed(r#"{"candidates":[{"content":{"parts":[{"text":"ok"}]}}]}"#),
            vec![Chunk::Text("ok".into())]
        );
    }

    /// Bir parçada hem metin hem çağrı olabiliyor.
    #[test]
    fn text_and_a_call_can_share_one_chunk() {
        let mut s = StreamState::new();
        let out = s.feed(
            r#"{"candidates":[{"content":{"parts":[
                {"text":"bakıyorum"},
                {"functionCall":{"name":"ara","args":{}}}]}}]}"#,
        );
        assert_eq!(out.len(), 2, "{out:?}");
        assert!(matches!(out[0], Chunk::Text(_)));
        assert!(matches!(out[1], Chunk::Call(_)));
    }
}
