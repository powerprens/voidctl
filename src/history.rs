// Pil geçmişi — %LOCALAPPDATA%\voidctl\history.jsonl
// Analiz fonksiyonu saf tutuldu (test edilebilir); IO ayrı.

use std::path::PathBuf;

fn dir() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".into());
    std::path::PathBuf::from(base).join("voidctl")
}

pub fn append(level: u8) {
    use std::io::Write;
    let _ = std::fs::create_dir_all(dir());
    let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir().join("history.jsonl"))
    else {
        return;
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let _ = writeln!(f, "{{\"t\":{},\"v\":{}}}", now, level);
}

fn extract_num(line: &str, key: &str) -> Option<f64> {
    line.split(key).nth(1)?.trim_start().split([',', '}']).next()?.parse().ok()
}

/// Son 7 günün noktalarından günlük düşüş ve kalan süre tahmini.
pub fn summary_from(lines: &[String], now_ms: u64, cur_level: u8) -> Option<String> {
    let week_ms: u64 = 7 * 24 * 3_600_000;
    let mut pts: Vec<(u64, f64)> = Vec::new();
    for l in lines {
        let t = extract_num(l, "\"t\":");
        let v = extract_num(l, "\"v\":");
        if let (Some(t), Some(v)) = (t, v) {
            let t = t as u64;
            if now_ms.saturating_sub(t) <= week_ms {
                pts.push((t, v.clamp(0.0, 100.0)));
            }
        }
    }
    if pts.len() < 2 {
        return None;
    }
    pts.sort_by_key(|p| p.0);
    let (t0, v0) = pts[0];
    let (t1, v1) = *pts.last().unwrap();
    let hours = (t1 - t0) as f64 / 3_600_000.0;
    if hours < 1.0 {
        return None;
    }
    let drop = v0 - v1;
    if drop <= 0.0 {
        return Some(format!(
            "Geçmiş: {pt} ölçüm / {hours:.0} sa (şarj dolmuş, tahmin yok)",
            pt = pts.len()
        ));
    }
    let per_day = drop / (hours / 24.0);
    let remain_h = cur_level as f64 / (per_day / 24.0);
    Some(format!(
        "Geçmiş: ~%{per_day:.0}/gün düşüş · şu seviyeyle ~{remain_h:.0} sa kalır",
    ))
}

/// Canlı özet: dosyayı okuyıp bugünün seviyesiyle analiz eder.
pub fn summary(cur_level: u8) -> Option<String> {
    let txt = std::fs::read_to_string(dir().join("history.jsonl")).ok()?;
    let lines: Vec<String> = txt.lines().map(str::to_string).collect();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    summary_from(&lines, now, cur_level)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: u64 = 86_400_000;

    #[test]
    fn gunluk_dusus_ve_tahmin() {
        let now = 10_000_000_000_000u64;
        // geçmişten bugüne 100 → 25 (3 gün, günde %25 düşüş)
        let lines: Vec<String> = (0..4)
            .map(|i| {
                let t = now - (3 - i) as u64 * DAY;
                let v = 100 - i * 25;
                format!("{{\"t\":{},\"v\":{}}}", t, v)
            })
            .collect();
        let s = summary_from(&lines, now, 25).unwrap();
        assert!(s.contains("%25"), "günlük düşüş yanlış: {s}");
        assert!(s.contains("24"), "kalan saat yanlış: {s}");
    }

    #[test]
    fn eksik_veri_yok() {
        assert!(summary_from(&[], 0, 50).is_none());
        let single = vec![format!("{{\"t\":1,\"v\":50}}")];
        assert!(summary_from(&single, 0, 50).is_none());
    }

    #[test]
    fn bozuk_satirlar_atlanir() {
        let now = 5_000_000_000_000u64;
        let lines = vec![
            "çöp satır".into(),
            format!("{{\"t\":{},\"v\":100}}", now - 2 * DAY),
            format!("{{\"t\":{},\"v\":80}}", now),
        ];
        let s = summary_from(&lines, now, 40).unwrap();
        assert!(s.contains("%10")); // 2 günde 20 → günde 10
    }

    #[test]
    fn sarj_artisi_tahmin_vermez() {
        let now = 5_000_000_000_000u64;
        let lines = vec![
            format!("{{\"t\":{},\"v\":30}}", now - DAY),
            format!("{{\"t\":{},\"v\":90}}", now),
        ];
        let s = summary_from(&lines, now, 90).unwrap();
        assert!(s.contains("şarj"));
    }
}
