// voidgui — voidctl'in mini pencere + tepsi uygulaması (koyu modern tema)
#![windows_subsystem = "windows"]

use native_windows_gui as nwg;
use std::cell::{Cell, RefCell};
use std::sync::mpsc;
use voidctl::{
    durum_adi, freebuds,
    freebuds::{FbCommand, FbSnapshot},
    history, profiles, Void,
};

const UPDATE_MS: u32 = 30_000; // tam yenileme (medyan pil + tepsi + geçmiş)
const APP_MS: u32 = 3_000; // ön plan uygulaması kontrolü

const WINDOW_TITLE: &str = "Voidctl — Corsair Void";
const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
const ICO_TMP: &str = "voidgui_tray.ico";

// ---------- tema renkleri ----------
const BG: [u8; 3] = [32, 34, 40];
const TEXT_MAIN: [u8; 3] = [232, 234, 237];
const TEXT_DIM: [u8; 3] = [154, 160, 166];
const GOOD: [u8; 3] = [97, 220, 130];
const WARN: [u8; 3] = [250, 200, 70];
const BAD: [u8; 3] = [245, 95, 90];

fn batt_color(level: u8) -> [u8; 3] {
    if level >= 50 {
        GOOD
    } else if level >= 20 {
        WARN
    } else {
        BAD
    }
}

/// RichLabel metnini renkle yaz (metin + karakter formatı birlikte).
fn rl_set(l: &nwg::RichLabel, text: &str, color: [u8; 3]) {
    l.set_text(text);
    let fmt = nwg::CharFormat {
        text_color: Some(color),
        ..Default::default()
    };
    l.set_char_format(0..text.len() as u32, &fmt);
}

/// Win11 koyu kontrol teması (butonlar, kaydırıcılar vb.).
fn enable_dark_app_mode() {
    use winapi::um::libloaderapi::{GetProcAddress, LoadLibraryA};
    type SetPreferredAppMode = unsafe extern "system" fn(i32) -> i32;
    let ux = b"uxtheme.dll\0";
    unsafe {
        let h = LoadLibraryA(ux.as_ptr() as *const i8);
        if !h.is_null() {
            // SetPreferredAppMode — belgelenmemiş, ordinal 135; 1 = AllowDark
            let f = GetProcAddress(h, 135 as *const i8);
            if !f.is_null() {
                let f: SetPreferredAppMode = std::mem::transmute(f);
                f(1);
            }
        }
    }
}

/// Koyu başlık çubuğu + pencere çerçeve rengi (DWM).
fn dark_frame(hwnd: winapi::shared::windef::HWND) {
    use winapi::ctypes::c_void;
    use winapi::um::dwmapi::DwmSetWindowAttribute;
    unsafe {
        let on: i32 = 1;
        DwmSetWindowAttribute(hwnd, 20, &on as *const i32 as *const c_void, 4);
        let to_color = |c: [u8; 3]| -> u32 {
            c[0] as u32 | ((c[1] as u32) << 8) | ((c[2] as u32) << 16) // COLORREF = 0x00BBGGRR
        };
        let cap = to_color(BG);
        DwmSetWindowAttribute(hwnd, 35, &cap as *const u32 as *const c_void, 4);
        let txt = to_color(TEXT_MAIN);
        DwmSetWindowAttribute(hwnd, 36, &txt as *const u32 as *const c_void, 4);
    }
}

/// Buton/kaydırıcı/ilerleme çubuğu gibi kontrollere Win11 koyu teması.
fn dark_controls(gui: &Gui) {
    use winapi::shared::windef::HWND;
    use winapi::um::uxtheme::SetWindowTheme;
    let name: Vec<u16> = "DarkMode_Explorer\0".encode_utf16().collect();
    let hwnds: [Option<HWND>; 15] = [
        gui.side_apply.handle.hwnd(),
        gui.led_on.handle.hwnd(),
        gui.led_off.handle.hwnd(),
        gui.refresh_btn.handle.hwnd(),
        gui.beep1.handle.hwnd(),
        gui.beep2.handle.hwnd(),
        gui.auto_side_btn.handle.hwnd(),
        gui.autostart_btn.handle.hwnd(),
        gui.side_slider.handle.hwnd(),
        gui.battery_bar.handle.hwnd(),
        gui.fb_next.handle.hwnd(),
        gui.fb_latency.handle.hwnd(),
        gui.fb_eq.handle.hwnd(),
        gui.fb_toggle.handle.hwnd(),
        gui.fb_launch.handle.hwnd(),
    ];
    for hwnd in hwnds.into_iter().flatten() {
        unsafe {
            SetWindowTheme(hwnd, name.as_ptr(), std::ptr::null());
        }
    }
}

/// WM_ERASEBKGND: pencere zemini koyu boya.
fn dark_bg_handler(handle: &nwg::ControlHandle, id: usize) -> nwg::RawEventHandler {
    nwg::bind_raw_event_handler(handle, id, |hwnd, msg, w, _l| {
        if msg == 0x0014 {
            unsafe {
                use winapi::shared::windef::RECT;
                use winapi::um::wingdi::{CreateSolidBrush, DeleteObject, RGB};
                use winapi::um::winuser::{FillRect, GetClientRect};
                let mut rc = RECT { left: 0, top: 0, right: 0, bottom: 0 };
                GetClientRect(hwnd, &mut rc);
                let brush = CreateSolidBrush(RGB(BG[0], BG[1], BG[2]));
                FillRect(w as _, &rc, brush);
                DeleteObject(brush as _);
            }
            Some(1)
        } else {
            None
        }
    })
    .expect("arka plan işleyicisi")
}

// ---------- 3x5 rakam fontu ----------
const DIGITS: [[u8; 5]; 10] = [
    [0b111, 0b101, 0b101, 0b101, 0b111],
    [0b010, 0b110, 0b010, 0b010, 0b111],
    [0b111, 0b001, 0b111, 0b100, 0b111],
    [0b111, 0b001, 0b111, 0b001, 0b111],
    [0b101, 0b101, 0b111, 0b001, 0b001],
    [0b111, 0b100, 0b111, 0b001, 0b111],
    [0b111, 0b100, 0b111, 0b101, 0b111],
    [0b111, 0b001, 0b001, 0b001, 0b001],
    [0b111, 0b101, 0b111, 0b101, 0b111],
    [0b111, 0b101, 0b111, 0b001, 0b111],
];

fn draw_pct_icon(pct: u8) -> Vec<[u8; 4]> {
    const W: usize = 32;
    const H: usize = 32;
    let pct = pct.min(99);
    let (r, g, b) = if pct >= 50 {
        (96, 220, 130)
    } else if pct >= 20 {
        (250, 200, 70)
    } else {
        (245, 95, 90)
    };

    let mut px = vec![[0u8; 4]; W * H];
    let rad = 6.0f32;
    for y in 0..H {
        for x in 0..W {
            let fx = x as f32 + 0.5;
            let fy = y as f32 + 0.5;
            let cx = fx.min(W as f32 - fx).min(rad);
            let cy = fy.min(H as f32 - fy).min(rad);
            let inside = if cx >= rad || cy >= rad {
                true
            } else {
                (rad - cx) * (rad - cx) + (rad - cy) * (rad - cy) <= rad * rad
            };
            if inside {
                px[y * W + x] = [38, 41, 48, 255];
            }
        }
    }

    let digits = [(pct / 10) as usize, (pct % 10) as usize];
    for (di, &d) in digits.iter().enumerate() {
        let ox = 2 + di * 16;
        let oy = 6;
        let font = DIGITS[d];
        for row in 0..5 {
            for col in 0..3 {
                if font[row] & (0b100 >> col) != 0 {
                    for sy in 0..4 {
                        for sx in 0..4 {
                            let yy = oy + row * 4 + sy;
                            let xx = ox + col * 4 + sx;
                            px[yy * W + xx] = [r, g, b, 255];
                        }
                    }
                }
            }
        }
    }
    px
}

fn pack_ico(px: &[[u8; 4]]) -> Vec<u8> {
    let mut out = Vec::with_capacity(22 + 40 + 32 * 32 * 4 + 128);
    out.extend_from_slice(&[0, 0, 1, 0, 1, 0]);
    out.extend_from_slice(&[32, 32, 0, 0, 1, 0, 32, 0]);
    let payload_len: u32 = (40 + 32 * 32 * 4 + 128) as u32;
    out.extend_from_slice(&payload_len.to_le_bytes());
    out.extend_from_slice(&22u32.to_le_bytes());
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&32i32.to_le_bytes());
    out.extend_from_slice(&64i32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&[0u8; 24]);
    for row in (0..32).rev() {
        for col in 0..32 {
            let [r, g, b, a] = px[row * 32 + col];
            out.extend_from_slice(&[b, g, r, a]);
        }
    }
    out.extend_from_slice(&[0u8; 128]);
    out
}

fn write_tray_ico(pct: u8) {
    let path = std::env::temp_dir().join(ICO_TMP);
    let _ = std::fs::write(path, pack_ico(&draw_pct_icon(pct)));
}

// ---------- otomatik başlangıç ----------
fn autostart_on() -> bool {
    use std::os::windows::process::CommandExt;
    const NO_WINDOW: u32 = 0x0800_0000;
    let out = std::process::Command::new("reg")
        .args(["query", RUN_KEY, "/v", "voidgui"])
        .creation_flags(NO_WINDOW)
        .output();
    matches!(out, Ok(o) if o.status.success())
}

fn autostart_set(on: bool) -> bool {
    use std::os::windows::process::CommandExt;
    const NO_WINDOW: u32 = 0x0800_0000;
    let exe = std::env::current_exe().unwrap_or_default();
    let val = format!("\"{}\" --hidden", exe.to_string_lossy());
    let mut cmd = if on {
        let mut c = std::process::Command::new("reg");
        c.args(["add", RUN_KEY, "/v", "voidgui", "/t", "REG_SZ", "/d", &val, "/f"]);
        c
    } else {
        let mut c = std::process::Command::new("reg");
        c.args(["delete", RUN_KEY, "/v", "voidgui", "/f"]);
        c
    };
    matches!(cmd.creation_flags(NO_WINDOW).output(), Ok(o) if o.status.success())
}

// ---------- tek örnek ----------
fn bring_existing_forward() -> bool {
    use winapi::um::winuser::{FindWindowW, SetForegroundWindow, ShowWindow, SW_RESTORE};
    let title: Vec<u16> = WINDOW_TITLE.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let hwnd = FindWindowW(std::ptr::null(), title.as_ptr());
        if hwnd.is_null() {
            false
        } else {
            ShowWindow(hwnd, SW_RESTORE);
            SetForegroundWindow(hwnd);
            true
        }
    }
}

fn cursor_pos() -> (i32, i32) {
    use winapi::shared::windef::POINT;
    use winapi::um::winuser::GetCursorPos;
    let mut p = POINT { x: 0, y: 0 };
    unsafe {
        GetCursorPos(&mut p);
    }
    (p.x, p.y)
}

/// Kurulu OpenFreebuds istemcisini başlatır; yoksa kullanıcıyı yönlendirir.
fn launch_ofb(status: &nwg::RichLabel) {
    match freebuds::find_ofb_exe() {
        Some(exe) => match std::process::Command::new(&exe).spawn() {
            Ok(_) => rl_set(status, "OpenFreebuds başlatıldı ✓", GOOD),
            Err(e) => rl_set(status, &format!("OFB başlatılamadı: {e}"), BAD),
        },
        None => rl_set(
            status,
            "OpenFreebuds bulunamadı — winget install MelianMiko.OpenFreebuds",
            WARN,
        ),
    }
}

#[derive(Default)]
struct Gui {
    big_font: nwg::Font,
    window: nwg::Window,
    battery_label: nwg::RichLabel,
    battery_bar: nwg::ProgressBar,
    detail_label: nwg::RichLabel,
    history_label: nwg::RichLabel,
    side_label: nwg::RichLabel,
    side_slider: nwg::TrackBar,
    side_apply: nwg::Button,
    led_on: nwg::Button,
    led_off: nwg::Button,
    beep1: nwg::Button,
    beep2: nwg::Button,
    refresh_btn: nwg::Button,
    auto_side_btn: nwg::Button,
    autostart_btn: nwg::Button,
    status: nwg::RichLabel,
    timer: nwg::Timer,
    app_timer: nwg::Timer,
    // FreeBuds (Huawei) — arka plan izleği + Notice ile güncellenir
    fb_notice: nwg::Notice,
    fb_cmd: RefCell<Option<mpsc::Sender<FbCommand>>>,
    fb_snap: RefCell<FbSnapshot>,
    fb_title: nwg::RichLabel,
    fb_batt: nwg::RichLabel,
    fb_anc: nwg::RichLabel,
    fb_next: nwg::Button,
    fb_latency: nwg::Button,
    fb_eq: nwg::ComboBox<&'static str>,
    fb_toggle: nwg::Button,
    fb_reload: nwg::Button,
    fb_launch: nwg::Button,
    // tepsi
    tray_icon: RefCell<nwg::Icon>,
    tray: nwg::TrayNotification,
    tray_menu: nwg::Menu,
    tray_show: nwg::MenuItem,
    tray_exit: nwg::MenuItem,
    // durum
    last_level: Cell<u8>,
    low_warned: Cell<bool>,
    auto_side: Cell<bool>,
    last_exe: RefCell<String>,
    last_side_applied: Cell<u16>,
    user_led_off: Cell<bool>,
    auto_led_off: Cell<bool>,
}

impl Gui {
    fn update_autostart_btn(&self) {
        let on = autostart_on();
        self.autostart_btn.set_text(if on {
            "Oto-Başlat: AÇIK ✓"
        } else {
            "Oto-Başlat: KAPALI"
        });
    }

    fn update_autoside_btn(&self) {
        self.auto_side_btn.set_text(if self.auto_side.get() {
            "Oto-Sidetone: AÇIK ✓"
        } else {
            "Oto-Sidetone: KAPALI"
        });
    }

    fn show_window(&self) {
        self.window.set_visible(true);
        self.window.restore();
        self.window.set_focus();
    }

    fn update_tray(&self, level: u8, status: u8) {
        self.tray
            .set_tip(&format!("Voidctl — Pil {}% ({})", level, durum_adi(status)));
        if self.last_level.get() != level {
            write_tray_ico(level);
            if let Ok(ico) = nwg::Icon::from_file(
                &std::env::temp_dir().join(ICO_TMP).to_string_lossy(),
                true,
            ) {
                self.tray.set_icon(&ico);
                *self.tray_icon.borrow_mut() = ico;
            }
            self.last_level.set(level);
        }
    }

    fn refresh(&self) {
        let res = Void::connect().and_then(|d| d.battery(3));
        match res {
            Ok(b) => {
                rl_set(
                    &self.battery_label,
                    &format!("Pil  ~{}%", b.level),
                    batt_color(b.level),
                );
                self.battery_bar.set_pos(b.level as u32);
                self.battery_bar.set_state(if b.level < 20 {
                    nwg::ProgressBarState::Error
                } else if b.level < 50 {
                    nwg::ProgressBarState::Paused
                } else {
                    nwg::ProgressBarState::Normal
                });
                let mic = if b.mic_up {
                    "mikrofon yukarıda (kapalı)"
                } else {
                    "mikrofon aşağıda (açık)"
                };
                rl_set(
                    &self.detail_label,
                    &format!(
                        "{} · {} · ölçüm {}-{}%",
                        durum_adi(b.status),
                        mic,
                        b.min,
                        b.max
                    ),
                    TEXT_DIM,
                );
                rl_set(&self.status, "bağlı ✓", GOOD);
                self.update_tray(b.level, b.status);

                history::append(b.level);
                match history::summary(b.level) {
                    Some(s) => rl_set(&self.history_label, &s, TEXT_DIM),
                    None => rl_set(
                        &self.history_label,
                        "Geçmiş: veri toplanıyor (birkaç saat)…",
                        TEXT_DIM,
                    ),
                }

                let charging = b.status == 4 || b.status == 5;
                if b.level < 20 && !charging {
                    if !self.low_warned.get() {
                        self.low_warned.set(true);
                        self.tray.show(
                            "Pil %20'nin altında — şarj tak!",
                            Some("Voidctl"),
                            None,
                            None,
                        );
                        let _ = Void::connect().and_then(|d| d.beep(0));
                    }
                } else if charging {
                    self.low_warned.set(false);
                }

                if b.level < 30 && !self.user_led_off.get() && !self.auto_led_off.get() {
                    if Void::connect().and_then(|d| d.set_led(false)).is_ok() {
                        self.auto_led_off.set(true);
                        self.tray.show(
                            "Pil %30 altında — LED kapatıldı (tasarruf)",
                            Some("Voidctl"),
                            None,
                            None,
                        );
                    }
                } else if b.level >= 50 && self.auto_led_off.get() {
                    if Void::connect().and_then(|d| d.set_led(true)).is_ok() {
                        self.auto_led_off.set(false);
                    }
                }
            }
            Err(e) => {
                rl_set(&self.battery_label, "Pil  —", TEXT_DIM);
                self.battery_bar.set_pos(0);
                rl_set(&self.detail_label, "—", TEXT_DIM);
                rl_set(&self.status, &e, BAD);
                self.tray.set_tip(&format!("Voidctl — {}", e));
            }
        }
    }

    fn check_app_sidetone(&self) {
        if !self.auto_side.get() {
            return;
        }
        let Some(exe) = profiles::foreground_exe() else {
            return;
        };
        let last = self.last_exe.borrow().clone();
        if exe == last {
            return;
        }
        *self.last_exe.borrow_mut() = exe.clone();
        let rules = profiles::load_rules();
        let Some(level) = profiles::level_for(&rules, &exe) else {
            return;
        };
        if level != self.last_side_applied.get() {
            if let Ok(dev) = Void::connect() {
                if dev.set_sidetone(level).is_ok() {
                    self.last_side_applied.set(level);
                    self.side_slider.set_pos(level as usize);
                    rl_set(&self.side_label, &format!("Sidetone: {level}"), TEXT_MAIN);
                    rl_set(&self.status, &format!("⚡ sidetone {level} ({exe})"), GOOD);
                }
            }
        }
    }

    fn act(&self, what: &str, f: impl FnOnce(&Void) -> Result<(), String>) {
        match Void::connect() {
            Ok(dev) => match f(&dev) {
                Ok(()) => rl_set(&self.status, &format!("{what}: tamam ✓"), GOOD),
                Err(e) => rl_set(&self.status, &format!("{what}: {e}"), BAD),
            },
            Err(e) => rl_set(&self.status, &e, BAD),
        }
    }

    // ---------- FreeBuds (Huawei) ----------

    /// Arka plan izleğini başlatır: komut kanalı + Notice.
    /// Tüm RPC çağrıları izlekte yapılır, UI işleği asla kilitlenmez.
    fn start_fb_worker(&self) {
        use std::sync::mpsc::channel;
        let sender = self.fb_notice.sender();
        let (tx, rx) = channel::<FbCommand>();
        *self.fb_cmd.borrow_mut() = Some(tx);
        // açılışta hemen bir kez topla ki bölüm boş kalmasın
        std::thread::spawn(move || fb_worker_loop(rx, sender));
        self.fb_cmd(FbCommand::Refresh);
    }

    fn fb_cmd(&self, cmd: FbCommand) {
        if let Some(tx) = self.fb_cmd.borrow().as_ref() {
            let _ = tx.send(cmd);
        }
    }

    /// Notice geldi VEYA zamanlayıcı turladı → bekleyen anlık varsa UI'ye uygula.
    /// (Çift yol: Notice olayı bir şekilde gelmezse 30 sn'de timer yakalar.)
    fn fb_poll(&self) {
        if let Some(snap) = SNAP.lock().ok().and_then(|mut s| s.take()) {
            self.fb_apply(snap);
        }
    }

    fn fb_apply(&self, snap: FbSnapshot) {
        *self.fb_snap.borrow_mut() = snap.clone();
        if !snap.reachable {
            rl_set(
                &self.fb_title,
                "Huawei FreeBuds — OpenFreebuds çalışmıyor",
                TEXT_DIM,
            );
            rl_set(&self.fb_batt, "—", TEXT_DIM);
            rl_set(&self.fb_anc, &snap.err, TEXT_DIM);
            self.fb_next.set_enabled(false);
            self.fb_latency.set_enabled(false);
            self.fb_eq.set_enabled(false);
            self.fb_toggle.set_enabled(false);
            self.fb_launch.set_text("OFB Aç");
            return;
        }
        let title = if snap.device_name.is_empty() {
            format!("Huawei FreeBuds — {}", freebuds::state_text(snap.state_code))
        } else {
            format!("{} — {}", snap.device_name, freebuds::state_text(snap.state_code))
        };
        rl_set(
            &self.fb_title,
            &title,
            if snap.connected { GOOD } else { WARN },
        );
        let batt_txt = if snap.battery_line.is_empty() {
            "Pil: —".to_string()
        } else {
            format!("Pil: {}", snap.battery_line)
        };
        rl_set(&self.fb_batt, &batt_txt, TEXT_MAIN);
        if snap.has_anc && !snap.anc_mode.is_empty() {
            rl_set(
                &self.fb_anc,
                &format!("ANC: {}", snap.anc_mode),
                TEXT_MAIN,
            );
        } else {
            rl_set(&self.fb_anc, "ANC: —", TEXT_DIM);
        }
        let on = snap.connected;
        self.fb_next.set_enabled(on && snap.has_anc);
        self.fb_latency.set_enabled(on);
        self.fb_toggle.set_enabled(true);
        // EQ listesini tazele (seçim korunur)
        if snap.has_eq {
            self.fb_eq.set_enabled(on);
            let cur: Vec<String> = self
                .fb_eq
                .collection()
                .iter()
                .map(|s| s.to_string())
                .collect();
            if cur != snap.eq_presets {
                while self.fb_eq.len() > 0 {
                    self.fb_eq.remove(0);
                }
                // ComboBox<&str> 'static ister: sızıntıyla sabitle (küçük, sabit liste)
                for p in &snap.eq_presets {
                    let leaked: &'static str = Box::leak(p.clone().into_boxed_str());
                    self.fb_eq.push(leaked);
                }
                if let Some(i) = snap.eq_presets.iter().position(|p| *p == snap.eq_current) {
                    self.fb_eq.set_selection(Some(i));
                }
            }
        } else {
            self.fb_eq.set_enabled(false);
        }
        self.fb_latency.set_text(if snap.low_latency {
            "Düşük Gecikme: AÇIK ✓"
        } else {
            "Düşük Gecikme"
        });
        self.fb_toggle.set_text(if snap.connected {
            "Bağlantıyı Kes"
        } else {
            "Bağlan"
        });
        // OFB ayaktaysa buton ayar penceresi açar, değilse programı başlatır
        self.fb_launch.set_text("OFB Ayarları");
    }

    fn fb_refresh(&self) {
        self.fb_cmd(FbCommand::Refresh);
        rl_set(&self.fb_anc, "güncelleniyor…", TEXT_DIM);
    }
}

static SNAP: std::sync::Mutex<Option<FbSnapshot>> = std::sync::Mutex::new(None);

/// Arka plan izlek gövdesi: komut uygula → topla → Notice.
fn fb_worker_loop(rx: mpsc::Receiver<FbCommand>, sender: nwg::NoticeSender) {
    let (url, secret) = freebuds::load_config();
    let client = freebuds::OfbClient::new(&url, &secret);
    loop {
        if let Ok(cmd) = rx.recv_timeout(std::time::Duration::from_secs(30)) {
            apply_fb_command(&client, cmd);
        }
        let snap = freebuds::collect(&client);
        if let Ok(mut slot) = SNAP.lock() {
            *slot = Some(snap);
        }
        sender.notice();
    }
}

fn apply_fb_command(client: &freebuds::OfbClient, cmd: FbCommand) {
    match cmd {
        FbCommand::Refresh => {}
        FbCommand::Shortcut(name) => {
            let _ = client.shortcut(&name);
        }
        FbCommand::SetProp { group, prop, value } => {
            let _ = client.set_prop(&group, &prop, &value);
        }
    }
}

fn build(data: &mut Gui) -> Result<(), nwg::NwgError> {
    nwg::Font::builder()
        .size(22)
        .weight(700)
        .family("Segoe UI")
        .build(&mut data.big_font)?;

    nwg::Window::builder()
        .flags(nwg::WindowFlags::WINDOW | nwg::WindowFlags::VISIBLE)
        .size((400, 640))
        .center(true)
        .title(WINDOW_TITLE)
        .build(&mut data.window)?;

    nwg::RichLabel::builder()
        .text("Pil  —")
        .font(Some(&data.big_font))
        .background_color(Some(BG))
        .position((16, 12))
        .size((368, 44))
        .line_height(Some(38))
        .parent(&data.window)
        .build(&mut data.battery_label)?;

    nwg::ProgressBar::builder()
        .range(0..101)
        .pos(0)
        .position((16, 64))
        .size((368, 18))
        .parent(&data.window)
        .build(&mut data.battery_bar)?;

    nwg::RichLabel::builder()
        .text("—")
        .background_color(Some(BG))
        .position((16, 92))
        .size((368, 20))
        .parent(&data.window)
        .build(&mut data.detail_label)?;

    nwg::RichLabel::builder()
        .text("Geçmiş: …")
        .background_color(Some(BG))
        .position((16, 112))
        .size((368, 18))
        .parent(&data.window)
        .build(&mut data.history_label)?;

    nwg::RichLabel::builder()
        .text("Sidetone: 90")
        .background_color(Some(BG))
        .position((16, 136))
        .size((220, 20))
        .parent(&data.window)
        .build(&mut data.side_label)?;

    nwg::TrackBar::builder()
        .range(Some(0..129))
        .pos(Some(90))
        .position((16, 158))
        .size((268, 26))
        .parent(&data.window)
        .build(&mut data.side_slider)?;

    nwg::Button::builder()
        .text("Uygula")
        .position((294, 154))
        .size((90, 28))
        .parent(&data.window)
        .build(&mut data.side_apply)?;

    nwg::Button::builder()
        .text("LED Aç")
        .position((16, 196))
        .size((118, 32))
        .parent(&data.window)
        .build(&mut data.led_on)?;

    nwg::Button::builder()
        .text("LED Kapat")
        .position((142, 196))
        .size((118, 32))
        .parent(&data.window)
        .build(&mut data.led_off)?;

    nwg::Button::builder()
        .text("Yenile")
        .position((268, 196))
        .size((116, 32))
        .parent(&data.window)
        .build(&mut data.refresh_btn)?;

    nwg::Button::builder()
        .text("Bip 1")
        .position((16, 236))
        .size((118, 32))
        .parent(&data.window)
        .build(&mut data.beep1)?;

    nwg::Button::builder()
        .text("Bip 2")
        .position((142, 236))
        .size((118, 32))
        .parent(&data.window)
        .build(&mut data.beep2)?;

    nwg::Button::builder()
        .text("Oto-Sidetone: …")
        .position((16, 276))
        .size((184, 32))
        .parent(&data.window)
        .build(&mut data.auto_side_btn)?;

    nwg::Button::builder()
        .text("Otomatik Başlat: …")
        .position((208, 276))
        .size((176, 32))
        .parent(&data.window)
        .build(&mut data.autostart_btn)?;

    nwg::RichLabel::builder()
        .text("")
        .background_color(Some(BG))
        .position((16, 318))
        .size((368, 22))
        .parent(&data.window)
        .build(&mut data.status)?;

    // ---------- FreeBuds (Huawei) bölümü ----------
    nwg::RichLabel::builder()
        .text("Huawei FreeBuds")
        .font(Some(&data.big_font))
        .background_color(Some(BG))
        .position((16, 352))
        .size((368, 40))
        .parent(&data.window)
        .build(&mut data.fb_title)?;

    nwg::RichLabel::builder()
        .text("—")
        .background_color(Some(BG))
        .position((16, 396))
        .size((368, 20))
        .parent(&data.window)
        .build(&mut data.fb_batt)?;

    nwg::RichLabel::builder()
        .text("ANC: —")
        .background_color(Some(BG))
        .position((16, 418))
        .size((368, 20))
        .parent(&data.window)
        .build(&mut data.fb_anc)?;

    nwg::Button::builder()
        .text("ANC Değiştir")
        .position((16, 446))
        .size((118, 32))
        .parent(&data.window)
        .build(&mut data.fb_next)?;

    nwg::Button::builder()
        .text("Düşük Gecikme")
        .position((142, 446))
        .size((126, 32))
        .parent(&data.window)
        .build(&mut data.fb_latency)?;

    nwg::Button::builder()
        .text("Bağlan")
        .position((276, 446))
        .size((108, 32))
        .parent(&data.window)
        .build(&mut data.fb_toggle)?;

    nwg::Button::builder()
        .text("Yenile")
        .position((276, 522))
        .size((108, 32))
        .parent(&data.window)
        .build(&mut data.fb_reload)?;

    nwg::ComboBox::builder()
        .collection(vec!["—"])
        .position((16, 486))
        .size((252, 26))
        .parent(&data.window)
        .build(&mut data.fb_eq)?;

    nwg::Button::builder()
        .text("OFB Aç")
        .position((276, 482))
        .size((108, 32))
        .parent(&data.window)
        .build(&mut data.fb_launch)?;

    nwg::Notice::builder()
        .parent(&data.window)
        .build(&mut data.fb_notice)?;

    nwg::Timer::builder()
        .parent(&data.window)
        .interval(UPDATE_MS)
        .build(&mut data.timer)?;

    nwg::Timer::builder()
        .parent(&data.window)
        .interval(APP_MS)
        .build(&mut data.app_timer)?;

    // tepsi ikonu
    write_tray_ico(99);
    nwg::Icon::builder()
        .source_file(Some(
            &std::env::temp_dir().join(ICO_TMP).to_string_lossy(),
        ))
        .build(&mut data.tray_icon.borrow_mut())?;

    nwg::TrayNotification::builder()
        .parent(&data.window)
        .icon(Some(&data.tray_icon.borrow()))
        .tip(Some("Voidctl"))
        .visible(true)
        .build(&mut data.tray)?;

    nwg::Menu::builder()
        .popup(true)
        .parent(&data.window)
        .build(&mut data.tray_menu)?;

    nwg::MenuItem::builder()
        .text("Göster")
        .parent(&data.tray_menu)
        .build(&mut data.tray_show)?;

    nwg::MenuItem::builder()
        .text("Çıkış")
        .parent(&data.tray_menu)
        .build(&mut data.tray_exit)?;

    Ok(())
}

fn main() {
    if bring_existing_forward() {
        return;
    }
    let hidden = std::env::args().any(|a| a == "--hidden");

    enable_dark_app_mode();
    nwg::init().expect("Native Windows GUI başlatılamadı");
    nwg::Font::set_global_family("Segoe UI").ok();

    let mut gui = Gui::default();
    build(&mut gui).expect("UI oluşturulamadı");

    // koyu çerçeve + koyu zemin
    if let Some(h) = gui.window.handle.hwnd() {
        dark_frame(h);
    }
    dark_controls(&gui);
    let _bg_main = dark_bg_handler(&gui.window.handle, 0x1000_0001);

    gui.update_autostart_btn();
    gui.auto_side.set(true);
    gui.update_autoside_btn();
    gui.last_side_applied.set(90);
    if hidden {
        gui.window.set_visible(false);
    }
    gui.refresh();
    gui.start_fb_worker();

    let app = std::rc::Rc::new(gui);
    let win = app.clone();
    let window_handle = app.window.handle;

    let _events = nwg::full_bind_event_handler(&window_handle, move |evt, _evt_data, handle| {
        use nwg::Event as E;
        use nwg::MousePressEvent as MP;
        let app = win.clone();
        match evt {
            E::OnWindowClose => {
                if handle == window_handle {
                    app.window.set_visible(false);
                }
            }
            E::OnButtonClick => {
                if &handle == &app.side_apply {
                    let level = app.side_slider.pos() as u16;
                    rl_set(&app.side_label, &format!("Sidetone: {level}"), TEXT_MAIN);
                    app.last_side_applied.set(level);
                    app.act("sidetone", move |d| d.set_sidetone(level));
                } else if &handle == &app.led_on {
                    app.user_led_off.set(false);
                    app.auto_led_off.set(false);
                    app.act("LED", |d| d.set_led(true));
                } else if &handle == &app.led_off {
                    app.user_led_off.set(true);
                    app.act("LED", |d| d.set_led(false));
                } else if &handle == &app.beep1 {
                    app.act("bip", |d| d.beep(0));
                } else if &handle == &app.beep2 {
                    app.act("bip", |d| d.beep(1));
                } else if &handle == &app.refresh_btn {
                    app.refresh();
                } else if &handle == &app.auto_side_btn {
                    let v = !app.auto_side.get();
                    app.auto_side.set(v);
                    app.update_autoside_btn();
                    if v {
                        app.check_app_sidetone();
                    }
                } else if &handle == &app.autostart_btn {
                    let now_on = !autostart_on();
                    if autostart_set(now_on) {
                        app.update_autostart_btn();
                        rl_set(
                            &app.status,
                            if now_on {
                                "Otomatik başlatma: AÇIK ✓"
                            } else {
                                "Otomatik başlatma: KAPALI"
                            },
                            GOOD,
                        );
                    } else {
                        rl_set(&app.status, "Otomatik başlatma ayarlanamadı!", BAD);
                    }
                } else if &handle == &app.fb_next {
                    app.fb_cmd(FbCommand::Shortcut("next_mode".into()));
                    rl_set(&app.fb_anc, "ANC değiştiriliyor…", TEXT_DIM);
                } else if &handle == &app.fb_latency {
                    let on = !app.fb_snap.borrow().low_latency;
                    app.fb_cmd(FbCommand::SetProp {
                        group: "config".into(),
                        prop: "low_latency".into(),
                        value: if on { "true" } else { "false" }.into(),
                    });
                } else if &handle == &app.fb_toggle {
                    let connected = app.fb_snap.borrow().connected;
                    app.fb_cmd(FbCommand::Shortcut(if connected {
                        "disconnect".into()
                    } else {
                        "connect".into()
                    }));
                    rl_set(&app.fb_anc, "bağlantı değişiyor…", TEXT_DIM);
                } else if &handle == &app.fb_reload {
                    app.fb_refresh();
                } else if &handle == &app.fb_launch {
                    if app.fb_snap.borrow().reachable {
                        // OFB çalışıyor: ikinci kopya değil, ayar penceresini aç
                        app.fb_cmd(FbCommand::Shortcut("show_main_window".into()));
                        rl_set(&app.status, "OpenFreebuds ayarları açılıyor…", GOOD);
                    } else {
                        launch_ofb(&app.status);
                        // OFB yeni açılıyorsa birkaç sn sonra topla
                        app.fb_cmd(FbCommand::Refresh);
                    }
                }
            }
            E::OnHorizontalScroll => {
                if &handle == &app.side_slider {
                    let level = app.side_slider.pos();
                    rl_set(&app.side_label, &format!("Sidetone: {level}"), TEXT_MAIN);
                }
            }
            E::OnTimerTick => {
                if &handle == &app.timer {
                    app.refresh();
                    app.fb_poll();
                } else if &handle == &app.app_timer {
                    app.check_app_sidetone();
                }
            }
            E::OnNotice => {
                if &handle == &app.fb_notice {
                    app.fb_poll();
                }
            }
            E::OnComboBoxClosed => {
                if &handle == &app.fb_eq {
                    if let Some(sel) = app.fb_eq.selection_string() {
                        let want = sel.trim().to_string();
                        if !want.is_empty() && want != app.fb_snap.borrow().eq_current {
                            app.fb_cmd(FbCommand::SetProp {
                                group: "sound".into(),
                                prop: "equalizer_preset".into(),
                                value: want,
                            });
                        }
                    }
                }
            }
            E::OnMousePress(MP::MousePressLeftUp) => {
                if &handle == &app.tray.handle {
                    app.show_window();
                }
            }
            E::OnContextMenu => {
                if &handle == &app.tray.handle {
                    let (x, y) = cursor_pos();
                    app.tray_menu.popup(x, y);
                }
            }
            E::OnMenuItemSelected => {
                if &handle == &app.tray_show {
                    app.show_window();
                } else if &handle == &app.tray_exit {
                    nwg::stop_thread_dispatch();
                }
            }
            _ => {}
        }
    });

    nwg::dispatch_thread_events();
}
