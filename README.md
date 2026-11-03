<div align="center">

# YouTube Desktop (`yt-desktop`)

**Ultra-lightweight, hardware-accelerated cross-platform YouTube client with embedded zero-config Anti-DPI bypass, AdBlock, SponsorBlock, Return YouTube Dislike, and Native Glass HUD.**

[![CI/CD Build & Release](https://github.com/lonestill/yt-desktop/actions/workflows/release.yml/badge.svg)](https://github.com/lonestill/yt-desktop/actions/workflows/release.yml)
[![Release](https://img.shields.io/github/v/release/lonestill/yt-desktop?color=emerald&label=Release)](https://github.com/lonestill/yt-desktop/releases/latest)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Linux%20%7C%20Windows-blueviolet.svg)]()

[🇷🇺 Документация на русском](#-русский) • [🇬🇧 English Documentation](#-english)

---

</div>

<a name="-русский"></a>
## 🇷🇺 Русский

### О проекте

`yt-desktop` — автономный десктопный клиент YouTube, написанный на **Rust**. В отличие от тяжеловесных решений на Electron (FreeTube и аналоги, сжирающие от 800 МБ до 1.5 ГБ RAM), приложение работает на нативных системных веб-движках (WebKit на macOS/Linux, WebView2 на Windows) и потребляет **~100–120 МБ оперативной памяти**.

Главная особенность — **полностью автономный обход блокировок ТСПУ (DPI)** без необходимости ставить VPN, сторонние службы или настраивать GoodbyeDPI/Zapret вручную.

---

### Архитектура и сравнение

| Характеристика | Electron (FreeTube / Chrome) | `yt-desktop` (Rust) |
|---|---|---|
| **Потребление RAM (RSS)** | 800 – 1400 MB | **~100 – 120 MB** |
| **Размер дистрибутива** | 150 – 250 MB | **~3 – 12 MB** |
| **Время холодного старта** | ~2.5 – 4.0 сек | **< 0.4 сек** |
| **Обход замедления YouTube (DPI)** | ❌ Требуется внешний VPN / zapret | ✅ **Встроенный SOCKS5 движок с авто-тестом** |
| **Блокировка рекламы** | Расширения / Инъекции | ✅ **Нативный AdBlock + Fast-Forward 16x** |
| **Спонсорские интеграции** | Сторонний плагин | ✅ **Встроенный SponsorBlock API** |
| **Счётчик дизлайков** | Сторонний плагин | ✅ **Встроенный Return YouTube Dislike API** |
| **Аппаратное декодирование** | Chromium sandbox | ✅ **Direct GPU / VideoToolbox / DXVA2** |

---

### Ключевые возможности

1. **Встроенный Anti-DPI движок (Zero-Config):**
   - Интегрированный `tpws` (macOS / Linux) и `ciadpi` (Windows), работающие в user-space на эфемерном порту без рут-прав и драйверов.
   - **Авто-тестирование при запуске:** параллельный пинг всех стратегий (`curl HEAD` с таймаутом 2с) и мгновенный выбор самого быстрого рабочего маршрута.
   - Готовые профили: `Ростелеком / Дом.ру`, `МТС / Мегафон / Yota`, `Билайн / МГТС / Теле2`, `TLS Record (SLD/SNI)`, `Прямое подключение`.
2. **Glass HUD Control Center:**
   - Компактный плавающий статус-бар (Pill) в шапке сайта с индикатором задержки в миллисекундах.
   - Вызов нативного меню настроек по `Cmd + ,` (macOS) или `Ctrl + ,` (Win/Linux).
   - Живое переключение стратегий без перезагрузки плеера.
3. **Полный AdBlock:**
   - Вырезание баннеров, промо-блоков и мерча.
   - Авто-скип видеорекламы: мгновенный mute звука + разгон ролика до 16x + нажатие кнопки пропуска за 50 мс.
   - Обход модальных окон «Блокировщики рекламы нарушают правила».
4. **SponsorBlock & Return YouTube Dislike:**
   - Нативная загрузка разметки сегментов спонсоров (Auto-skip + визуализация на таймлайне плеера).
   - Отображение честного количества дизлайков в кнопке под видео.
5. **OLED Pure Black & Max Quality:**
   - Режим глубокого черного `#000000` для OLED/Mini-LED дисплеев.
   - Принудительный выбор максимального качества потока (4K / 8K 60fps).
6. **Автообновления (GitHub Releases):**
   - Фоновая проверка новых версий через GitHub API.
   - Ненавязчивый индикатор и кнопка прямого скачивания обновлений.

---

### Установка

Скачайте релиз под вашу платформу из раздела **[Releases](https://github.com/lonestill/yt-desktop/releases/latest)**:

- **macOS (Apple Silicon & Intel):**
  Скачайте `YouTube-macOS.dmg`, откройте и перетащите `YouTube.app` в `Applications`.
- **Windows (x64):**
  Скачайте `YouTube-Windows-x64.zip`, распакуйте в любую папку и запустите `yt-desktop.exe`.
- **Linux (x86_64):**
  Скачайте `YouTube-Linux-x64.tar.gz`, распакуйте и запустите `./yt-desktop`.

---

### Горячие клавиши

| Сочетание клавиш | Действие |
|---|---|
| `Cmd + ,` / `Ctrl + ,` | Открыть / закрыть Glass HUD панель настроек |
| `Cmd + Shift + B` / `Ctrl + Shift + B` | Запустить тест всех маршрутов обхода (Benchmark) |
| `Cmd + Shift + P` / `Ctrl + Shift + P` | Режим Картинка-в-картинке (Picture-in-Picture) |
| `Cmd + R` / `Ctrl + R` | Перезагрузить страницу |
| `Cmd + [` / `Cmd + ]` | Навигация назад / вперед по истории |

---

### Флаги командной строки

```bash
# Принудительно открыть мастер первоначальной настройки сети:
yt-desktop --first
# или:
yt-desktop --welcome
```

---

<a name="-english"></a>
## 🇬🇧 English

### Overview

`yt-desktop` is a standalone, performance-focused YouTube desktop client built with **Rust**. Powered by native OS web rendering engines (WebKit on macOS/Linux and WebView2 on Windows), it delivers seamless 4K/8K 60fps playback with **~100 MB RAM** footprint.

Designed from the ground up for regions facing ISP-level YouTube throttling or DPI censorship, it embeds a local packet-tampering engine with parallel route benchmarking.

---

### Features

- **Embedded Anti-DPI Engine:** Transparent local SOCKS5 proxy running in user-space (`tpws` on Unix, `ciadpi` on Windows).
- **Startup Auto-Benchmark:** Parallel route probing on launch to automatically select the lowest-latency working route.
- **Glass HUD & Masthead Pill:** Ultra-clean Control Center accessible via `Cmd+,` / `Ctrl+,` or clicking the top header pill.
- **Full AdBlock:** Removes promo banners, overlays, and uses 16x fast-forward auto-skip for unskippable ads.
- **SponsorBlock & Return YouTube Dislike:** Direct native API integration without heavy browser extensions.
- **OLED Pure Black Mode:** Optimized for OLED and Mini-LED displays.
- **Automatic Update Checks:** Background queries to GitHub Releases API with seamless one-click updates.

---

### Building from Source

#### Prerequisites
- Rust 1.80+ (`cargo`, `rustc`)
- C compiler (`clang` or `gcc`) & `make`

#### macOS
```bash
git clone https://github.com/lonestill/yt-desktop.git
cd yt-desktop
./build_app.sh
open YouTube.app
```

#### Linux (Debian / Ubuntu)
```bash
sudo apt-get install -y libgtk-3-dev libwebkit2gtk-4.1-dev libappindicator3-dev gcc make
git clone https://github.com/lonestill/yt-desktop.git
cd yt-desktop
cd tpws && make && cd ..
cargo build --release
./target/release/yt-desktop
```

#### Windows
```powershell
git clone https://github.com/lonestill/yt-desktop.git
cd yt-desktop
cargo build --release
# Place ciadpi.exe next to target\release\yt-desktop.exe
```

---

### License

Distributed under the **MIT License**.
Embedded DPI engines:
- `tpws`: Bol-van (Zapret project)
- `ciadpi`: Hufrea (ByeDPI project)


### Platform validation

CI runs Rust tests and builds on macOS, Windows x64 and Linux x64.
The macOS bundle contains both Apple Silicon and Intel executables.
Windows requires the Microsoft Edge WebView2 Runtime. Linux requires GTK 3,
WebKitGTK 4.1 and curl at runtime; archives do not bundle these system libraries.
Native playback, login and DPI behavior still require testing on each OS.
For a local universal macOS build, install both targets first:

```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin
./build_app.sh
```
