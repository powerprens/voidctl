// voidctl CLI — Corsair Void kulaklık kontrolü (iCUE'süz)
// Protokol katmanı: src/lib.rs

use std::process::exit;
use voidctl::{durum_adi, list_interfaces, Void};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("battery") => battery(),
        Some("sidetone") => sidetone(args.get(2)),
        Some("led") => led(args.get(2).map(String::as_str)),
        Some("beep") => beep(args.get(2).map(String::as_str)),
        Some("otokapat") => otokapat(args.get(2).map(String::as_str)),
        Some("ffdev") => ffdev(args.get(2)),
        Some("watch") => watch(args.get(2)),
        Some("history") => history_cmd(),
        Some("fw") => fw_cmd(),
        Some("info") => info(),
        Some("serve") => serve_cmd(args.get(2)),
        Some("fb") => fb_cmd(),
        _ => {
            println!("voidctl — Corsair Void kulaklık kontrolü (iCUE'süz)\n");
            println!("Kullanım:");
            println!("  voidctl battery            pil + mikrofon durumu (5 ölçüm medyanı)");
            println!("  voidctl sidetone <0-128>   sidetone seviyesi (0 = kapalı)");
            println!("  voidctl led on|off         LED");
            println!("  voidctl beep [0|1]         bildirim sesi");
            println!("  voidctl otokapat on|off    hareketsizlikte otomatik kapanma (kulaklık hafızasına yazar)");
            println!("  voidctl watch [sn]         pil izle (varsayılan 60 sn; <%20 bip)");
            println!("  voidctl fw                 dongle + kulaklık firmware sürümü");
            println!("  voidctl info               HID arayüzlerini listele");
            println!("  voidctl fb                 Huawei FreeBuds durumu (OpenFreebuds RPC üzerinden)");
    println!("  voidctl serve [port]       yerel HTTP API (yalnızca 127.0.0.1, örn. 59312)");
    println!("\nGUI: voidgui.exe");
        }
    }
}

fn die<T>(msg: String) -> T {
    eprintln!("HATA: {msg}");
    exit(1);
}

fn battery() {
    let dev = Void::connect().unwrap_or_else(die);
    let b = dev.battery(5).unwrap_or_else(die);
    println!(
        "Pil: ~{}% ({})  (5 ölçüm: {}-{} arası)",
        b.level,
        durum_adi(b.status),
        b.min,
        b.max
    );
    println!(
        "Mikrofon: {}",
        if b.mic_up { "yukarıda (kapalı)" } else { "aşağıda (açık)" }
    );
}

fn sidetone(arg: Option<&String>) {
    let Some(level_str) = arg else {
        eprintln!("Kullanım: voidctl sidetone <0-128>");
        exit(1);
    };
    let Ok(level) = level_str.parse::<u16>() else {
        eprintln!("HATA: seviye 0-128 arası bir sayı olmalı.");
        exit(1);
    };
    let dev = Void::connect().unwrap_or_else(die);
    match dev.set_sidetone(level) {
        Ok(()) => println!("Sidetone {level} olarak ayarlandı."),
        Err(e) => die(e),
    }
}

fn led(arg: Option<&str>) {
    let on = match arg {
        Some("on") => true,
        Some("off") => false,
        _ => {
            eprintln!("Kullanım: voidctl led on|off");
            exit(1);
        }
    };
    let dev = Void::connect().unwrap_or_else(die);
    match dev.set_led(on) {
        Ok(()) => println!("LED {}.", if on { "açık" } else { "kapalı" }),
        Err(e) => die(e),
    }
}

fn beep(arg: Option<&str>) {
    let id: u8 = match arg.map(str::parse) {
        Some(Ok(v @ 0..=1)) => v,
        None => 0,
        _ => {
            eprintln!("Kullanım: voidctl beep [0|1]");
            exit(1);
        }
    };
    let dev = Void::connect().unwrap_or_else(die);
    match dev.beep(id) {
        Ok(()) => println!("Bildirim sesi ({id}) çalındı."),
        Err(e) => die(e),
    }
}

fn otokapat(arg: Option<&str>) {
    let on = match arg {
        Some("on") => true,
        Some("off") => false,
        _ => {
            eprintln!("Kullanım: voidctl otokapat on|off");
            exit(1);
        }
    };
    let dev = Void::connect().unwrap_or_else(die);
    match dev.set_auto_shutdown(on) {
        Ok(()) => println!(
            "Otomatik kapanma {} — iCUE'daki Auto-Shutdown anahtarını kontrol et, aynı durumu görmeli.",
            if on { "AÇIK" } else { "KAPALI" }
        ),
        Err(e) => die(e),
    }
}

fn ffdev(arg: Option<&String>) {
    let Some(hex) = arg else {
        eprintln!("Kullanım: voidctl ffdev <hex baytlar, örn. ff0800ff04000103700700c0>");
        exit(1);
    };
    let clean: String = hex.chars().filter(|c| !c.is_whitespace()).collect();
    let mut data = Vec::new();
    let bytes = clean.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        let hi = (bytes[i] as char).to_digit(16);
        let lo = (bytes[i + 1] as char).to_digit(16);
        match (hi, lo) {
            (Some(h), Some(l)) => data.push((h * 16 + l) as u8),
            _ => {
                eprintln!("HATA: geçersiz hex: {}", &clean[i..i + 2]);
                exit(1);
            }
        }
        i += 2;
    }
    if data.is_empty() {
        eprintln!("HATA: boş veri");
        exit(1);
    }
    let dev = Void::connect().unwrap_or_else(die);
    match dev.send_ff_raw(&data) {
        Ok(()) => println!("FF raporu gönderildi ({} bayt): {}", data.len(), clean),
        Err(e) => die(e),
    }
}

fn watch(arg: Option<&String>) {
    let secs: u64 = arg
        .and_then(|s| s.parse().ok())
        .filter(|s| *s >= 5)
        .unwrap_or(60);
    println!("İzleme başladı: {secs} sn'de bir pil okunacak, <%20'de bip atacak. Durdurmak için Ctrl+C.");
    let start = std::time::Instant::now();
    let mut dev = Void::connect().ok();
    loop {
        let elapsed = start.elapsed().as_secs();
        let ts = format!("{:02}:{:02}", elapsed / 60, elapsed % 60);
        if dev.is_none() {
            dev = Void::connect().ok();
        }
        match dev.as_ref().and_then(|d| d.battery(1).ok()) {
            Some(b) => {
                println!("[{ts}] Pil {}% ({})", b.level, durum_adi(b.status));
                if b.level < 20 && b.status != 4 && b.status != 5 {
                    println!("[{ts}] ⚠ PİL ZAYIF — şarj tak!");
                    let _ = dev.as_ref().map(|d| d.beep(0));
                }
            }
            None => {
                println!("[{ts}] Kulaklık bağlı değil");
                dev = None;
            }
        }
        std::thread::sleep(std::time::Duration::from_secs(secs));
    }
}

fn info() {
    let rows = list_interfaces();
    if rows.is_empty() {
        println!("Corsair (VID 1b1c) HID cihazı görünmüyor.");
    } else {
        for r in rows {
            println!("{r}");
        }
    }
}

fn history_cmd() {
    let dev = Void::connect().unwrap_or_else(die);
    let b = dev.battery(3).unwrap_or_else(die);
    match voidctl::history::summary(b.level) {
        Some(s) => println!("{s}"),
        None => println!("Geçmiş verisi yetersiz (birkaç saat veri gerekli)."),
    }
}

fn fb_cmd() {
    let (url, secret) = voidctl::freebuds::load_config();
    let c = voidctl::freebuds::OfbClient::new(&url, &secret);
    if !c.ping() {
        eprintln!("HATA: OpenFreebuds çalışmıyor ({url}).");
        exit(1);
    }
    let s = voidctl::freebuds::collect(&c);
    println!(
        "Cihaz:  {}",
        if s.device_name.is_empty() {
            "?"
        } else {
            &s.device_name
        }
    );
    println!(
        "Durum:  {} (kod {})",
        voidctl::freebuds::state_text(s.state_code),
        s.state_code
    );
    if s.battery_line.is_empty() {
        println!("Pil:    —");
    } else {
        println!("Pil:    {}", s.battery_line);
    }
    println!(
        "ANC:    {}  [{}]",
        if s.anc_mode.is_empty() { "—" } else { &s.anc_mode },
        s.anc_options.join(",")
    );
    println!(
        "EQ:     {}  [{}]",
        if s.eq_current.is_empty() { "—" } else { &s.eq_current },
        s.eq_presets.join(",")
    );
    println!("Düşük gecikme: {}", if s.low_latency { "AÇIK" } else { "KAPALI" });
}

fn serve_cmd(arg: Option<&String>) {
    let port: u16 = arg
        .and_then(|s| s.parse().ok())
        .unwrap_or(voidctl::api::DEFAULT_PORT);
    let poll: u64 = std::env::var("VOIDCTL_POLL_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(voidctl::api::DEFAULT_POLL_SECS);
    voidctl::api::serve(port, poll);
}

fn fw_cmd() {
    let dev = Void::connect().unwrap_or_else(die);
    let f = dev.firmware().unwrap_or_else(die);
    println!("Dongle  firmware: {}.{}", f.dongle.0, f.dongle.1);
    if f.headset.0 == 0 && f.headset.1 == 0 {
        println!("Kulaklık firmware: bağlı değil (dongle açık görmüyor)");
    } else {
        println!("Kulaklık firmware: {}.{}", f.headset.0, f.headset.1);
    }
}
