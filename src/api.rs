// voidctl serve — tek örnek yerel HTTP API (yalnızca 127.0.0.1)
// Uçlar: GET /health, GET /state, POST /sidetone /led /beep /otokapat
// Pil arka plan izleğinde örneklenip önbelleğe yazılır; /state anında döner.
// (battery(5) ~600 ms sürdüğü için istek anında ölçüm yapılmaz.)

use std::io::Read;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use crate::{durum_adi, history, Void};

pub const DEFAULT_PORT: u16 = 59312;
pub const DEFAULT_POLL_SECS: u64 = 60;

/// Önbelleğe yazılan son pil anlığı.
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub connected: bool,
    pub level: u8,
    pub min: u8,
    pub max: u8,
    pub status: u8,
    pub mic_up: bool,
    pub updated_ms: u64,
    pub err: String,
}

impl Default for Snapshot {
    fn default() -> Self {
        Snapshot {
            connected: false,
            level: 0,
            min: 0,
            max: 0,
            status: 0,
            mic_up: false,
            updated_ms: 0,
            err: "henüz ölçüm yok".into(),
        }
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Tüm HID işlemleri bu kilidin ardından yapılır: dongle tek kaynaktır,
/// eşzamanlı yaz/oku karışırsa cevaplar birbirine girer.
fn hid() -> &'static Mutex<()> {
    static HID: OnceLock<Mutex<()>> = OnceLock::new();
    HID.get_or_init(|| Mutex::new(()))
}

fn poll_once() -> Snapshot {
    let _g = hid().lock().unwrap();
    match Void::connect().and_then(|d| d.battery(3)) {
        Ok(b) => {
            history::append(b.level);
            Snapshot {
                connected: true,
                level: b.level,
                min: b.min,
                max: b.max,
                status: b.status,
                mic_up: b.mic_up,
                updated_ms: now_ms(),
                err: String::new(),
            }
        }
        Err(e) => Snapshot {
            err: e,
            ..Default::default()
        },
    }
}

fn hid_action(f: impl FnOnce(&Void) -> Result<(), String>) -> Result<(), String> {
    let _g = hid().lock().unwrap();
    let dev = Void::connect()?;
    f(&dev)
}

fn json_resp(code: u16, body: String) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    use tiny_http::{Header, Response, StatusCode};
    let ct: Header = "Content-Type: application/json".parse().unwrap();
    Response::from_string(body)
        .with_status_code(StatusCode(code))
        .with_header(ct)
}

fn ok() -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    json_resp(200, r#"{"ok":true}"#.into())
}

fn fail(code: u16, msg: &str) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    let body = serde_json::json!({"ok": false, "error": msg}).to_string();
    json_resp(code, body)
}

fn state_json(snap: &Snapshot, poll_secs: u64) -> String {
    if !snap.connected {
        return serde_json::json!({
            "connected": false,
            "error": snap.err,
        })
        .to_string();
    }
    let stale = now_ms().saturating_sub(snap.updated_ms) > 2 * poll_secs * 1000;
    serde_json::json!({
        "connected": true,
        "level": snap.level,
        "min": snap.min,
        "max": snap.max,
        "status": snap.status,
        "status_text": durum_adi(snap.status),
        "mic_up": snap.mic_up,
        "updated_ms": snap.updated_ms,
        "stale": stale,
    })
    .to_string()
}

fn read_body(req: &mut tiny_http::Request) -> Result<String, String> {
    let mut buf = Vec::new();
    req.as_reader()
        .take(4096)
        .read_to_end(&mut buf)
        .map_err(|e| format!("gövde okunamadı: {e}"))?;
    String::from_utf8(buf).map_err(|_| "gövde UTF-8 değil".to_string())
}

/// Dinlemeye başlar; geri dönmez.
pub fn serve(port: u16, poll_secs: u64) -> ! {
    let addr = format!("127.0.0.1:{port}");
    let server = tiny_http::Server::http(&addr).unwrap_or_else(|e| {
        eprintln!("HATA: {addr} dinlenemedi (başka serve çalışıyor olabilir): {e}");
        std::process::exit(1);
    });
    println!("voidctl serve — http://{addr} (yalnızca bu bilgisayar)");

    let snap = Arc::new(Mutex::new(poll_once()));
    let poller_snap = snap.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(poll_secs));
        let s = poll_once();
        *poller_snap.lock().unwrap() = s;
    });

    let version = env!("CARGO_PKG_VERSION");
    for mut req in server.incoming_requests() {
        let method = req.method().as_str().to_string();
        let path = req.url().split('?').next().unwrap_or("").to_string();
        let resp = match (method.as_str(), path.as_str()) {
            ("GET", "/health") => {
                let body = serde_json::json!({
                    "ok": true,
                    "service": "voidctl",
                    "version": version,
                    "port": port,
                })
                .to_string();
                json_resp(200, body)
            }
            ("GET", "/state") => {
                let s = snap.lock().unwrap().clone();
                json_resp(200, state_json(&s, poll_secs))
            }
            ("POST", "/sidetone") => match read_body(&mut req)
                .ok()
                .and_then(|b| serde_json::from_str::<serde_json::Value>(&b).ok())
                .and_then(|v| v.get("value").and_then(|x| x.as_u64()))
            {
                Some(n) => match hid_action(|d| d.set_sidetone(n as u16)) {
                    Ok(()) => ok(),
                    Err(e) => fail(500, &e),
                },
                None => fail(400, "gövde: {\"value\": 0-128} beklenir"),
            },
            ("POST", "/led") => match read_body(&mut req)
                .ok()
                .and_then(|b| serde_json::from_str::<serde_json::Value>(&b).ok())
                .and_then(|v| v.get("on").and_then(|x| x.as_bool()))
            {
                Some(on) => match hid_action(|d| d.set_led(on)) {
                    Ok(()) => ok(),
                    Err(e) => fail(500, &e),
                },
                None => fail(400, "gövde: {\"on\": true|false} beklenir"),
            },
            ("POST", "/beep") => {
                let id = read_body(&mut req)
                    .ok()
                    .and_then(|b| serde_json::from_str::<serde_json::Value>(&b).ok())
                    .and_then(|v| v.get("id").and_then(|x| x.as_u64()))
                    .unwrap_or(0) as u8;
                match hid_action(|d| d.beep(id)) {
                    Ok(()) => ok(),
                    Err(e) => fail(500, &e),
                }
            }
            ("POST", "/otokapat") => match read_body(&mut req)
                .ok()
                .and_then(|b| serde_json::from_str::<serde_json::Value>(&b).ok())
                .and_then(|v| v.get("on").and_then(|x| x.as_bool()))
            {
                Some(on) => match hid_action(|d| d.set_auto_shutdown(on)) {
                    Ok(()) => ok(),
                    Err(e) => fail(500, &e),
                },
                None => fail(400, "gövde: {\"on\": true|false} beklenir"),
            },
            _ => fail(404, "bilinmeyen uç (health/state/sidetone/led/beep/otokapat)"),
        };
        let _ = req.respond(resp);
    }
    unreachable!("HTTP döngüsü bitti");
}
