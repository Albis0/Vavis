//! Sistem tool'ları — donanım durumu ve kontrol.

use crate::tool::{arg_num, Domain, Param, Risk, Tool, ToolOutcome};
use serde_json::Value;
use std::sync::Mutex;
use sysinfo::System;

/// `System` nesnesi pahalı kurulur ve her sorguda yenilenmesi yeterli —
/// her çağrıda sıfırdan yaratmak yavaş ve ilk CPU okuması hep 0 çıkar.
static SYSTEM: Mutex<Option<System>> = Mutex::new(None);

fn with_system<T>(f: impl FnOnce(&mut System) -> T) -> T {
    let mut guard = SYSTEM.lock().unwrap_or_else(|e| e.into_inner());
    let sys = guard.get_or_insert_with(System::new_all);
    sys.refresh_all();
    f(sys)
}

/// Refreshes only what the CPU reading needs.
///
/// [`with_system`] calls `refresh_all`, which walks every process, disk and
/// network interface on the machine. Measured at ~14 ms. That is fine for
/// the tool a user invokes now and then, and much too slow for the status
/// poll, which runs **once a second** and holds the locks the settings
/// screen wants -- which is what made moving between settings categories
/// stutter.
fn with_cpu<T>(f: impl FnOnce(&mut System) -> T) -> T {
    let mut guard = SYSTEM.lock().unwrap_or_else(|e| e.into_inner());
    let sys = guard.get_or_insert_with(System::new_all);
    sys.refresh_cpu_usage();
    f(sys)
}

/// Sistem telemetrisi — CPU, RAM, disk.
pub struct SystemInfo;

impl Tool for SystemInfo {
    fn name(&self) -> &'static str {
        "get_system_status"
    }

    fn description(&self) -> &'static str {
        "Current machine status: CPU usage, RAM and disk. Use when performance is asked about."
    }

    fn domain(&self) -> Domain {
        Domain::System
    }

    fn keywords(&self) -> &'static [&'static str] {
        &[
            "cpu",
            "ram",
            "bellek",
            "disk",
            "performans",
            "sistem",
            "durum",
        ]
    }

    fn run(&self, _args: &Value) -> ToolOutcome {
        let (cpu, used_mb, total_mb, cores) = with_system(|sys| {
            (
                sys.global_cpu_usage(),
                sys.used_memory() / 1_048_576,
                sys.total_memory() / 1_048_576,
                sys.cpus().len(),
            )
        });

        let ram_pct = if total_mb > 0 {
            (used_mb as f64 / total_mb as f64 * 100.0).round()
        } else {
            0.0
        };

        ToolOutcome::ok(format!(
            "CPU: %{:.0} ({cores} çekirdek)\nRAM: {used_mb} MB / {total_mb} MB (%{ram_pct:.0})",
            cpu
        ))
    }
}

/// Çalışan uygulamalar.
pub struct ListProcesses;

impl Tool for ListProcesses {
    fn name(&self) -> &'static str {
        "list_processes"
    }

    fn description(&self) -> &'static str {
        "Lists the processes using the most resources."
    }

    fn domain(&self) -> Domain {
        Domain::System
    }

    fn keywords(&self) -> &'static [&'static str] {
        &["uygulama", "program", "süreç", "process", "çalışan"]
    }

    fn run(&self, _args: &Value) -> ToolOutcome {
        let mut rows: Vec<(String, u64)> = with_system(|sys| {
            sys.processes()
                .values()
                .map(|p| (p.name().to_string_lossy().to_string(), p.memory()))
                .collect()
        });

        // Bellek kullanımına göre sırala, ilk 10'u göster — tam liste
        // yüzlerce satır olur ve modelin bağlamını boğar.
        rows.sort_by_key(|row| std::cmp::Reverse(row.1));
        rows.truncate(10);

        if rows.is_empty() {
            return ToolOutcome::err("süreç listesi alınamadı");
        }

        let list = rows
            .into_iter()
            .map(|(name, mem)| format!("{name} — {} MB", mem / 1_048_576))
            .collect::<Vec<_>>()
            .join("\n");

        ToolOutcome::ok(format!("En çok bellek kullananlar:\n{list}"))
    }
}

/// Pil durumu.
pub struct Battery;

impl Tool for Battery {
    fn name(&self) -> &'static str {
        "get_battery"
    }

    fn description(&self) -> &'static str {
        "Battery percentage and charging status."
    }

    fn domain(&self) -> Domain {
        Domain::System
    }

    fn keywords(&self) -> &'static [&'static str] {
        &["pil", "batarya", "şarj", "battery"]
    }

    fn run(&self, _args: &Value) -> ToolOutcome {
        #[cfg(windows)]
        {
            match windows_battery() {
                Some(text) => ToolOutcome::ok(text),
                None => ToolOutcome::err("pil bilgisi okunamadı (masaüstü olabilir)"),
            }
        }
        #[cfg(not(windows))]
        {
            ToolOutcome::err("pil bilgisi bu platformda desteklenmiyor")
        }
    }
}

#[cfg(windows)]
fn windows_battery() -> Option<String> {
    #[repr(C)]
    struct SystemPowerStatus {
        ac_line_status: u8,
        battery_flag: u8,
        battery_life_percent: u8,
        system_status_flag: u8,
        battery_life_time: u32,
        battery_full_life_time: u32,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GetSystemPowerStatus(status: *mut SystemPowerStatus) -> i32;
    }

    let mut status = SystemPowerStatus {
        ac_line_status: 0,
        battery_flag: 0,
        battery_life_percent: 0,
        system_status_flag: 0,
        battery_life_time: 0,
        battery_full_life_time: 0,
    };

    // SAFETY: geçerli bir yapıya yazıyoruz; API sadece doldurur.
    if unsafe { GetSystemPowerStatus(&mut status) } == 0 {
        return None;
    }

    // 255 = bilinmiyor (pil yok).
    if status.battery_life_percent == 255 {
        return Some("Pil yok (masaüstü) — prize takılı".to_string());
    }

    let sarj = match status.ac_line_status {
        1 => "şarjda",
        _ => "pilde",
    };
    Some(format!("Pil: %{} ({sarj})", status.battery_life_percent))
}

/// Ses seviyesi ayarı.
pub struct SetVolume;

impl Tool for SetVolume {
    fn name(&self) -> &'static str {
        "set_volume"
    }

    fn description(&self) -> &'static str {
        "Sets the system volume, 0 to 100."
    }

    fn domain(&self) -> Domain {
        Domain::Control
    }

    /// Geri alınabilir ama kullanıcıyı şaşırtabilir → onay ister.
    fn risk(&self) -> Risk {
        Risk::Moderate
    }

    fn params(&self) -> Vec<Param> {
        vec![Param::required("level", "Volume level between 0 and 100")]
    }

    fn keywords(&self) -> &'static [&'static str] {
        &["ses", "volume", "sessiz", "kıs", "aç"]
    }

    fn run(&self, args: &Value) -> ToolOutcome {
        let Some(level) = arg_num(args, "level") else {
            return ToolOutcome::err("level is required (0-100)");
        };
        if !(0.0..=100.0).contains(&level) {
            return ToolOutcome::err("seviye 0-100 arasında olmalı");
        }
        set_system_volume(level as u8)
    }
}

#[cfg(windows)]
fn set_system_volume(level: u8) -> ToolOutcome {
    // Windows'ta ses ayarı COM (IAudioEndpointVolume) gerektirir. Bağımlılık
    // eklemeden PowerShell üzerinden yapmak güvenilir ve kısa.
    let script = format!(
        "$vol = {level} / 100; \
         Add-Type -TypeDefinition 'using System.Runtime.InteropServices; \
         [Guid(\"5CDF2C82-841E-4546-9722-0CF74078229A\"), \
         InterfaceType(ComInterfaceType.InterfaceIsIUnknown)] \
         interface IAudioEndpointVolume {{ \
           int f(); int g(); int h(); int i(); \
           int SetMasterVolumeLevelScalar(float fLevel, System.Guid pguidEventContext); \
           int j(); int GetMasterVolumeLevelScalar(out float pfLevel); }} \
         [Guid(\"D666063F-1587-4E43-81F1-B948E807363F\"), \
         InterfaceType(ComInterfaceType.InterfaceIsIUnknown)] \
         interface IMMDevice {{ int Activate(ref System.Guid id, int clsCtx, System.IntPtr p, \
           [MarshalAs(UnmanagedType.IUnknown)] out object o); }} \
         [Guid(\"A95664D2-9614-4F35-A746-DE8DB63617E6\"), \
         InterfaceType(ComInterfaceType.InterfaceIsIUnknown)] \
         interface IMMDeviceEnumerator {{ int f(); \
           int GetDefaultAudioEndpoint(int dataFlow, int role, out IMMDevice endpoint); }} \
         [ComImport, Guid(\"BCDE0395-E52F-467C-8E3D-C4579291692E\")] class MMDeviceEnumeratorComObject {{ }} \
         public class Audio {{ \
           public static void Set(float v) {{ \
             var e = (IMMDeviceEnumerator)(new MMDeviceEnumeratorComObject()); \
             IMMDevice dev; e.GetDefaultAudioEndpoint(0, 1, out dev); \
             var g = typeof(IAudioEndpointVolume).GUID; object o; \
             dev.Activate(ref g, 23, System.IntPtr.Zero, out o); \
             ((IAudioEndpointVolume)o).SetMasterVolumeLevelScalar(v, System.Guid.Empty); }} }}'; \
         [Audio]::Set($vol)"
    );

    match run_powershell(&script) {
        Ok(_) => ToolOutcome::ok(format!("Ses seviyesi %{level} yapıldı")),
        Err(e) => ToolOutcome::err(format!("ses ayarlanamadı: {e}")),
    }
}

#[cfg(not(windows))]
fn set_system_volume(_level: u8) -> ToolOutcome {
    ToolOutcome::err("ses ayarı bu platformda desteklenmiyor")
}

/// PowerShell komutu çalıştırır ve çıktısını döner.
#[cfg(windows)]
pub(crate) fn run_powershell(script: &str) -> std::io::Result<String> {
    run_powershell_within(script, POWERSHELL_TIMEOUT)
}

#[cfg_attr(not(windows), allow(dead_code))]
/// How long one PowerShell run may take. Generous -- a UI Automation walk
/// of a browser window takes seconds -- but finite: a command that stops
/// to wait for input used to hold the agent forever.
const POWERSHELL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

/// As [`run_powershell`], with a chosen time limit.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn run_powershell_within(
    script: &str,
    timeout: std::time::Duration,
) -> std::io::Result<String> {
    use std::process::Command;

    // UTF-8 out: Windows PowerShell writes in the console's OEM code page
    // by default, which turns every "ş", "ğ" and "ı" in a window title,
    // file name or command output into mojibake on the way back.
    let script = format!("[Console]::OutputEncoding = [System.Text.Encoding]::UTF8; {script}");
    let mut cmd = Command::new("powershell");
    cmd.args([
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        &script,
    ]);
    let run = crate::process::run(cmd, timeout).map_err(std::io::Error::other)?;

    match run.code {
        Some(0) => Ok(run.stdout.trim().to_string()),
        Some(_) => {
            let err = run.stderr.trim().to_string();
            Err(std::io::Error::other(if err.is_empty() {
                "komut başarısız".to_string()
            } else {
                err
            }))
        }
        None => Err(std::io::Error::other(format!(
            "{} saniyede bitmedi, durduruldu",
            timeout.as_secs()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_info_reports_cpu_and_ram() {
        let out = SystemInfo.run(&Value::Null);
        assert!(out.ok);
        assert!(out.content.contains("CPU"), "çıktı: {}", out.content);
        assert!(out.content.contains("RAM"), "çıktı: {}", out.content);
    }

    #[test]
    fn process_list_is_capped() {
        let out = ListProcesses.run(&Value::Null);
        assert!(out.ok);
        // Başlık + en fazla 10 satır — bağlamı boğmamalı.
        assert!(out.content.lines().count() <= 11, "liste çok uzun");
    }

    #[test]
    fn volume_rejects_out_of_range() {
        for bad in [-5.0, 101.0, 1000.0] {
            let args = serde_json::json!({"level": bad});
            let out = SetVolume.run(&args);
            assert!(!out.ok, "{bad} reddedilmeliydi");
        }
    }

    #[test]
    fn volume_requires_the_parameter() {
        assert!(!SetVolume.run(&serde_json::json!({})).ok);
    }

    #[test]
    fn volume_accepts_string_numbers() {
        // Model sayıyı string olarak gönderebilir — kabul edilmeli.
        let args = serde_json::json!({"level": "50"});
        assert_eq!(arg_num(&args, "level"), Some(50.0));
    }

    #[test]
    fn volume_is_moderate_risk() {
        assert_eq!(SetVolume.risk(), Risk::Moderate);
        assert_eq!(SystemInfo.risk(), Risk::Safe);
    }

    #[test]
    fn battery_returns_something_on_windows() {
        let out = Battery.run(&Value::Null);
        if cfg!(windows) {
            // Masaüstünde "pil yok" da geçerli bir cevap.
            assert!(out.ok || out.content.contains("okunamadı"));
        }
    }
}

// ── Otomasyon zamanlayıcısı için ölçümler ───────────────────────────────────
//
// Koşullu tetikleyiciler (pil %20 altına inince…) bu değerleri okur.
// Tool'lardan ayrı fonksiyonlar: zamanlayıcı tool çalıştırmadan ölçmeli.

/// Pil yüzdesi. Pil yoksa (masaüstü) `None`.
pub fn battery_percent() -> Option<u32> {
    #[cfg(windows)]
    {
        let text = windows_battery()?;
        // "Pil: %85 (şarjda)" biçiminden sayıyı çıkar.
        let digits: String = text
            .chars()
            .skip_while(|c| !c.is_ascii_digit())
            .take_while(char::is_ascii_digit)
            .collect();
        digits.parse().ok()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Names of the running processes, lower-case, without `.exe`.
///
/// Refreshes the process list only -- not disks, networks or CPU -- since
/// the event watcher asks every few seconds.
pub fn process_names() -> std::collections::HashSet<String> {
    let mut guard = SYSTEM.lock().unwrap_or_else(|e| e.into_inner());
    let sys = guard.get_or_insert_with(System::new);
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    sys.processes()
        .values()
        .map(|p| {
            let name = p.name().to_string_lossy().to_lowercase();
            name.strip_suffix(".exe")
                .map(str::to_string)
                .unwrap_or(name)
        })
        .collect()
}

/// Seconds since the user last touched the keyboard or mouse.
///
/// `None` where the platform does not say -- the "welcome back" trigger then
/// simply never fires, which is better than guessing.
#[cfg(windows)]
pub fn idle_seconds() -> Option<u64> {
    #[repr(C)]
    struct LastInputInfo {
        size: u32,
        time: u32,
    }
    #[link(name = "user32")]
    extern "system" {
        fn GetLastInputInfo(info: *mut LastInputInfo) -> i32;
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetTickCount() -> u32;
    }
    let mut info = LastInputInfo {
        size: std::mem::size_of::<LastInputInfo>() as u32,
        time: 0,
    };
    // SAFETY: `info` is a correctly sized, writable struct as the API wants.
    let ok = unsafe { GetLastInputInfo(&mut info) };
    if ok == 0 {
        return None;
    }
    // Both are 32-bit millisecond tick counts; wrapping subtraction stays
    // right across the 49-day rollover.
    let now = unsafe { GetTickCount() };
    Some(u64::from(now.wrapping_sub(info.time)) / 1000)
}

#[cfg(not(windows))]
pub fn idle_seconds() -> Option<u64> {
    None
}

/// Anlık CPU kullanım yüzdesi.
pub fn cpu_percent() -> Option<u32> {
    // `with_cpu`, not `with_system`: this is on the once-a-second status
    // poll, and a full refresh there costs ~14 ms of lock-held work.
    let usage = with_cpu(|sys| sys.global_cpu_usage());
    if usage.is_nan() {
        return None;
    }
    Some(usage.clamp(0.0, 100.0) as u32)
}

#[cfg(test)]
mod measurement_tests {
    use super::*;

    #[test]
    fn cpu_measurement_is_a_valid_percentage() {
        let cpu = cpu_percent().expect("cpu ölçülebilmeli");
        assert!(cpu <= 100, "cpu %{cpu} geçersiz");
    }

    #[test]
    fn battery_measurement_is_valid_or_absent() {
        // Masaüstünde None dönmesi doğru davranış.
        if let Some(pct) = battery_percent() {
            assert!(pct <= 100, "pil %{pct} geçersiz");
        }
    }
}
