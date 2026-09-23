//! Yerleşik tool'lar.
//!
//! Eski projede 353 tool vardı ve `routine_*` gibi tek özellik için 9 ayrı
//! tool tanımlanmıştı. Burada bilinçli olarak **az sayıda, geniş kapsamlı**
//! tool var — model az seçenek arasından daha isabetli seçer.

pub mod automation;
pub mod canvas;
pub mod computer;
pub mod control;
pub mod core;
pub mod files;
pub mod media;
pub mod memory;
pub mod obsidian;
pub mod request_tools;
pub mod spotify;
pub mod steam;
pub mod system;
pub mod uia;
pub mod virustotal;
pub mod vision;
pub mod web;

use crate::tool::Registry;

/// Tüm yerleşik tool'ları kaydeder.
pub fn register_all(registry: &mut Registry) {
    // Çekirdek — her alanla birlikte sunulur.
    registry.register(Box::new(core::Now));
    registry.register(Box::new(core::Calculate));
    registry.register(Box::new(request_tools::RequestTools));

    // Sistem.
    registry.register(Box::new(system::SystemInfo));
    registry.register(Box::new(system::ListProcesses));
    registry.register(Box::new(system::Battery));
    registry.register(Box::new(system::SetVolume));
    registry.register(Box::new(control::SetBrightness));
    registry.register(Box::new(control::LaunchApp));
    registry.register(Box::new(control::CloseApp));
    registry.register(Box::new(control::RunCommand));
    registry.register(Box::new(control::ReadClipboard));
    registry.register(Box::new(control::WriteClipboard));
    registry.register(Box::new(control::ListWindows));

    // Dosya.
    registry.register(Box::new(files::ReadFile));
    registry.register(Box::new(files::ListDir));
    registry.register(Box::new(files::WriteFile));
    registry.register(Box::new(files::FindFile));

    // Görü.
    registry.register(Box::new(vision::Screenshot));
    registry.register(Box::new(computer::ScreenSize));
    registry.register(Box::new(computer::Click));
    registry.register(Box::new(computer::TypeText));
    registry.register(Box::new(computer::PressKey));
    // Döngünün "kontrol et" adımı: tıkla → bekle → gerekiyorsa bak.
    registry.register(Box::new(computer::WaitForScreen));
    registry.register(Box::new(uia::ListElements));
    registry.register(Box::new(uia::ClickElement));
    registry.register(Box::new(uia::SetElementText));
    registry.register(Box::new(uia::Scroll));
    registry.register(Box::new(uia::Drag));

    // Web.
    registry.register(Box::new(web::WebSearch));
    registry.register(Box::new(web::FetchUrl));

    // Medya.
    registry.register(Box::new(media::MediaControl));
    registry.register(Box::new(media::NowPlaying));

    // Otomasyon.
    registry.register(Box::new(automation::CreateAutomation));
    registry.register(Box::new(automation::ListAutomations));
    registry.register(Box::new(automation::DeleteAutomation));

    // Obsidian kasası.
    registry.register(Box::new(obsidian::SearchNotes));
    registry.register(Box::new(obsidian::ReadNote));
    registry.register(Box::new(obsidian::ListNotes));
    registry.register(Box::new(obsidian::CreateNote));
    registry.register(Box::new(obsidian::AppendNote));
    registry.register(Box::new(obsidian::EditNote));
    registry.register(Box::new(obsidian::DeleteNote));
    registry.register(Box::new(obsidian::NoteLinks));
    registry.register(Box::new(obsidian::DailyNote));

    // Spotify.
    registry.register(Box::new(spotify::Playback));
    registry.register(Box::new(spotify::NowPlaying));
    registry.register(Box::new(spotify::PlaySearch));
    registry.register(Box::new(spotify::Queue));
    registry.register(Box::new(spotify::PlayerSettings));
    registry.register(Box::new(spotify::Like));
    registry.register(Box::new(spotify::Devices));

    // Steam.
    registry.register(Box::new(steam::Library));
    registry.register(Box::new(steam::NowPlaying));
    registry.register(Box::new(steam::LaunchGame));
    registry.register(Box::new(steam::Achievements));
    registry.register(Box::new(steam::StorePrice));
    registry.register(Box::new(steam::Wishlist));
    registry.register(Box::new(steam::Friends));

    registry.register(Box::new(virustotal::ScanFile));
    registry.register(Box::new(virustotal::LookupHash));
    registry.register(Box::new(virustotal::CheckUrl));

    // Canvas — tek tool: varyasyon, büyütme ve parametre ayarı arayüzde.
    registry.register(Box::new(canvas::Generate));

    // Hafıza.
    registry.register(Box::new(memory::Remember));
    registry.register(Box::new(memory::Recall));
    registry.register(Box::new(memory::Forget));
}
