// Uygulamaya göre otomatik sidetone — %LOCALAPPDATA%\voidctl\profiles.ini

use std::collections::BTreeMap;
use std::path::PathBuf;

fn dir() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".into());
    std::path::PathBuf::from(base).join("voidctl")
}

fn ini_path() -> PathBuf {
    dir().join("profiles.ini")
}

pub fn default_ini() -> String {
    "[rules]\n\
     # exe_adi=seviye (0-128), 'default' eslesmeyen her sey icin\n\
     default=90\n\
     discord.exe=100\n\
     zoom.exe=100\n\
     chrome.exe=80\n\
     firefox.exe=80\n"
        .into()
}

pub fn load_rules() -> BTreeMap<String, u16> {
    let p = ini_path();
    if !p.exists() {
        let _ = std::fs::create_dir_all(dir());
        let _ = std::fs::write(&p, default_ini());
    }
    let mut map = BTreeMap::new();
    let Ok(txt) = std::fs::read_to_string(&p) else {
        return map;
    };
    for line in txt.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') || l.starts_with('[') {
            continue;
        }
        if let Some((k, v)) = l.split_once('=') {
            if let Ok(n) = v.trim().parse::<u16>() {
                map.insert(k.trim().to_lowercase(), n.min(128));
            }
        }
    }
    map
}

/// exe adına kural seviyesi; yoksa 'default'.
pub fn level_for(rules: &BTreeMap<String, u16>, exe: &str) -> Option<u16> {
    let e = exe.to_lowercase();
    rules.get(&e).copied().or_else(|| rules.get("default").copied())
}

/// Ön plandaki pencerenin exe adı (ör. "discord.exe").
pub fn foreground_exe() -> Option<String> {
    use std::os::windows::ffi::OsStringExt;
    use winapi::um::handleapi::CloseHandle;
    use winapi::um::processthreadsapi::OpenProcess;
    use winapi::um::winbase::QueryFullProcessImageNameW;
    use winapi::um::winnt::PROCESS_QUERY_LIMITED_INFORMATION;
    use winapi::um::winuser::{GetForegroundWindow, GetWindowThreadProcessId};
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == 0 {
            return None;
        }
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return None;
        }
        let mut buf = [0u16; 512];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut len);
        CloseHandle(h);
        if ok != 0 {
            let full = std::ffi::OsString::from_wide(&buf[..len as usize])
                .to_string_lossy()
                .to_string();
            Some(full.rsplit('\\').next().unwrap_or(&full).to_string())
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kural_eslesme() {
        let mut r = BTreeMap::new();
        r.insert("discord.exe".into(), 100);
        r.insert("default".into(), 40);
        assert_eq!(level_for(&r, "Discord.EXE"), Some(100));
        assert_eq!(level_for(&r, "bilmemne.exe"), Some(40));
    }

    #[test]
    fn ini_ayristirma() {
        let _ = std::fs::create_dir_all(dir());
        std::fs::write(ini_path(), "[rules]\ndefault=55\ntestapp.exe=10\n# yorum\n").unwrap();
        let r = load_rules();
        assert_eq!(r.get("default"), Some(&55));
        assert_eq!(r.get("testapp.exe"), Some(&10));
    }
}
