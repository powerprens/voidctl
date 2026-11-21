# voidctl — Yol Haritası (PLAN.md)

> 2026-11-17'de, popüler ticari kulaklık yazılımları (iCUE, SteelSeries GG/Sonar, G Hub, Synapse)
> ve açık kaynak muadilleri ([Sapd/HeadsetControl](https://github.com/Sapd/HeadsetControl),
> [EarTrumpet](https://github.com/File-New-Project/EarTrumpet),
> [Equalizer APO + Peace](https://equalizerapo.com/peace-equalizer.html),
> [AutoEq](https://github.com/jaakkopasanen/AutoEq)) araştırılarak hazırlandı.

## Mevcut durum (v0.1)
- ✅ Pil (5 ölçüm medyanı), sidetone 0–128, LED on/off, beep, watch (düşük pil bip'i)
- ✅ Pil geçmişi: `%LOCALAPPDATA%\voidctl\history.jsonl`
- ✅ Uygulamaya göre otomatik sidetone: `profiles.ini`
- ✅ GUI: `voidgui.exe` (native Windows GUI, Rust)

## Rakiplerde ne var, bizde ne eksik?

| Özellik | iCUE | HeadsetControl | EarTrumpet | Peace/EQ APO | voidctl |
|---|---|---|---|---|---|
| Pil durumu | ✅ | ✅ | — | — | ✅ |
| Sidetone | ✅ | ✅ | — | — | ✅ (+ otomatik) |
| LED | ✅ | ✅ | — | — | ✅ |
| Bildirim sesi | ✅ | ✅ | — | — | ✅ |
| Otomatik kapanma (inactive) | ✅ | ✅ (Void V2) | — | — | ❌ |
| Sesli uyarılar on/off | ✅ | ✅ | — | — | ❌ |
| EQ (cihaz üstünde) | ✅ | ✅ (Corsair EQ + parametrik) | — | — | ❌ |
| Sistem geneli EQ + AutoEQ | ❌ | — | — | ✅ (1000+ preset) | ❌ |
| Uygulama bazlı ses | ✅ | — | ✅ | — | ❌ |
| Uygulama→cihaz yönlendirme | ✅ | — | ✅ | — | ❌ |
| ChatMix (oyun/sohbet dengesi) | ✅ | ✅ (bazı cihazlar) | — | — | ❌ |
| Donanım yazılımı sürümü | ✅ | ✅ (Linux çekirdeği okuyor) | — | — | ❌ |
| Mikrofon sesi/kapatma durumu okuma | ✅ | ✅ | — | — | ❌ |
| Tray ikonu + canlı pil | ✅ | GUI'ler var | ✅ | — | ❌ (sadece GUI penceresi) |
| Pil grafiği/geçmişi | ❌ | ❌ | — | — | 🔶 (veri var, görsel yok) |

**Boşluk fırsatı:** İkisinin de yapmadığı şeyler — pil geçmişi grafiği + süre tahmini,
uygulama bazlı sidetone (bizde zaten var, rakiplerde yok!), AutoEQ preset'ini tek tıkla
Void üstüne basmak.

## Fazlar

### Faz 1 — Tray + bildirim + UX (hızlı kazanımlar, ~1-2 gün)
- [ ] Tray ikonu: canlı pil yüzdesi (ikon rengi/tooltip), sağ menü: battery/sidetone/led/beep
- [ ] Düşük pil → Windows toast bildirimi (beep'in yerine/yanına)
- [ ] `WM_DEVICECHANGE`: kulaklık takıldı/çıktı algısı → otomatik bağlan + olay bildirimi
- [ ] history.jsonl → GUI'de pil grafiği (son 24 saat / 7 gün)
- [ ] Windows ile otomatik başlatma (Registry Run), tek örnek (single-instance)
- [ ] Global kısayol (örn. Ctrl+Alt+P → pil popup'ı)

### Faz 2 — HID protokol genişletme (orta, RE gerektirir)
Kaynak: HeadsetControl `device_registry.c` (Corsair Void paketleri hazır), [Ravenslofty/corsair-re](https://github.com/Ravenslofty/corsair-re), Linux `hid-corsair-void` sürücüsü.
- [ ] Otomatik kapanma süresi (inactive time)
- [ ] Sesli uyarılar on/off
- [ ] Mevcut sidetone/LED durumunu **okuma** (readback)
- [ ] Donanım yazılımı (fw) sürümü okuma
- [ ] Mikrofon kapatma durumu okuma (flip-to-mute algısı)
- [ ] Kulaklık/mikrofon ses seviyesi ayarı
- [ ] EQ preset'leri cihaza gönderme (iCUE'daki 7-band EQ)
- Not: Firmware güncellemesi bilinçli olarak kapsam dışı (riskli).

### Faz 3 — PC tarafı ses işleme (büyük fark yaratır)
- [ ] Uygulama bazlı ses kontrolü (EarTrumpet muadili, windows-rs WASAPI)
- [ ] ChatMix taklidi: Discord/oyun ayrımı tek kaydıraçla (HID yoksa per-app ses ile emülasyon)
- [ ] profiles.ini genişletme: uygulama bazlı EQ + LED + ses (sidetone kuralına ek)
- [ ] Equalizer APO entegrasyonu: varsa onun config'ini yazan hızlı EQ paneli
- [ ] AutoEq preset içe aktarma: JSON → EQ config (Void için hazır ayarlar)

### Faz 4 — Cila + dağıtım
- [ ] profiles.ini için GUI ayar editörü (elle INI düzenleme bitsin)
- [ ] Pil tüketim eğrisinden "kalan dakika" tahmini (history verisiyle)
- [ ] Dark mode + ayarlar sekmesi, TR/EN dil
- [ ] Kurulum paketi (Inno Setup), GitHub'da yayınlama + CI derleme
- [ ] Birim testler (profiles/history saf fonksiyonları zaten test edilebilir)

## Önerilen sıra
Faz 1 → (2'nin ilk 3 maddesi) → Faz 3'ün ChatMix + profil genişletmesi → kalanı.
Gerekçe: Faz 1 kullanıcıya anında görünür değer verir; Faz 2'nin readback'i EQ'dan önce
gelirse protokolü doğrulamak kolaylaşır.
