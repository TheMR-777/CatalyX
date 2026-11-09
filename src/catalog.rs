//! What CatalyX knows about common programs and services: a friendly name, a category and a
//! suggested rule. The suggestions are what a fresh install starts with, and what the
//! "Use suggestions" buttons apply. They are deliberately gentle: nothing that can lose work is
//! ever *closed* by suggestion - browsers, chat and sync clients are only *lowered* (lowest
//! priority and memory trimmed), which frees the CPU and RAM a game wants without data loss.

use crate::config::ProcAction;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Category {
    Browser,
    Chat,
    Sync,
    Updater,
    Windows,
    Media,
    Dev,
    Launcher,
    Peripheral,
    Download,
    Other,
}

impl Category {
    pub fn label(self) -> &'static str {
        match self {
            Category::Browser => "Browser",
            Category::Chat => "Chat",
            Category::Sync => "Cloud sync",
            Category::Updater => "Updater",
            Category::Windows => "Windows extra",
            Category::Media => "Media",
            Category::Dev => "Developer tool",
            Category::Launcher => "Game launcher",
            Category::Peripheral => "Hardware software",
            Category::Download => "Downloader",
            Category::Other => "App",
        }
    }
}

pub struct KnownApp {
    pub exe: &'static str,
    pub label: &'static str,
    pub category: Category,
    /// What a fresh install does with it while gaming.
    pub suggested: ProcAction,
}

const fn app(exe: &'static str, label: &'static str, category: Category, suggested: ProcAction) -> KnownApp {
    KnownApp { exe, label, category, suggested }
}

use Category::*;
use ProcAction::{Close, Keep, Lower, Suspend};

pub const KNOWN_APPS: &[KnownApp] = &[
    // Browsers: lowered, never closed (tabs and forms survive).
    app("chrome.exe", "Google Chrome", Browser, Lower),
    app("msedge.exe", "Microsoft Edge", Browser, Lower),
    app("msedgewebview2.exe", "Edge web view (apps' built-in pages)", Browser, Lower),
    app("firefox.exe", "Firefox", Browser, Lower),
    app("brave.exe", "Brave", Browser, Lower),
    app("opera.exe", "Opera", Browser, Lower),
    app("opera_gx.exe", "Opera GX", Browser, Lower),
    app("vivaldi.exe", "Vivaldi", Browser, Lower),
    app("arc.exe", "Arc", Browser, Lower),
    // Chat: lowered. Discord is left alone on purpose - people talk while they play.
    app("discord.exe", "Discord (voice chat)", Chat, Keep),
    app("telegram.exe", "Telegram", Chat, Lower),
    app("whatsapp.exe", "WhatsApp", Chat, Lower),
    app("slack.exe", "Slack", Chat, Lower),
    app("teams.exe", "Microsoft Teams", Chat, Lower),
    app("ms-teams.exe", "Microsoft Teams", Chat, Lower),
    app("zoom.exe", "Zoom", Chat, Lower),
    app("skype.exe", "Skype", Chat, Lower),
    app("signal.exe", "Signal", Chat, Lower),
    // Cloud sync: lowered so uploads never fight the game for disk and network.
    app("onedrive.exe", "OneDrive", Sync, Lower),
    app("onedrive.sync.service.exe", "OneDrive sync engine", Sync, Lower),
    app("filecoauth.exe", "OneDrive file co-authoring", Sync, Lower),
    app("dropbox.exe", "Dropbox", Sync, Lower),
    app("googledrivefs.exe", "Google Drive", Sync, Lower),
    app("megasync.exe", "MEGA sync", Sync, Lower),
    app("icloudservices.exe", "iCloud", Sync, Lower),
    app("nextcloud.exe", "Nextcloud", Sync, Lower),
    // Updaters: pure background noise, safe to end (they come back on their own schedule).
    app("googleupdate.exe", "Google updater", Updater, Close),
    app("microsoftedgeupdate.exe", "Edge updater", Updater, Close),
    app("adobearm.exe", "Adobe updater", Updater, Close),
    app("adobeupdateservice.exe", "Adobe updater", Updater, Close),
    app("jusched.exe", "Java updater", Updater, Close),
    app("softwareupdate.exe", "Software updater", Updater, Close),
    app("updater.exe", "Updater", Updater, Close),
    app("ccxprocess.exe", "Adobe Creative Cloud helper", Updater, Close),
    app("creative cloud ui helper.exe", "Adobe Creative Cloud helper", Updater, Close),
    app("acrotray.exe", "Acrobat tray helper", Updater, Close),
    app("hpsupportassistant.exe", "HP Support Assistant", Updater, Close),
    app("lenovovantage.exe", "Lenovo Vantage", Updater, Suspend),
    // Windows extras that nothing in a game needs.
    app("widgets.exe", "Windows Widgets", Windows, Close),
    app("widgetservice.exe", "Windows Widgets service", Windows, Close),
    app("phoneexperiencehost.exe", "Phone Link", Windows, Close),
    app("yourphone.exe", "Phone Link", Windows, Close),
    app("yourphoneappproxy.exe", "Phone Link helper", Windows, Close),
    app("crossdeviceresume.exe", "Cross-device resume", Windows, Close),
    app("crossdeviceservice.exe", "Cross-device service", Windows, Close),
    app("photos.exe", "Photos", Windows, Close),
    app("microsoft.photos.exe", "Photos", Windows, Close),
    app("copilot.exe", "Copilot", Windows, Close),
    app("useroobebroker.exe", "Windows welcome helper", Windows, Close),
    app("helppane.exe", "Windows help", Windows, Close),
    app("systemsettings.exe", "Settings window", Windows, Close),
    app("mspcmanager.exe", "PC Manager", Windows, Close),
    app("mspcmanagercore.exe", "PC Manager core", Windows, Close),
    app("edgegameassist.exe", "Edge game assist", Windows, Close),
    app("xboxpcappft.exe", "Xbox app helper", Windows, Close),
    app("gamingservicesui.exe", "Gaming services UI", Windows, Keep),
    app("windowspackagemanagerserver.exe", "WinGet server", Windows, Close),
    app("searchhost.exe", "Windows Search window", Windows, Lower),
    app("startmenuexperiencehost.exe", "Start menu", Windows, Keep),
    app("mobsync.exe", "Offline files sync", Windows, Close),
    app("filesyncconfig.exe", "Offline files", Windows, Close),
    app("appmarket.exe", "App market helper", Windows, Close),
    app("uninstallmonitor.exe", "Uninstall monitor", Windows, Close),
    app("pubplatform.exe", "Publisher platform", Windows, Close),
    app("sdxhelper.exe", "Office helper", Windows, Close),
    // Media: lowered so music keeps playing but costs the game nothing.
    app("spotify.exe", "Spotify", Media, Lower),
    app("itunes.exe", "iTunes", Media, Lower),
    app("vlc.exe", "VLC", Media, Suspend),
    app("wmplayer.exe", "Windows Media Player", Media, Suspend),
    app("musicbee.exe", "MusicBee", Media, Lower),
    app("obs64.exe", "OBS Studio", Media, Keep),
    // Developer tools: lowered, never closed.
    app("code.exe", "Visual Studio Code", Dev, Lower),
    app("devenv.exe", "Visual Studio", Dev, Lower),
    app("idea64.exe", "IntelliJ IDEA", Dev, Lower),
    app("pycharm64.exe", "PyCharm", Dev, Lower),
    app("docker desktop.exe", "Docker Desktop", Dev, Lower),
    app("com.docker.backend.exe", "Docker engine", Dev, Lower),
    app("node.exe", "Node.js", Dev, Lower),
    app("powertoys.exe", "PowerToys", Dev, Suspend),
    app("powertoys.powerlauncher.exe", "PowerToys Run", Dev, Suspend),
    app("powertoys.colorpickerui.exe", "PowerToys Color Picker", Dev, Suspend),
    app("powertoys.powerocr.exe", "PowerToys Text Extractor", Dev, Suspend),
    app("powertoys.advancedpaste.exe", "PowerToys Advanced Paste", Dev, Suspend),
    app("powertoys.peek.ui.exe", "PowerToys Peek", Dev, Suspend),
    app("powertoys.quickaccess.exe", "PowerToys Quick Access", Dev, Suspend),
    app("microsoft.cmdpal.ui.exe", "Command Palette", Dev, Suspend),
    // Downloaders.
    app("idman.exe", "Internet Download Manager", Download, Suspend),
    app("qbittorrent.exe", "qBittorrent", Download, Lower),
    app("utorrent.exe", "uTorrent", Download, Lower),
    app("bittorrent.exe", "BitTorrent", Download, Lower),
    app("freedownloadmanager.exe", "Free Download Manager", Download, Lower),
    // Game launchers and hardware software: shown, never suggested for any action.
    app("steam.exe", "Steam", Launcher, Keep),
    app("steamwebhelper.exe", "Steam web helper", Launcher, Keep),
    app("epicgameslauncher.exe", "Epic Games Launcher", Launcher, Keep),
    app("battle.net.exe", "Battle.net", Launcher, Keep),
    app("galaxyclient.exe", "GOG Galaxy", Launcher, Keep),
    app("eadesktop.exe", "EA app", Launcher, Keep),
    app("upc.exe", "Ubisoft Connect", Launcher, Keep),
    app("icue.exe", "Corsair iCUE", Peripheral, Keep),
    app("razer synapse 3.exe", "Razer Synapse", Peripheral, Keep),
    app("lghub.exe", "Logitech G HUB", Peripheral, Keep),
    app("armourycrate.service.exe", "ASUS Armoury Crate", Peripheral, Keep),
    app("msiafterburner.exe", "MSI Afterburner", Peripheral, Keep),
    app("rtss.exe", "RivaTuner Statistics Server", Peripheral, Keep),
];

pub fn known(exe_lower: &str) -> Option<&'static KnownApp> {
    KNOWN_APPS.iter().find(|a| a.exe == exe_lower)
}

/// A readable name for a program: the catalog's, else the file name without ".exe".
pub fn friendly(exe_lower: &str, fallback: &str) -> String {
    match known(exe_lower) {
        Some(k) => k.label.to_string(),
        None => {
            let n = if fallback.is_empty() { exe_lower } else { fallback };
            n.trim_end_matches(".exe").trim_end_matches(".EXE").to_string()
        }
    }
}

pub fn category_of(exe_lower: &str) -> Category {
    known(exe_lower).map(|k| k.category).unwrap_or(Category::Other)
}

/// The rules a fresh install starts with: every catalog entry that is not "Keep".
pub fn suggested_apps() -> std::collections::BTreeMap<String, ProcAction> {
    KNOWN_APPS.iter().filter(|a| a.suggested != ProcAction::Keep).map(|a| (a.exe.to_string(), a.suggested)).collect()
}

// ------------------------------------------------------------------ services

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Impact {
    /// Nothing a person using this PC for gaming will notice.
    Safe,
    /// Stops something people sometimes use (printing, scanners, Windows Update downloads).
    Noticeable,
}

pub struct KnownService {
    pub name: &'static str,
    pub why: &'static str,
    pub impact: Impact,
    /// Part of the recommended set.
    pub recommended: bool,
}

const fn svc(name: &'static str, why: &'static str, impact: Impact, recommended: bool) -> KnownService {
    KnownService { name, why, impact, recommended }
}

/// The recommended set: services that are safe to pause for a play session and that Windows or the
/// service's own clients start again on demand. Everything is started again afterwards.
pub const KNOWN_SERVICES: &[KnownService] = &[
    svc("DiagTrack", "Sends usage data to Microsoft in the background", Impact::Safe, true),
    svc("MapsBroker", "Downloaded maps for the Maps app", Impact::Safe, true),
    svc("WMPNetworkSvc", "Windows Media Player network sharing", Impact::Safe, true),
    svc("Fax", "Fax", Impact::Safe, true),
    svc("lfsvc", "Location service", Impact::Safe, true),
    svc("RetailDemo", "Shop demo mode", Impact::Safe, true),
    svc("WerSvc", "Error reporting", Impact::Safe, true),
    svc("wercplsupport", "Error reporting UI", Impact::Safe, true),
    svc("dmwappushservice", "Device management push messages", Impact::Safe, true),
    svc("PcaSvc", "Program compatibility assistant", Impact::Safe, true),
    svc("WdiServiceHost", "Diagnostic service host", Impact::Safe, true),
    svc("WdiSystemHost", "Diagnostic system host", Impact::Safe, true),
    svc("DPS", "Diagnostic policy service", Impact::Safe, true),
    svc("TrkWks", "Tracks moved NTFS files", Impact::Safe, true),
    svc("CscService", "Offline files", Impact::Safe, true),
    svc("RemoteRegistry", "Remote registry access", Impact::Safe, true),
    svc("WebClient", "WebDAV file access", Impact::Safe, true),
    svc("SNMPTRAP", "SNMP traps", Impact::Safe, true),
    svc("NcdAutoSetup", "Network connected devices auto-setup", Impact::Safe, true),
    svc("PrintNotify", "Printer notifications", Impact::Safe, true),
    svc("WSearch", "Windows Search indexer: heavy disk and CPU use", Impact::Safe, true),
    svc("SysMain", "Superfetch / prefetching", Impact::Safe, true),
    svc("TabletInputService", "Touch keyboard and handwriting", Impact::Safe, true),
    svc("StiSvc", "Scanner and camera import", Impact::Noticeable, true),
    svc("Spooler", "Printing", Impact::Noticeable, true),
    svc("wuauserv", "Windows Update downloads", Impact::Noticeable, true),
    svc("DoSvc", "Delivery Optimization (update sharing)", Impact::Noticeable, true),
    svc("UsoSvc", "Update orchestrator", Impact::Noticeable, true),
    svc("BITS", "Background file transfers", Impact::Noticeable, false),
    svc("bthserv", "Bluetooth (stops Bluetooth controllers and headsets)", Impact::Noticeable, false),
    svc("W32Time", "Clock synchronisation", Impact::Noticeable, false),
    svc("SessionEnv", "Remote Desktop configuration", Impact::Noticeable, false),
    svc("UmRdpService", "Remote Desktop", Impact::Noticeable, false),
    svc("Themes", "Visual themes", Impact::Noticeable, false),
];

pub fn known_service(name: &str) -> Option<&'static KnownService> {
    KNOWN_SERVICES.iter().find(|s| s.name.eq_ignore_ascii_case(name))
}

pub fn suggested_services() -> std::collections::BTreeMap<String, bool> {
    KNOWN_SERVICES.iter().map(|s| (s.name.to_string(), s.recommended)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{app_is_protected, service_is_protected};

    #[test]
    fn nothing_protected_is_ever_suggested() {
        for a in KNOWN_APPS {
            assert!(!app_is_protected(a.exe) || a.suggested == ProcAction::Keep, "{}", a.exe);
        }
        for s in KNOWN_SERVICES.iter().filter(|s| s.recommended) {
            assert!(!service_is_protected(s.name), "{}", s.name);
        }
    }

    #[test]
    fn data_holding_apps_are_never_closed_by_suggestion() {
        for a in KNOWN_APPS {
            if matches!(a.category, Browser | Chat | Sync | Dev | Media | Download) {
                assert_ne!(a.suggested, ProcAction::Close, "{}", a.exe);
            }
        }
    }

    #[test]
    fn catalog_names_are_lowercase_and_unique() {
        let mut seen = std::collections::HashSet::new();
        for a in KNOWN_APPS {
            assert_eq!(a.exe, a.exe.to_lowercase());
            assert!(seen.insert(a.exe), "duplicate {}", a.exe);
        }
    }
}
