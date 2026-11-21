// voidctl — Corsair Void kulaklık kontrol kütüphanesi
// Protokol: HeadSetControl (https://github.com/Sapd/HeadsetControl)
// lib/devices/corsair_void_rich.hpp + corsair_device.hpp referans alınmıştır.

use hidapi::HidApi;

pub mod api;
pub mod freebuds;
pub mod history;
pub mod profiles;

pub const VID: u16 = 0x1B1C;
pub const PIDS: &[u16] = &[
    0x1b1c, 0x1b27, 0x0a14, 0x0a16, 0x0a17, 0x0a1d, 0x0a1a, 0x1b2a, 0x1b23, 0x1b29, 0x0a55, 0x0a51,
    0x0a52, 0x0a38, 0x0a4f, 0x0a2b, 0x0a75, 0x0a56,
];
pub const USAGE_CTRL: u16 = 0xFFC5; // pil / LED / bip  (interface 3)
pub const USAGE_SIDETONE: u16 = 0xFF00; // sidetone feature report (interface 0)

#[derive(Debug, Clone, Copy)]
pub struct Battery {
    pub level: u8,  // medyan (0-100)
    pub min: u8,
    pub max: u8,
    pub status: u8, // 1=normal 2=zayıf 4/5=şarj
    pub mic_up: bool,
}

pub fn durum_adi(status: u8) -> &'static str {
    match status {
        4 | 5 => "şarj oluyor",
        2 => "pil zayıf",
        _ => "normal",
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Firmware {
    pub dongle: (u8, u8),   // (major, minor)
    pub headset: (u8, u8),  // kulaklık bağlı değilse (0, 0)
}

/// Bağlı Void cihazı. Her işlemden önce yeniden açmak da güvenlidir
/// (dongle tak-çık sonrası eski handle'lar ölür).
pub struct Void {
    _api: HidApi,
    ctrl: hidapi::HidDevice,
    side: Option<hidapi::HidDevice>,
}

impl Void {
    pub fn connect() -> Result<Void, String> {
        let api = HidApi::new().map_err(|e| format!("HID API başlatılamadı: {e}"))?;

        let find = |usage: u16| -> Option<std::ffi::CString> {
            api.device_list()
                .filter(|d| d.vendor_id() == VID && PIDS.contains(&d.product_id()))
                .find(|d| d.usage_page() == usage)
                .map(|d| d.path().to_owned())
        };

        let ctrl_path = find(USAGE_CTRL).ok_or("Dongle bulunamadı (kulaklık kapalı olabilir). 'voidctl info' ile bak.")?;
        let ctrl = api
            .open_path(&ctrl_path)
            .map_err(|e| format!("HID arayüzü açılamadı (ffc5): {e}"))?;
        let side = find(USAGE_SIDETONE)
            .and_then(|p| api.open_path(&p).ok());

        Ok(Void { _api: api, ctrl, side })
    }

    fn battery_once(&self) -> Result<(u8, u8, bool), String> {
        self.ctrl
            .write(&[0xC9, 0x64])
            .map_err(|e| format!("pil sorgusu yazılamadı: {e}"))?;
        let mut resp = [0u8; 5];
        let n = self
            .ctrl
            .read_timeout(&mut resp, 2000)
            .map_err(|e| format!("pil cevabı okunamadı: {e}"))?;
        if n < 5 {
            return Err("kulaklık dongle'a bağlı değil".into());
        }
        let status = resp[4];
        if status == 0 {
            return Err("kulaklık dongle'a bağlı değil".into());
        }
        let mut level = resp[2];
        let mic_up = level & 0x80 != 0;
        level &= 0x7F;
        Ok((level, status, mic_up))
    }

    /// `samples` adet ölçüm alıp medyan döndürür (dongle'ın pil baytı ±10 oynuyor).
    pub fn battery(&self, samples: usize) -> Result<Battery, String> {
        let mut levels: Vec<u8> = Vec::new();
        let mut status = 1u8;
        let mut mic_up = false;
        for i in 0..samples.max(1) {
            let (level, st, mic) = self.battery_once()?;
            levels.push(level);
            status = st;
            mic_up = mic_up || mic;
            if i + 1 < samples {
                std::thread::sleep(std::time::Duration::from_millis(150));
            }
        }
        levels.sort_unstable();
        let mid = levels.len() / 2;
        Ok(Battery {
            level: levels[mid],
            min: levels[0],
            max: levels[levels.len() - 1],
            status,
            mic_up,
        })
    }

    pub fn set_led(&self, on: bool) -> Result<(), String> {
        // Ters mantık: 0x00 = açık, 0x01 = kapalı
        let val: u8 = if on { 0x00 } else { 0x01 };
        self.ctrl
            .write(&[0xC8, val, 0x00])
            .map(|_| ())
            .map_err(|e| format!("LED ayarlanamadı: {e}"))
    }

    pub fn set_sidetone(&self, level: u16) -> Result<(), String> {
        if level > 128 {
            return Err("seviye 0-128 arası olmalı".into());
        }
        let dev = self.side.as_ref().ok_or("sidetone arayüzü açılamadı (ff00)")?;
        // 0-128 -> cihaz aralığı 200-255
        let mapped: u8 = (200 + (level as u32 * 55) / 128) as u8;
        let mut buf = [0u8; 64];
        buf[..12].copy_from_slice(&[0xFF, 0x0B, 0x00, 0xFF, 0x04, 0x0E, 0xFF, 0x05, 0x01, 0x04, 0x00, mapped]);
        dev.send_feature_report(&buf)
            .map(|_| ())
            .map_err(|e| format!("sidetone ayarlanamadı: {e}"))
    }

    pub fn beep(&self, id: u8) -> Result<(), String> {
        if id > 1 {
            return Err("ses kimliği 0 veya 1 olmalı".into());
        }
        self.ctrl
            .write(&[0xCA, 0x02, id])
            .map(|_| ())
            .map_err(|e| format!("bildirim sesi gönderilemedi: {e}"))
    }

    /// Otomatik kapanma (auto-shutdown) aç/kapa.
    /// Protokol USBPcap ile iCUE'dan yakalandı (2026-11-17):
    /// SET_REPORT(Feature, rapor 2, interface 3) = [0xCA, 0x04, 01(aç)/00(kapa), 0x00, 0x00]
    /// Kulaklığın KENDİ zamanlayıcısıdır; iCUE çalışmasa da geçerlidir.
    pub fn set_auto_shutdown(&self, on: bool) -> Result<(), String> {
        let v: u8 = if on { 0x01 } else { 0x00 };
        self.ctrl
            .write(&[0xCA, 0x04, v, 0x00, 0x00])
            .map(|_| ())
            .map_err(|e| format!("otomatik kapanma ayarlanamadı: {e}"))
    }

    /// DENEME: ham FF ayar raporu (64 bayta pad'lenir).
    /// iCUE'nun kablosu: SetReport(Feature, ID=FF, interface 3) + 64 bayt data.
    /// HID katmanı boyut kısıtladığı için birden fazla yol dener.
    pub fn send_ff_raw(&self, data: &[u8]) -> Result<(), String> {
        let mut buf = [0u8; 64];
        let n = data.len().min(64);
        buf[..n].copy_from_slice(&data[..n]);
        let mut errs: Vec<String> = Vec::new();

        // 1) ctrl interrupt OUT (64)
        if self.ctrl.write(&buf).is_ok() {
            return Ok(());
        } else {
            errs.push("ctrl.write".into());
        }
        // 2) ctrl SetFeature [ID + 64] = 65
        let mut f65 = [0u8; 65];
        f65[0] = 0xFF;
        f65[1..].copy_from_slice(&buf);
        if self.ctrl.send_feature_report(&f65).is_ok() {
            return Ok(());
        } else {
            errs.push("ctrl.feature(65)".into());
        }
        // 3) sidetone (FF00) arayüzunden SetFeature 65
        if let Some(side) = self.side.as_ref() {
            if side.send_feature_report(&f65).is_ok() {
                return Ok(());
            }
            errs.push("side.feature(65)".into());
            // 4) sidetone interrupt OUT 64
            if side.write(&buf).is_ok() {
                return Ok(());
            }
            errs.push("side.write".into());
        }
        Err(format!("tum yollar basarisiz: {}", errs.join(", ")))
    }

    /// Donanım yazılımı sürümü — Linux hid-corsair-void sürücüsündeki protokol:
    /// istek [0xC9, 0x66]; cevap 5 bayt:
    /// [0x66, dongle_major, dongle_minor, kulaklık_major, kulaklık_minor]
    pub fn firmware(&self) -> Result<Firmware, String> {
        self.ctrl
            .write(&[0xC9, 0x66])
            .map_err(|e| format!("fw sorgusu yazılamadı: {e}"))?;
        // Birikmiş pil raporu (0x64) gelmiş olabilir; 0x66'ya kadar oku.
        let mut resp = [0u8; 5];
        for _ in 0..6 {
            let n = self
                .ctrl
                .read_timeout(&mut resp, 1000)
                .map_err(|e| format!("fw cevabı okunamadı: {e}"))?;
            if n >= 5 && resp[0] == 0x66 {
                return Ok(Firmware {
                    dongle: (resp[1], resp[2]),
                    headset: (resp[3], resp[4]),
                });
            }
        }
        Err("firmware cevabı alınamadı (0x66 raporu gelmedi)".into())
    }
}

/// `voidctl info` için: bağlı Corsair HID arayüzlerini listeler.
pub fn list_interfaces() -> Vec<String> {
    let Ok(api) = HidApi::new() else {
        return vec![];
    };
    api.device_list()
        .filter(|d| d.vendor_id() == VID)
        .map(|d| {
            format!(
                "PID {:#06x}  usage_page={:#06x} usage={:#06x} if={}",
                d.product_id(),
                d.usage_page(),
                d.usage(),
                d.interface_number()
            )
        })
        .collect()
}
