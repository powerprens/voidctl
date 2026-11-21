// OpenFreebuds RPC istemcisi — Huawei FreeBuds (ayrı süreç, yalnızca HTTP)
// Bilinen uçlar (OFB 0.18.1, webserver.py + stupid_rpc.py):
//   GET  /__rpc__/ping | GET /list_shortcuts | GET /{shortcut}
//   POST /__rpc__/get_state | /get_property | /set_property | /get_device_tags
// Güvenlik: genel RPC yardımcısı YOK — yalnız aşağıdaki allowlist metotlar.
// RPC bilinmeyen metoda HTTP 200 + "ok" döner; yanıtlar şekline göre doğrulanır.

use std::path::PathBuf;
use std::time::Duration;

pub const OFB_URL: &str = "http://127.0.0.1:19823";
const TIMEOUT: Duration = Duration::from_secs(5);

fn dir() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".into());
    PathBuf::from(base).join("voidctl")
}

fn panel_path() -> PathBuf {
    dir().join("panel.json")
}

/// panel.json: {"openfreebuds_url": "...", "openfreebuds_secret": "..."}
/// Dosya yoksa varsayılan URL ile boş secret döner.
pub fn load_config() -> (String, String) {
    let mut url = OFB_URL.to_string();
    let mut secret = String::new();
    if let Ok(txt) = std::fs::read_to_string(panel_path()) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&txt) {
            if let Some(u) = v.get("openfreebuds_url").and_then(|x| x.as_str()) {
                if u.starts_with("http://127.") || u.starts_with("http://localhost") {
                    url = u.to_string();
                }
            }
            if let Some(s) = v.get("openfreebuds_secret").and_then(|x| x.as_str()) {
                secret = s.to_string();
            }
        }
    }
    (url, secret)
}

pub fn save_url(url: &str, secret: &str) {
    let _ = std::fs::create_dir_all(dir());
    let body = serde_json::json!({"openfreebuds_url": url, "openfreebuds_secret": secret}).to_string();
    let _ = std::fs::write(panel_path(), body);
}

#[derive(Debug, Clone, Default)]
pub struct OfbClient {
    pub url: String,
    secret: String,
}

impl OfbClient {
    pub fn new(url: &str, secret: &str) -> Self {
        OfbClient {
            url: url.trim_end_matches('/').to_string(),
            secret: secret.to_string(),
        }
    }

    fn agent() -> ureq::Agent {
        ureq::Agent::config_builder()
            .timeout_global(Some(TIMEOUT))
            .build()
            .new_agent()
    }

    /// GET isteği, gövdeyi string döner (X-Secret gerekiyorsa eklenir).
    fn get_text(&self, agent: &ureq::Agent, path: &str) -> Option<String> {
        let mut r = agent.get(format!("{}{}", self.url, path));
        if !self.secret.is_empty() {
            r = r.header("X-Secret", &self.secret);
        }
        let mut resp = r.call().ok()?;
        resp.body_mut().read_to_string().ok()
    }

    /// OFB ayakta mı? (yalnız GET /__rpc__/ping, 200 + "pong")
    pub fn ping(&self) -> bool {
        let agent = Self::agent();
        self.get_text(&agent, "/__rpc__/ping")
            .map(|s| s == "pong")
            .unwrap_or(false)
    }

    /// Durum kodu: 3=bağlı, 1=bağlı değil, 2=bekliyor, 4=hata, 5=duraklatıldı, 0=durduruldu, -1=yok edildi.
    pub fn state(&self) -> Option<i64> {
        self.rpc("get_state", &[], &[]).and_then(|v| v.as_i64())
    }

    /// Cihaz adı + adres: [ad, adres].
    pub fn device_tags(&self) -> Option<(String, String)> {
        let v = self.rpc("get_device_tags", &[], &[])?;
        let a = v.as_array()?;
        Some((
            a.get(0)?.as_str()?.to_string(),
            a.get(1)?.as_str()?.to_string(),
        ))
    }

    /// Tüm özellik deposu (get_property boş çağrısı).
    pub fn store(&self) -> Option<serde_json::Value> {
        let v = self.rpc("get_property", &[], &[])?;
        v.is_object().then_some(v)
    }

    /// Tek özellik okuma; yoksa None (fallback yerine None kullanılır).
    pub fn prop(&self, group: &str, prop: &str) -> Option<String> {
        let ga = serde_json::Value::String(group.into());
        let pa = serde_json::Value::String(prop.into());
        match self.rpc("get_property", &[ga, pa], &[]) {
            Some(serde_json::Value::String(s)) => Some(s),
            _ => None,
        }
    }

    /// Tek özellik yazma. Değerler string taşınır (OFB kuralı).
    pub fn set_prop(&self, group: &str, prop: &str, value: &str) -> Result<(), String> {
        let args = [
            serde_json::Value::String(group.into()),
            serde_json::Value::String(prop.into()),
            serde_json::Value::String(value.into()),
        ];
        match self.rpc("set_property", &args, &[]) {
            Some(_) => Ok(()),
            None => Err("set_property başarısız (yanıt yok)".into()),
        }
    }

    /// Kısayol listesi — UI hangi düğmeleri göstereceğini buradan çıkarır.
    pub fn shortcuts(&self) -> Vec<String> {
        let agent = Self::agent();
        match self.get_text(&agent, "/list_shortcuts") {
            Some(txt) => serde_json::from_str::<Vec<String>>(&txt).unwrap_or_default(),
            None => vec![],
        }
    }

    /// Bilinen kısayolu çalıştır (allowlist dışı ad reddedilir).
    pub fn shortcut(&self, name: &str) -> Result<(), String> {
        const KNOWN: &[&str] = &[
            "mode_normal",
            "mode_cancellation",
            "mode_awareness",
            "enable_low_latency",
            "next_mode",
            "connect",
            "disconnect",
            "toggle_connect",
            "show_main_window",
        ];
        if !KNOWN.contains(&name) {
            return Err(format!("bilinmeyen kısayol: {name}"));
        }
        let agent = Self::agent();
        let mut r = agent.get(format!("{}/{name}", self.url));
        if !self.secret.is_empty() {
            r = r.header("X-Secret", &self.secret);
        }
        match r.call() {
            Ok(mut resp) => {
                let ok = resp
                    .body_mut()
                    .read_to_string()
                    .ok()
                    .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                    .and_then(|v| v.get("result").and_then(|x| x.as_bool()))
                    .unwrap_or(false);
                if ok {
                    Ok(())
                } else {
                    Err(format!("{name} başarısız (500)"))
                }
            }
            Err(e) => Err(format!("{name}: {e}")),
        }
    }

    /// Allowlist'li RPC: yalnız bilinen yönetici metotları.
    fn rpc(
        &self,
        method: &str,
        args: &[serde_json::Value],
        kwargs: &[(&str, serde_json::Value)],
    ) -> Option<serde_json::Value> {
        const KNOWN: &[&str] = &[
            "get_state",
            "get_property",
            "set_property",
            "get_device_tags",
            "get_health_report",
        ];
        if !KNOWN.contains(&method) {
            return None;
        }
        let agent = Self::agent();
        let mut req = agent.post(format!("{}/__rpc__/{}", self.url, method));
        if !self.secret.is_empty() {
            req = req.header("X-Secret", &self.secret);
        }
        let mut kw = serde_json::Map::new();
        for (k, v) in kwargs {
            kw.insert(k.to_string(), v.clone());
        }
        let body = serde_json::json!({"args": args, "kwargs": kw}).to_string();
        let v: serde_json::Value = req
            .header("Content-Type", "application/json")
            .send(&body)
            .ok()
            .and_then(|mut resp| resp.body_mut().read_to_string().ok())
            .and_then(|txt| serde_json::from_str(&txt).ok())?;
        // Sessiz başarı tuzağı: "ok" stringi = bilinmeyen metot yanıtı → geçersiz.
        if v == serde_json::Value::String("ok".into()) {
            return None;
        }
        Some(v)
    }
}

pub fn state_text(code: i64) -> &'static str {
    match code {
        3 => "Bağlı",
        1 => "Bağlı değil",
        2 => "Bekliyor",
        4 => "Hata",
        5 => "Duraklatıldı",
        0 => "Durduruldu",
        -1 => "Yok edildi",
        _ => "?",
    }
}

/// Virgüllü seçenek stringini listeye çevirir ("a,b,c" → [a,b,c]).
pub fn split_options(s: &str) -> Vec<String> {
    s.split(',')
        .map(str::trim)
        .filter(|x| !x.is_empty())
        .map(str::to_string)
        .collect()
}

/// Sayı veya sayı-string JSON değerini u8'e çevirir (OFB sürücüler arası fark).
fn num(v: &serde_json::Value) -> Option<u8> {
    if let Some(n) = v.as_i64() {
        return Some(n.clamp(0, 100) as u8);
    }
    v.as_str()?.trim().parse().ok()
}

fn str_of(v: &serde_json::Value) -> Option<String> {
    if let Some(s) = v.as_str() {
        return Some(s.to_string());
    }
    v.as_i64().map(|n| n.to_string())
}

/// Panelin tükettiği FreeBuds anlık görünümü (arka plan izleğinde `collect` ile dolar).
#[derive(Debug, Clone, Default)]
pub struct FbSnapshot {
    pub reachable: bool, // OFB ping'e cevap veriyor
    pub connected: bool, // state == 3
    pub state_code: i64, // -99 = bilinmiyor
    pub device_name: String,
    pub battery_line: String, // ör. "Sol 85 · Sağ 82 · Kutu 90 ⚡"
    pub anc_mode: String,
    pub anc_options: Vec<String>,
    pub eq_presets: Vec<String>,
    pub eq_current: String,
    pub low_latency: bool,
    pub has_anc: bool,
    pub has_eq: bool,
    pub err: String,
}

/// UI → arka plan izleği komutları.
#[derive(Debug, Clone)]
pub enum FbCommand {
    Refresh,
    Shortcut(String),
    SetProp {
        group: String,
        prop: String,
        value: String,
    },
}

/// OFB'den tüm görünümü toplar. Ağ çağrıları saniyeler sürebilir:
/// UI işleğinde DEĞİL, arka plan izleğinde çağırın.
pub fn collect(client: &OfbClient) -> FbSnapshot {
    let mut s = FbSnapshot {
        state_code: -99,
        ..Default::default()
    };
    if !client.ping() {
        s.err = "OpenFreebuds çalışmıyor".into();
        return s;
    }
    s.reachable = true;
    s.state_code = client.state().unwrap_or(-99);
    s.connected = s.state_code == 3;
    if let Some((name, _)) = client.device_tags() {
        s.device_name = name;
    }
    let store = client.store().unwrap_or_default();

    if let Some(b) = store.get("battery") {
        let mut parts: Vec<String> = vec![];
        let left = b.get("left").and_then(num);
        let right = b.get("right").and_then(num);
        match (left, right) {
            (Some(l), Some(r)) => parts.push(format!("Sol {l} · Sağ {r}")),
            (Some(l), None) => parts.push(format!("Sol {l}")),
            (None, Some(r)) => parts.push(format!("Sağ {r}")),
            (None, None) => {
                if let Some(g) = b.get("global").and_then(num) {
                    parts.push(format!("Pil {g}"));
                }
            }
        }
        if let Some(c) = b.get("case").and_then(num) {
            parts.push(format!("Kutu {c}"));
        }
        let charging = b
            .get("is_charging")
            .and_then(|v| v.as_bool().or_else(|| v.as_str().map(|x| x == "true")))
            .unwrap_or(false);
        if !parts.is_empty() {
            s.battery_line = parts.join(" · ");
            if charging {
                s.battery_line.push_str(" · şarj oluyor ⚡");
            }
        }
    }

    if let Some(a) = store.get("anc") {
        if let Some(m) = a.get("mode").and_then(str_of) {
            s.has_anc = true;
            s.anc_mode = m;
        }
        if let Some(o) = a.get("mode_options").and_then(str_of) {
            s.anc_options = split_options(&o);
        }
    }

    if let Some(snd) = store.get("sound") {
        if let Some(p) = snd.get("equalizer_preset").and_then(str_of) {
            s.eq_current = p;
        }
        if let Some(o) = snd.get("equalizer_preset_options").and_then(str_of) {
            s.eq_presets = split_options(&o);
            s.has_eq = !s.eq_presets.is_empty();
        }
    }

    if let Some(c) = store.get("config") {
        s.low_latency = c
            .get("low_latency")
            .and_then(|v| v.as_bool().or_else(|| v.as_str().map(|x| x == "true")))
            .unwrap_or(false);
    }
    s
}

/// Kurulu OpenFreebuds istemcisini bulur (PATH öncelikli).
pub fn find_ofb_exe() -> Option<PathBuf> {
    use std::os::windows::process::CommandExt;
    const NO_WINDOW: u32 = 0x0800_0000;
    // 1) PATH'te ara (kurulumlara göre iki ad da olabilir)
    for probe in ["openfreebuds.exe", "openfreebuds_qt.exe"] {
        if let Ok(out) = std::process::Command::new("where")
            .arg(probe)
            .creation_flags(NO_WINDOW)
            .output()
        {
            if out.status.success() {
                let first = String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if !first.is_empty() {
                    return Some(PathBuf::from(first));
                }
            }
        }
    }
    // 2) bilinen kurulum yerleri
    let mut cands: Vec<PathBuf> = vec![];
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        cands.push(PathBuf::from(format!(
            "{local}\\OpenFreebuds\\openfreebuds.exe"
        )));
        cands.push(PathBuf::from(format!(
            "{local}\\Microsoft\\WinGet\\Links\\openfreebuds.exe"
        )));
        cands.push(PathBuf::from(format!(
            "{local}\\Programs\\OpenFreebuds\\openfreebuds_qt.exe"
        )));
    }
    cands.push(PathBuf::from(
        "C:\\Program Files\\OpenFreebuds\\openfreebuds_qt.exe",
    ));
    cands.into_iter().find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secenek_ayristirma() {
        assert_eq!(split_options("a,b, c"), vec!["a", "b", "c"]);
        assert!(split_options("").is_empty());
    }

    #[test]
    fn durum_metni() {
        assert_eq!(state_text(3), "Bağlı");
        assert_eq!(state_text(1), "Bağlı değil");
    }

    #[test]
    fn bilinmeyen_rpc_reddedilir() {
        let c = OfbClient::new(OFB_URL, "");
        assert!(c.rpc("destroy", &[], &[]).is_none());
        assert!(c.rpc("_set_paused", &[], &[]).is_none());
    }

    #[test]
    fn bilinmeyen_kisayol_reddedilir() {
        let c = OfbClient::new(OFB_URL, "");
        assert!(c.shortcut("destroy").is_err());
    }
}
