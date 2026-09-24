//! Bilgisayar kullanımı — fare ve klavye simülasyonu.
//!
//! Asistanın ekrandaki şeylere tıklayıp yazabilmesi. Ekran görüntüsüyle
//! birlikte çalışır: model önce bakar, sonra tıklar.
//!
//! # Neden hepsi yıkıcı sayılıyor
//!
//! Bir tıklama "gönder" düğmesine denk gelebilir, bir tuş dizisi
//! kaydedilmemiş belgeyi kapatabilir. Model ekranı %100 doğru yorumlayamaz;
//! kullanıcı her eylemi görmeli. Eski projede de onay isteniyordu.
//!
//! # Güvenlik sınırı
//!
//! Koordinatlar ekran sınırları içinde olmalı; metin uzunluğu sınırlı.
//! Sonsuz döngüye giren bir model klavyeyi kilitleyemesin.

// The pointer and keyboard helpers are only called from the Windows
// implementations below; elsewhere the platform stubs refuse before
// reaching them, and a build off Windows should not drown in warnings.
#![cfg_attr(not(windows), allow(dead_code))]

use crate::tool::{arg_num, arg_str, Domain, Param, Risk, Tool, ToolOutcome};
use serde_json::Value;

/// Tek seferde yazılabilecek en fazla karakter.
const MAX_TYPE_CHARS: usize = 500;

/// Kaç adımda imleç hedefe götürülür.
///
/// Why the cursor is moved at all rather than teleported: a lot of software
/// only reacts to a pointer that arrives. Hover states never fire, menus
/// never open, and a few applications drop a click that lands in the same
/// frame as the move. A dozen steps costs a tenth of a second and makes the
/// pointer behave like a pointer.
const MOVE_STEPS: u32 = 14;

/// İki adım arası bekleme.
const MOVE_STEP_MS: u64 = 8;

/// Tuş vuruşları arası bekleme.
///
/// Sending five hundred characters in one burst overruns the input handling
/// of a fair amount of software — the first characters land, the rest go
/// nowhere. Typing in chunks with a pause behaves like a keyboard.
const TYPE_CHUNK: usize = 24;
const TYPE_CHUNK_MS: u64 = 40;

/// Fare tıklaması.
pub struct Click;

impl Tool for Click {
    fn name(&self) -> &'static str {
        "click"
    }

    fn description(&self) -> &'static str {
        "Clicks at the given screen coordinate. For anything with a label — a \
         button, menu, tab, link — list_ui_elements and click_element are faster \
         and exact; use this for what has no name. The order is: take_screenshot \
         to look → click → wait_for_screen to confirm."
    }

    fn domain(&self) -> Domain {
        Domain::Vision
    }

    /// Neye tıkladığını kesin bilemeyiz — her zaman onay.
    fn risk(&self) -> Risk {
        Risk::Destructive
    }

    fn params(&self) -> Vec<Param> {
        vec![
            Param::required("x", "Horizontal coordinate, in pixels"),
            Param::required("y", "Vertical coordinate, in pixels"),
            Param::optional("button", "left | right | middle (default: left)"),
            Param::optional("double", "'yes' for a double click"),
        ]
    }

    fn keywords(&self) -> &'static [&'static str] {
        &["tıkla", "click", "bas", "seç"]
    }

    fn run(&self, args: &Value) -> ToolOutcome {
        let (Some(x), Some(y)) = (arg_num(args, "x"), arg_num(args, "y")) else {
            return ToolOutcome::err("x ve y koordinatları gerekli");
        };

        let (w, h) = screen_size();
        if x < 0.0 || y < 0.0 || x > w as f64 || y > h as f64 {
            return ToolOutcome::err(format!(
                "koordinat ekran dışında ({x:.0},{y:.0}) — ekran {w}x{h}"
            ));
        }

        // English is what the schema advertises; the Turkish spellings stay
        // accepted because a model answering in Turkish sometimes fills the
        // field in Turkish, and refusing the click over that helps nobody.
        let button = match arg_str(args, "button").unwrap_or("left") {
            "right" | "sag" | "sağ" => MouseButton::Right,
            "middle" | "orta" => MouseButton::Middle,
            _ => MouseButton::Left,
        };
        let double = arg_str(args, "double")
            .is_some_and(|v| matches!(v.to_lowercase().as_str(), "evet" | "true" | "yes" | "1"));

        click_platform(x as i32, y as i32, button, double)
    }
}

/// Klavyeden metin yazma.
pub struct TypeText;

impl Tool for TypeText {
    fn name(&self) -> &'static str {
        "type_text"
    }

    fn description(&self) -> &'static str {
        "Types text into the currently focused window. Click the right place \
         first to give it focus, then confirm with wait_for_screen that the \
         text actually landed."
    }

    fn domain(&self) -> Domain {
        Domain::Vision
    }

    fn risk(&self) -> Risk {
        Risk::Destructive
    }

    fn params(&self) -> Vec<Param> {
        vec![Param::required("text", "Text to type")]
    }

    fn keywords(&self) -> &'static [&'static str] {
        &["yaz", "klavye", "gir", "type"]
    }

    fn run(&self, args: &Value) -> ToolOutcome {
        let Some(text) = arg_str(args, "text") else {
            return ToolOutcome::err("text is required");
        };
        if text.chars().count() > MAX_TYPE_CHARS {
            return ToolOutcome::err(format!(
                "metin çok uzun ({} karakter) — en fazla {MAX_TYPE_CHARS}",
                text.chars().count()
            ));
        }
        type_platform(text)
    }
}

/// Tuş kombinasyonu.
pub struct PressKey;

impl Tool for PressKey {
    fn name(&self) -> &'static str {
        "press_key"
    }

    fn description(&self) -> &'static str {
        "Presses a key or a key combination, e.g. enter, escape, tab, \
         ctrl+s, ctrl+c, alt+f4, win+d."
    }

    fn domain(&self) -> Domain {
        Domain::Vision
    }

    fn risk(&self) -> Risk {
        Risk::Destructive
    }

    fn params(&self) -> Vec<Param> {
        vec![Param::required("key", "Key name or combination")]
    }

    fn keywords(&self) -> &'static [&'static str] {
        &["tuş", "bas", "enter", "escape", "kısayol"]
    }

    fn run(&self, args: &Value) -> ToolOutcome {
        let Some(key) = arg_str(args, "key") else {
            return ToolOutcome::err("key is required");
        };
        let Some(sequence) = translate_keys(key) else {
            return ToolOutcome::err(format!("'{key}' tanınmayan bir tuş"));
        };
        press_platform(&sequence, key)
    }
}

/// Ekranın durulmasını bekler ve neyin değiştiğini söyler.
///
/// # Neden ayrı bir tool
///
/// Tıklamak yarısı. İnsan tıklar, sonuca **bakar**, ona göre devam eder.
/// Model bunu ekran görüntüsüyle yapabilir ama iki sorun var: pencere daha
/// çizilmeden çekilen görüntü yanıltıyor, ve her adımda tam ekran görüntüsü
/// göndermek bağlamı dolduruyor.
///
/// Bu tool ikisini de çözüyor: ekran durana kadar bekliyor, sonra **bir
/// cümleyle** ne olduğunu söylüyor. Model ancak gerçekten bakması
/// gerekiyorsa görüntü istiyor.
pub struct WaitForScreen;

/// Ekran imzası kaç hücreye bölünüyor.
///
/// Coarse on purpose. A blinking text caret changes a handful of pixels; at
/// this size it lands inside one cell and moves its average barely at all,
/// so a waiting screen actually settles instead of blinking forever.
const SIGNATURE_COLUMNS: usize = 32;
const SIGNATURE_ROWS: usize = 18;

/// Bir hücrenin "değişti" sayılması için gereken parlaklık farkı (0-255).
const CELL_CHANGE_THRESHOLD: i32 = 12;

/// Bu kadar hücre değiştiyse ekran hâlâ hareket ediyor demektir.
const SETTLED_CELLS: usize = 2;

/// En fazla ne kadar beklenir.
const MAX_WAIT_MS: u64 = 8_000;

/// İki imza arası bekleme.
const POLL_MS: u64 = 250;

impl Tool for WaitForScreen {
    fn name(&self) -> &'static str {
        "wait_for_screen"
    }

    fn description(&self) -> &'static str {
        "Waits for the screen to settle after a click or typing, and reports \
         what changed. Use this after every step; ask for a screenshot only \
         when you genuinely need to look."
    }

    fn domain(&self) -> Domain {
        Domain::Vision
    }

    /// Sadece bakıyor — hiçbir şeye dokunmuyor.
    fn risk(&self) -> Risk {
        Risk::Safe
    }

    fn params(&self) -> Vec<Param> {
        vec![Param::optional(
            "seconds",
            "Maximum seconds to wait (default 8)",
        )]
    }

    fn keywords(&self) -> &'static [&'static str] {
        &["bekle", "değişti", "degisti", "yüklendi", "yuklendi"]
    }

    fn run(&self, args: &Value) -> ToolOutcome {
        let budget_ms = arg_num(args, "seconds")
            .map_or(MAX_WAIT_MS, |s| (s * 1000.0).clamp(500.0, 30_000.0) as u64);

        let mut previous = match screen_signature() {
            Ok(signature) => signature,
            Err(e) => return ToolOutcome::err(e),
        };

        let started = std::time::Instant::now();
        let mut moved_at_all = false;

        loop {
            std::thread::sleep(std::time::Duration::from_millis(POLL_MS));

            let current = match screen_signature() {
                Ok(signature) => signature,
                Err(e) => return ToolOutcome::err(e),
            };

            let changed = changed_cells(&previous, &current, CELL_CHANGE_THRESHOLD);
            let elapsed = started.elapsed().as_millis();

            if changed.len() <= SETTLED_CELLS {
                return ToolOutcome::ok(if moved_at_all {
                    format!("Ekran {elapsed} ms sonra duruldu.")
                } else {
                    // Nothing happened. That is information, not a failure:
                    // the click probably missed, and the model should look
                    // rather than carry on as though it worked.
                    format!(
                        "Ekranda değişiklik yok ({elapsed} ms). \
                         Tıklama hedefi ıskalamış olabilir — take_screenshot ile bak."
                    )
                });
            }

            moved_at_all = true;
            previous = current;

            if started.elapsed().as_millis() as u64 >= budget_ms {
                return ToolOutcome::ok(format!(
                    "Ekran {budget_ms} ms sonra hâlâ değişiyor ({} bölge). \
                     Muhtemelen bir animasyon veya video var.",
                    changed.len()
                ));
            }
        }
    }
}

/// Ekranın kaba parlaklık haritası — `SIGNATURE_COLUMNS * SIGNATURE_ROWS` hücre.
type Signature = Vec<u8>;

/// İki imza arasında eşiği aşan hücrelerin indeksleri.
///
/// Pure, so the thing that decides "has anything happened" is testable
/// without a screen.
fn changed_cells(before: &[u8], after: &[u8], threshold: i32) -> Vec<usize> {
    if before.len() != after.len() {
        // Different lengths mean a resolution change, which is certainly a
        // change; reporting every cell says so without pretending to know
        // which ones.
        return (0..after.len()).collect();
    }

    before
        .iter()
        .zip(after)
        .enumerate()
        .filter(|(_, (a, b))| (i32::from(**a) - i32::from(**b)).abs() > threshold)
        .map(|(i, _)| i)
        .collect()
}

/// Ekranı küçültüp gri tonlamalı bir hücre haritası çıkarır.
///
/// The scaling is done by Windows rather than in Rust: decoding a PNG here
/// would mean shipping an image decoder for one gauge, and GDI already has
/// the bitmap in memory.
#[cfg(windows)]
fn screen_signature() -> Result<Signature, String> {
    use super::system::run_powershell;

    let script = format!(
        "Add-Type -AssemblyName System.Windows.Forms, System.Drawing; \
         $b = [System.Windows.Forms.SystemInformation]::VirtualScreen; \
         $full = New-Object System.Drawing.Bitmap($b.Width, $b.Height); \
         $g = [System.Drawing.Graphics]::FromImage($full); \
         $g.CopyFromScreen($b.Left, $b.Top, 0, 0, $full.Size); \
         $small = New-Object System.Drawing.Bitmap($full, {SIGNATURE_COLUMNS}, {SIGNATURE_ROWS}); \
         $out = New-Object System.Text.StringBuilder; \
         for ($y = 0; $y -lt {SIGNATURE_ROWS}; $y++) {{ \
           for ($x = 0; $x -lt {SIGNATURE_COLUMNS}; $x++) {{ \
             $p = $small.GetPixel($x, $y); \
             $v = [int](($p.R * 30 + $p.G * 59 + $p.B * 11) / 100); \
             [void]$out.Append('{{0:x2}}' -f $v) }} }}; \
         $g.Dispose(); $full.Dispose(); $small.Dispose(); \
         $out.ToString()"
    );

    let output = run_powershell(&script).map_err(|e| e.to_string())?;
    parse_signature(output.trim())
}

#[cfg(not(windows))]
fn screen_signature() -> Result<Signature, String> {
    Err("ekran okuma bu platformda desteklenmiyor".into())
}

/// Hex imzayı bayt dizisine çevirir.
fn parse_signature(hex: &str) -> Result<Signature, String> {
    let cleaned: String = hex.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    let expected = SIGNATURE_COLUMNS * SIGNATURE_ROWS;

    if cleaned.len() != expected * 2 {
        return Err(format!(
            "ekran okunamadı ({} hücre bekleniyordu, {} geldi)",
            expected,
            cleaned.len() / 2
        ));
    }

    (0..expected)
        .map(|i| {
            u8::from_str_radix(&cleaned[i * 2..i * 2 + 2], 16)
                .map_err(|_| "ekran imzası bozuk".to_string())
        })
        .collect()
}

/// Ekran boyutu — model koordinat hesaplarken bilmeli.
pub struct ScreenSize;

impl Tool for ScreenSize {
    fn name(&self) -> &'static str {
        "get_screen_size"
    }

    fn description(&self) -> &'static str {
        "Returns the screen resolution. Use before working out click coordinates."
    }

    fn domain(&self) -> Domain {
        Domain::Vision
    }

    fn keywords(&self) -> &'static [&'static str] {
        &["ekran", "çözünürlük", "boyut"]
    }

    fn run(&self, _args: &Value) -> ToolOutcome {
        let (w, h) = screen_size();
        ToolOutcome::ok(format!("Ekran: {w}x{h} piksel"))
    }
}

/// A left click at a point, for tools that found the point some other way
/// (UI Automation reports a control's centre).
pub(crate) fn click_at(x: i32, y: i32) -> ToolOutcome {
    let (w, h) = screen_size();
    if x < 0 || y < 0 || x > w || y > h {
        return ToolOutcome::err(format!("koordinat ekran dışında ({x},{y}) — ekran {w}x{h}"));
    }
    click_platform(x, y, MouseButton::Left, false)
}

/// Types into whatever has focus, as `type_text` does.
pub(crate) fn type_text(text: &str) -> ToolOutcome {
    if text.chars().count() > MAX_TYPE_CHARS {
        return ToolOutcome::err(format!(
            "metin çok uzun (en fazla {MAX_TYPE_CHARS} karakter)"
        ));
    }
    type_platform(text)
}

/// Mouse-wheel notches, optionally after moving to a point.
pub(crate) fn scroll(
    direction: super::uia::ScrollDir,
    notches: i32,
    at: Option<(i32, i32)>,
) -> ToolOutcome {
    if let Some((x, y)) = at {
        let (w, h) = screen_size();
        if x < 0 || y < 0 || x > w || y > h {
            return ToolOutcome::err(format!("koordinat ekran dışında ({x},{y}) — ekran {w}x{h}"));
        }
    }
    scroll_platform(direction, notches, at)
}

/// Press at `from`, travel, release at `to`.
pub(crate) fn drag(from: (i32, i32), to: (i32, i32)) -> ToolOutcome {
    let (w, h) = screen_size();
    for (x, y) in [from, to] {
        if x < 0 || y < 0 || x > w || y > h {
            return ToolOutcome::err(format!("koordinat ekran dışında ({x},{y}) — ekran {w}x{h}"));
        }
    }
    drag_platform(from, to)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MouseButton {
    Left,
    Right,
    Middle,
}

/// İmlecin `from`'dan `to`'ya izleyeceği nokta dizisi.
///
/// Eased rather than linear — fast in the middle, slowing at the end — which
/// is both how a hand moves and what gives slow software a moment to notice
/// the pointer before the click lands. The last point is always exactly the
/// target: an eased path that stopped a pixel short would click the wrong
/// thing, which is the one mistake that matters here.
fn ease_path(from: (i32, i32), to: (i32, i32), steps: u32) -> Vec<(i32, i32)> {
    if steps == 0 || from == to {
        return vec![to];
    }

    (1..=steps)
        .map(|i| {
            let t = f64::from(i) / f64::from(steps);
            // Smoothstep: zero velocity at both ends.
            let eased = t * t * (3.0 - 2.0 * t);
            let x = f64::from(from.0) + (f64::from(to.0 - from.0) * eased);
            let y = f64::from(from.1) + (f64::from(to.1 - from.1) * eased);
            (x.round() as i32, y.round() as i32)
        })
        // Rounding can repeat a point on a short move; sending the same
        // position twice is wasted time, not a smoother path.
        .fold(Vec::with_capacity(steps as usize), |mut acc, point| {
            if acc.last() != Some(&point) {
                acc.push(point);
            }
            acc
        })
}

/// Metni klavye hızında parçalara böler.
fn type_chunks(text: &str, size: usize) -> Vec<String> {
    if size == 0 {
        return vec![text.to_string()];
    }
    let chars: Vec<char> = text.chars().collect();
    chars
        .chunks(size)
        .map(|chunk| chunk.iter().collect())
        .collect()
}

/// Kullanıcı tuş adını SendKeys biçimine çevirir.
///
/// `None` dönerse tuş tanınmıyor — körlemesine göndermek yerine hata veriyoruz
/// (tanınmayan metin ekrana harf harf yazılabilirdi).
fn translate_keys(input: &str) -> Option<String> {
    let lower = input.trim().to_lowercase();

    // Kombinasyon mu?
    if lower.contains('+') {
        let mut modifiers = String::new();
        let mut key = None;

        for part in lower.split('+') {
            match part.trim() {
                "ctrl" | "control" => modifiers.push('^'),
                "alt" => modifiers.push('%'),
                "shift" => modifiers.push('+'),
                "win" | "windows" => modifiers.push_str("^{ESC}"), // yaklaşık
                other => key = Some(single_key(other)?),
            }
        }
        let key = key?;
        return Some(format!("{modifiers}{key}"));
    }

    single_key(&lower)
}

/// Tek tuşun SendKeys karşılığı.
fn single_key(name: &str) -> Option<String> {
    let key = match name {
        "enter" | "return" | "giriş" => "{ENTER}",
        "escape" | "esc" | "iptal" => "{ESC}",
        "tab" | "sekme" => "{TAB}",
        "space" | "bosluk" | "boşluk" => " ",
        "backspace" | "geri" => "{BACKSPACE}",
        "delete" | "del" | "sil" => "{DELETE}",
        "up" | "yukari" | "yukarı" => "{UP}",
        "down" | "asagi" | "aşağı" => "{DOWN}",
        "left" | "sol" => "{LEFT}",
        "right" | "sag" | "sağ" => "{RIGHT}",
        "home" => "{HOME}",
        "end" | "son" => "{END}",
        "pageup" | "pgup" => "{PGUP}",
        "pagedown" | "pgdn" => "{PGDN}",
        "f1" => "{F1}",
        "f2" => "{F2}",
        "f3" => "{F3}",
        "f4" => "{F4}",
        "f5" => "{F5}",
        "f11" => "{F11}",
        "f12" => "{F12}",
        // Tek harf/rakam.
        s if s.len() == 1 && s.chars().all(|c| c.is_ascii_alphanumeric()) => {
            return Some(s.to_string())
        }
        _ => return None,
    };
    Some(key.to_string())
}

/// SendKeys için metin kaçırma.
///
/// `+ ^ % ~ ( ) { } [ ]` SendKeys'te özel anlam taşır; kaçırılmazsa
/// metin yerine tuş kombinasyonu olarak yorumlanır.
fn escape_sendkeys(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 2);
    for c in text.chars() {
        match c {
            '+' | '^' | '%' | '~' | '(' | ')' | '{' | '}' | '[' | ']' => {
                out.push('{');
                out.push(c);
                out.push('}');
            }
            // A bare line feed goes out as Ctrl+Enter -- which sends the
            // message in Teams, Slack or Outlook halfway through typing it.
            // A new line is the Enter key; the carriage return of a CRLF
            // pair would press it twice.
            '\n' => out.push_str("{ENTER}"),
            '\r' => {}
            '\t' => out.push_str("{TAB}"),
            c => out.push(c),
        }
    }
    out
}

#[cfg(windows)]
fn screen_size() -> (i32, i32) {
    #[link(name = "user32")]
    extern "system" {
        fn GetSystemMetrics(index: i32) -> i32;
    }
    const SM_CXSCREEN: i32 = 0;
    const SM_CYSCREEN: i32 = 1;

    // SAFETY: salt okunur sistem sorgusu, parametresi sabit.
    unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) }
}

#[cfg(not(windows))]
fn screen_size() -> (i32, i32) {
    (1920, 1080)
}

#[cfg(windows)]
fn click_platform(x: i32, y: i32, button: MouseButton, double: bool) -> ToolOutcome {
    #[link(name = "user32")]
    extern "system" {
        fn SetCursorPos(x: i32, y: i32) -> i32;
        fn GetCursorPos(point: *mut [i32; 2]) -> i32;
        fn mouse_event(flags: u32, dx: u32, dy: u32, data: u32, extra: usize);
    }

    const MOUSEEVENTF_LEFTDOWN: u32 = 0x0002;
    const MOUSEEVENTF_LEFTUP: u32 = 0x0004;
    const MOUSEEVENTF_RIGHTDOWN: u32 = 0x0008;
    const MOUSEEVENTF_RIGHTUP: u32 = 0x0010;
    const MOUSEEVENTF_MIDDLEDOWN: u32 = 0x0020;
    const MOUSEEVENTF_MIDDLEUP: u32 = 0x0040;

    let (down, up) = match button {
        MouseButton::Left => (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP),
        MouseButton::Right => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP),
        MouseButton::Middle => (MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP),
    };

    // SAFETY: koordinatlar doğrulandı; API'ler yan etkisi bilinen sistem çağrıları.
    unsafe {
        // Where the pointer is now, so it can be moved rather than teleported.
        let mut current = [0i32; 2];
        let from = if GetCursorPos(&mut current) != 0 {
            (current[0], current[1])
        } else {
            (x, y)
        };

        for (px, py) in ease_path(from, (x, y), MOVE_STEPS) {
            SetCursorPos(px, py);
            std::thread::sleep(std::time::Duration::from_millis(MOVE_STEP_MS));
        }

        // İmlecin yerleşmesi için kısa bekleme — bazı uygulamalar
        // anında gelen tıklamayı kaçırıyor.
        std::thread::sleep(std::time::Duration::from_millis(60));

        let clicks = if double { 2 } else { 1 };
        for i in 0..clicks {
            mouse_event(down, 0, 0, 0, 0);
            mouse_event(up, 0, 0, 0, 0);
            if i == 0 && double {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
    }

    let kind = if double {
        "çift tıklandı"
    } else {
        "tıklandı"
    };
    ToolOutcome::ok(format!("({x},{y}) noktasına {kind}"))
}

#[cfg(not(windows))]
fn click_platform(_x: i32, _y: i32, _b: MouseButton, _d: bool) -> ToolOutcome {
    ToolOutcome::err("fare kontrolü bu platformda desteklenmiyor")
}

#[cfg(windows)]
fn scroll_platform(
    direction: super::uia::ScrollDir,
    notches: i32,
    at: Option<(i32, i32)>,
) -> ToolOutcome {
    use super::uia::ScrollDir;

    #[link(name = "user32")]
    extern "system" {
        fn SetCursorPos(x: i32, y: i32) -> i32;
        fn mouse_event(flags: u32, dx: u32, dy: u32, data: u32, extra: usize);
    }
    const MOUSEEVENTF_WHEEL: u32 = 0x0800;
    const MOUSEEVENTF_HWHEEL: u32 = 0x1000;
    /// One notch, as Windows counts it.
    const WHEEL_DELTA: i32 = 120;

    let (flag, sign) = match direction {
        ScrollDir::Up => (MOUSEEVENTF_WHEEL, 1),
        ScrollDir::Down => (MOUSEEVENTF_WHEEL, -1),
        ScrollDir::Right => (MOUSEEVENTF_HWHEEL, 1),
        ScrollDir::Left => (MOUSEEVENTF_HWHEEL, -1),
    };

    // SAFETY: coordinates were checked against the screen; these are the
    // same documented calls the click tool uses.
    unsafe {
        if let Some((x, y)) = at {
            SetCursorPos(x, y);
            std::thread::sleep(std::time::Duration::from_millis(40));
        }
        // One notch at a time with a pause: a single large delta is often
        // collapsed into one step by the receiving application.
        for _ in 0..notches {
            mouse_event(flag, 0, 0, (sign * WHEEL_DELTA) as u32, 0);
            std::thread::sleep(std::time::Duration::from_millis(30));
        }
    }
    let way = match direction {
        ScrollDir::Up => "yukarı",
        ScrollDir::Down => "aşağı",
        ScrollDir::Left => "sola",
        ScrollDir::Right => "sağa",
    };
    ToolOutcome::ok(format!("{notches} çentik {way} kaydırıldı"))
}

#[cfg(not(windows))]
fn scroll_platform(
    _direction: super::uia::ScrollDir,
    _notches: i32,
    _at: Option<(i32, i32)>,
) -> ToolOutcome {
    ToolOutcome::err("fare kontrolü bu platformda desteklenmiyor")
}

#[cfg(windows)]
fn drag_platform(from: (i32, i32), to: (i32, i32)) -> ToolOutcome {
    #[link(name = "user32")]
    extern "system" {
        fn SetCursorPos(x: i32, y: i32) -> i32;
        fn mouse_event(flags: u32, dx: u32, dy: u32, data: u32, extra: usize);
    }
    const MOUSEEVENTF_LEFTDOWN: u32 = 0x0002;
    const MOUSEEVENTF_LEFTUP: u32 = 0x0004;

    // SAFETY: both points were checked against the screen.
    unsafe {
        SetCursorPos(from.0, from.1);
        std::thread::sleep(std::time::Duration::from_millis(60));
        mouse_event(MOUSEEVENTF_LEFTDOWN, 0, 0, 0, 0);
        // A drag that jumps straight to its end is read as a click by most
        // software: the drag threshold is never crossed while the button is
        // down. The eased path crosses it the way a hand does.
        std::thread::sleep(std::time::Duration::from_millis(80));
        for (x, y) in ease_path(from, to, MOVE_STEPS * 2) {
            SetCursorPos(x, y);
            std::thread::sleep(std::time::Duration::from_millis(MOVE_STEP_MS * 2));
        }
        std::thread::sleep(std::time::Duration::from_millis(80));
        mouse_event(MOUSEEVENTF_LEFTUP, 0, 0, 0, 0);
    }
    ToolOutcome::ok(format!(
        "({},{}) noktasından ({},{}) noktasına sürüklendi",
        from.0, from.1, to.0, to.1
    ))
}

#[cfg(not(windows))]
fn drag_platform(_from: (i32, i32), _to: (i32, i32)) -> ToolOutcome {
    ToolOutcome::err("fare kontrolü bu platformda desteklenmiyor")
}

#[cfg(windows)]
fn type_platform(text: &str) -> ToolOutcome {
    use super::system::run_powershell;

    // One PowerShell process, but the keystrokes inside it are paced. Starting
    // a process per chunk would take longer than the typing.
    let sends: Vec<String> = type_chunks(text, TYPE_CHUNK)
        .iter()
        .map(|chunk| {
            let keys = vavis_core::process::ps_quote(&escape_sendkeys(chunk));
            format!(
                "[System.Windows.Forms.SendKeys]::SendWait({keys}); \
                 Start-Sleep -Milliseconds {TYPE_CHUNK_MS}"
            )
        })
        .collect();

    let script = format!(
        "Add-Type -AssemblyName System.Windows.Forms; {}",
        sends.join("; ")
    );

    match run_powershell(&script) {
        Ok(_) => ToolOutcome::ok(format!("yazıldı ({} karakter)", text.chars().count())),
        Err(e) => ToolOutcome::err(format!("yazılamadı: {e}")),
    }
}

#[cfg(not(windows))]
fn type_platform(_text: &str) -> ToolOutcome {
    ToolOutcome::err("klavye kontrolü bu platformda desteklenmiyor")
}

#[cfg(windows)]
fn press_platform(sequence: &str, label: &str) -> ToolOutcome {
    use super::system::run_powershell;

    let keys = vavis_core::process::ps_quote(sequence);
    let script = format!(
        "Add-Type -AssemblyName System.Windows.Forms; \
         [System.Windows.Forms.SendKeys]::SendWait({keys})"
    );

    match run_powershell(&script) {
        Ok(_) => ToolOutcome::ok(format!("{label} tuşuna basıldı")),
        Err(e) => ToolOutcome::err(format!("tuşa basılamadı: {e}")),
    }
}

#[cfg(not(windows))]
fn press_platform(_sequence: &str, _label: &str) -> ToolOutcome {
    ToolOutcome::err("klavye kontrolü bu platformda desteklenmiyor")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn click_requires_both_coordinates() {
        assert!(!Click.run(&serde_json::json!({})).ok);
        assert!(!Click.run(&serde_json::json!({"x": 100})).ok);
        assert!(!Click.run(&serde_json::json!({"y": 100})).ok);
    }

    #[test]
    fn click_rejects_off_screen_coordinates() {
        // Ekran dışına tıklamak anlamsız; model halüsinasyon görmüş olabilir.
        for (x, y) in [(-10.0, 100.0), (100.0, -5.0), (99999.0, 100.0)] {
            let args = serde_json::json!({"x": x, "y": y});
            let out = Click.run(&args);
            assert!(!out.ok, "({x},{y}) reddedilmeliydi");
            assert!(out.content.contains("ekran dışında"));
        }
    }

    #[test]
    fn click_accepts_string_coordinates() {
        // Model sayıyı string gönderebilir.
        let args = serde_json::json!({"x": "100", "y": "100"});
        assert_eq!(arg_num(&args, "x"), Some(100.0));
    }

    #[test]
    fn typing_rejects_overly_long_text() {
        let long = "a".repeat(MAX_TYPE_CHARS + 1);
        let args = serde_json::json!({"text": long});
        let out = TypeText.run(&args);
        assert!(!out.ok);
        assert!(out.content.contains("çok uzun"));
    }

    #[test]
    fn typing_requires_text() {
        assert!(!TypeText.run(&serde_json::json!({})).ok);
    }

    #[test]
    fn all_input_tools_are_destructive() {
        // Model ekranı yanlış yorumlayabilir — kullanıcı her eylemi görmeli.
        assert_eq!(Click.risk(), Risk::Destructive);
        assert_eq!(TypeText.risk(), Risk::Destructive);
        assert_eq!(PressKey.risk(), Risk::Destructive);
        // Boyut sorgusu güvenli.
        assert_eq!(ScreenSize.risk(), Risk::Safe);
    }

    #[test]
    fn screen_size_returns_positive_dimensions() {
        let out = ScreenSize.run(&Value::Null);
        assert!(out.ok);
        assert!(out.content.contains('x'));

        let (w, h) = screen_size();
        assert!(w > 0 && h > 0, "ekran boyutu {w}x{h}");
    }

    // ── Tuş çevirisi ─────────────────────────────────────────────────────

    #[test]
    fn common_keys_translate() {
        assert_eq!(translate_keys("enter").as_deref(), Some("{ENTER}"));
        assert_eq!(translate_keys("ESC").as_deref(), Some("{ESC}"));
        assert_eq!(translate_keys("tab").as_deref(), Some("{TAB}"));
    }

    #[test]
    fn turkish_key_names_work() {
        assert_eq!(translate_keys("yukarı").as_deref(), Some("{UP}"));
        assert_eq!(translate_keys("aşağı").as_deref(), Some("{DOWN}"));
        assert_eq!(translate_keys("sil").as_deref(), Some("{DELETE}"));
    }

    #[test]
    fn modifier_combinations_translate() {
        assert_eq!(translate_keys("ctrl+s").as_deref(), Some("^s"));
        assert_eq!(translate_keys("alt+f4").as_deref(), Some("%{F4}"));
        assert_eq!(translate_keys("ctrl+shift+n").as_deref(), Some("^+n"));
    }

    #[test]
    fn unknown_keys_are_rejected_not_typed_literally() {
        // Tanınmayan tuş adını ekrana yazmak yerine hata dönmeli.
        assert!(translate_keys("bilinmeyen_tus").is_none());
        assert!(translate_keys("").is_none());

        let out = PressKey.run(&serde_json::json!({"key": "saçmalık"}));
        assert!(!out.ok);
        assert!(out.content.contains("tanınmayan"));
    }

    #[test]
    fn single_letters_and_digits_pass_through() {
        assert_eq!(translate_keys("a").as_deref(), Some("a"));
        assert_eq!(translate_keys("5").as_deref(), Some("5"));
    }

    // ── SendKeys kaçırma ─────────────────────────────────────────────────

    #[test]
    fn sendkeys_special_characters_are_escaped() {
        // Kaçırılmazsa "+" shift, "^" ctrl olarak yorumlanır.
        assert_eq!(escape_sendkeys("a+b"), "a{+}b");
        assert_eq!(escape_sendkeys("100%"), "100{%}");
        assert_eq!(escape_sendkeys("f(x)"), "f{(}x{)}");
        assert_eq!(escape_sendkeys("a^b~c"), "a{^}b{~}c");
    }

    #[test]
    fn a_new_line_is_the_enter_key_not_ctrl_enter() {
        assert_eq!(escape_sendkeys("a\nb"), "a{ENTER}b");
        assert_eq!(escape_sendkeys("a\r\nb"), "a{ENTER}b");
        assert_eq!(escape_sendkeys("a\tb"), "a{TAB}b");
    }

    #[test]
    fn ordinary_text_survives_escaping() {
        let text = "merhaba dünya 123";
        assert_eq!(escape_sendkeys(text), text);
    }

    #[test]
    fn turkish_characters_survive_escaping() {
        let text = "çğıöşü ÇĞİÖŞÜ";
        assert_eq!(escape_sendkeys(text), text);
    }

    #[test]
    fn descriptions_tell_the_model_to_look_first() {
        // Koordinat uydurmasın, önce ekrana baksın.
        assert!(Click.description().contains("take_screenshot"));
        assert!(TypeText.description().contains("Click"));
    }
}

#[cfg(test)]
mod human_tests {
    use super::*;

    #[test]
    fn the_cursor_path_ends_exactly_on_target() {
        let path = ease_path((0, 0), (100, 50), MOVE_STEPS);
        // Stopping a pixel short would click the wrong thing, which is the
        // one mistake here that actually matters.
        assert_eq!(*path.last().unwrap(), (100, 50));
    }

    #[test]
    fn the_cursor_path_is_eased_not_linear() {
        let path = ease_path((0, 0), (100, 0), 10);
        let midpoint = path[path.len() / 2 - 1].0;
        // Halfway through the steps, an eased move is near the middle; the
        // difference from linear shows at the quarter points.
        let quarter = path[1].0;
        assert!(quarter < 25, "start should be slow, got {quarter}");
        assert!((40..=60).contains(&midpoint), "midpoint {midpoint}");
    }

    #[test]
    fn a_move_to_where_the_cursor_already_is_costs_one_step() {
        assert_eq!(ease_path((7, 7), (7, 7), MOVE_STEPS), vec![(7, 7)]);
    }

    #[test]
    fn a_one_pixel_move_does_not_repeat_the_same_point() {
        let path = ease_path((0, 0), (1, 0), MOVE_STEPS);
        // Rounding would otherwise send (0,0) a dozen times before (1,0).
        let mut deduped = path.clone();
        deduped.dedup();
        assert_eq!(path, deduped);
        assert_eq!(*path.last().unwrap(), (1, 0));
    }

    #[test]
    fn zero_steps_still_reaches_the_target() {
        assert_eq!(ease_path((0, 0), (5, 5), 0), vec![(5, 5)]);
    }

    #[test]
    fn typing_is_split_into_keyboard_sized_pieces() {
        let chunks = type_chunks(&"a".repeat(50), 24);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].len(), 24);
        assert_eq!(chunks[2].len(), 2);
    }

    #[test]
    fn typing_splits_on_characters_not_bytes() {
        // Turkish is the common case; splitting mid-character would send
        // broken text to the keyboard.
        let text = "çğıöşü".repeat(6);
        let chunks = type_chunks(&text, 4);
        assert_eq!(chunks.concat(), text);
        assert!(chunks.iter().all(|c| c.chars().count() <= 4));
    }

    #[test]
    fn short_text_is_one_chunk() {
        assert_eq!(type_chunks("hello", 24), vec!["hello"]);
        assert_eq!(type_chunks("", 24), Vec::<String>::new());
    }

    #[test]
    fn an_unchanged_screen_reports_nothing_changed() {
        let screen = vec![100u8; SIGNATURE_COLUMNS * SIGNATURE_ROWS];
        assert!(changed_cells(&screen, &screen, CELL_CHANGE_THRESHOLD).is_empty());
    }

    #[test]
    fn a_blinking_caret_does_not_count_as_movement() {
        // One cell shifting slightly is what a text cursor looks like at this
        // resolution. Counting it would mean the screen never settles.
        let before = vec![100u8; SIGNATURE_COLUMNS * SIGNATURE_ROWS];
        let mut after = before.clone();
        after[200] = 108;

        assert!(changed_cells(&before, &after, CELL_CHANGE_THRESHOLD).is_empty());
    }

    #[test]
    fn a_window_opening_counts_as_movement() {
        let before = vec![100u8; SIGNATURE_COLUMNS * SIGNATURE_ROWS];
        let mut after = before.clone();
        for cell in after.iter_mut().take(80) {
            *cell = 240;
        }

        let changed = changed_cells(&before, &after, CELL_CHANGE_THRESHOLD);
        assert_eq!(changed.len(), 80);
        assert!(changed.len() > SETTLED_CELLS);
    }

    #[test]
    fn a_resolution_change_is_reported_as_a_full_change() {
        let before = vec![0u8; 10];
        let after = vec![0u8; 20];
        assert_eq!(
            changed_cells(&before, &after, CELL_CHANGE_THRESHOLD).len(),
            20
        );
    }

    #[test]
    fn a_signature_round_trips_through_hex() {
        let cells: Vec<u8> = (0..SIGNATURE_COLUMNS * SIGNATURE_ROWS)
            .map(|i| (i % 256) as u8)
            .collect();
        let hex: String = cells.iter().map(|c| format!("{c:02x}")).collect();

        assert_eq!(parse_signature(&hex).unwrap(), cells);
    }

    #[test]
    fn a_signature_survives_the_newlines_powershell_adds() {
        let cells = vec![0xABu8; SIGNATURE_COLUMNS * SIGNATURE_ROWS];
        let hex: String = cells.iter().map(|c| format!("{c:02x}")).collect();
        let wrapped = format!("\r\n{}\r\n  ", hex);

        assert_eq!(parse_signature(&wrapped).unwrap(), cells);
    }

    #[test]
    fn a_short_signature_is_an_error_not_a_wrong_answer() {
        // A truncated read compared against a full one would report the
        // screen as entirely changed, forever.
        assert!(parse_signature("aabb").is_err());
        assert!(parse_signature("").is_err());
    }

    #[test]
    fn waiting_only_looks_and_so_needs_no_approval() {
        assert_eq!(WaitForScreen.risk(), Risk::Safe);
    }
}

#[cfg(test)]
mod live_screen_tests {
    use super::*;

    /// Reads the real screen. Ignored by default — needs a desktop session.
    ///
    /// ```text
    /// cargo test -p vavis-tools live_screen -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "needs a real desktop session"]
    fn live_screen_signature_has_the_expected_shape() {
        let signature = screen_signature().expect("the screen should be readable");
        assert_eq!(signature.len(), SIGNATURE_COLUMNS * SIGNATURE_ROWS);

        // An all-identical map means the scaling silently produced nothing.
        let distinct = signature.iter().collect::<std::collections::HashSet<_>>();
        assert!(distinct.len() > 1, "the screen came back uniform");
        println!(
            "{} cells, {} distinct values",
            signature.len(),
            distinct.len()
        );
    }

    #[test]
    #[ignore = "needs a real desktop session"]
    fn live_screen_settles_when_nothing_is_happening() {
        let out = WaitForScreen.run(&serde_json::json!({ "seconds": 4 }));
        println!("{}", out.content);
        assert!(out.ok, "{}", out.content);
    }
}
