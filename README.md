# Notely ✦

<p align="center">
  <strong>The invisible, ultra-low-latency AI copilot & desktop overlay for macOS.</strong><br>
  Built for high-stakes interviews, executive calls, sales pitches, and live technical discussions.
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Platform-macOS%20(Apple%20Silicon%20%2F%20Intel)-black?style=flat-square&logo=apple" alt="Platform: macOS" />
  <img src="https://img.shields.io/badge/Framework-Tauri%20v2-24C8D5?style=flat-square&logo=tauri" alt="Tauri v2" />
  <img src="https://img.shields.io/badge/Backend-Rust-DEA584?style=flat-square&logo=rust" alt="Rust" />
  <img src="https://img.shields.io/badge/Frontend-React%2018%20%2B%20Tailwind-38BDF8?style=flat-square&logo=react" alt="React" />
  <img src="https://img.shields.io/badge/AI-Google%20Gemini%20API-4285F4?style=flat-square&logo=google" alt="Gemini" />
  <img src="https://img.shields.io/badge/License-MIT-green?style=flat-square" alt="License" />
</p>

---

## Overview

**Notely** is a lightweight, stealth AI desktop assistant designed to sit discretely above all your open applications, presentations, and screen shares without ever being captured on video or noticed by other participants.

By combining native **macOS window privacy APIs** (`NSWindowSharingNone`), **Tauri 2**, and **Google Gemini's streaming models**, Notely delivers intelligent conversational guidance, real-time code solutions, screen analysis, and meeting transcription in **under 1 second**.

---

## Key Features

### 🥷 1. Complete Stealth & Screen-Share Immunity
- **Invisible to Screen Shares**: Built with native macOS `contentProtection` (`NSWindowSharingNone`). The overlay card is completely hidden from Zoom, Google Meet, Microsoft Teams, Discord, OBS, and QuickTime screen recordings.
- **No Dock or ⌘-Tab Presence**: Configured as an accessory (`LSUIElement`). It does not appear in the macOS Dock or the ⌘+Tab application switcher.
- **Universal Multi-Workspace Floating**: Floats (`alwaysOnTop`) across all macOS Spaces, mission control, and over fullscreen apps (Xcode, VS Code, Keynote, browsers).
- **Click-Through Mode (`⌘+Shift+T`)**: Ignores mouse events on command, allowing you to click directly into the IDE or presentation running behind the overlay without moving or minimizing Notely.
- **Panic Hide & Tray Recovery (`⌘+Shift+H`)**: Instantly conceals the overlay. Restore it anytime using the global hotkey or the persistent macOS menu bar icon.

### ⚡ 2. Sub-Second AI Assist (< 1s Latency)
- **Fast Mode (`gemini-2.5-flash`)**: Configured with zero thinking overhead for sub-second, punchy bullet points and natural answers you can speak aloud mid-sentence.
- **Smart Mode (`gemini-3.5-flash`)**: Engages deeper reasoning tokens for complex algorithmic challenges, system design architecture, and tough objections.
- **Live Elapsed Reasoning Timer**: Real-time ticker (`Thinking… (1.2s)`) with instant cancellation (`Cancel` button or hotkey).
- **Zero-Latency Text Queries**: Automatically bypasses screenshot capture when answering purely textual questions to ensure instant response streaming.

### 🎙️ 3. Live Meeting Transcription & Diarization
- **Dual-Stream Audio Mixer**: Captures both your microphone input and system sound (other callers via virtual loopback like BlackHole).
- **Real-Time Speaker Diarization**: Clean visual separation between **Me** and **Others** in a live rolling transcript.
- **Gemini Live Integration**: Streams audio through Google's Gemini Live bidirectional WebSocket session.

### 📸 4. Region & Fullscreen Screen Intelligence
- **Retina-Accurate Region Snipping (`⌘+Shift+A`)**: Drag to select any rectangle on any monitor. Window coordinates are automatically normalized from Apple Retina physical pixels to point bounds.
- **Multimodal Visual Reasoning**: Ask questions directly about diagrams, compiler error traces, slide decks, or live interview coding prompts.

### 📝 5. Automated Post-Meeting Synthesis
- **Structured JSON Summaries**: Generates high-signal meeting notes, key decisions, owner-assigned action items, and a ready-to-send follow-up email.
- **Searchable Local SQLite Archive**: Stores past meetings, transcripts, and notes locally with instant search.

### 🔒 6. Zero Third-Party Servers (Privacy First)
- **Local-Only Architecture**: Notely does not run any intermediary backend servers or analytics.
- **Native OS Keychain Security**: Your Google Gemini API key is stored strictly inside macOS Keychain (`login.keychain-db`) with local SQLite fallback. Keys never touch JavaScript `localStorage` or the webview console.

---

## Architecture

```
┌────────────────────────────────────────────────────────┐
│                      Notely UI                         │
│       React 18 · Vite · Tailwind · Framer-feel Glass   │
└───────────────────────────┬────────────────────────────┘
                            │ Tauri IPC (Commands & Events)
┌───────────────────────────▼────────────────────────────┐
│                  Rust Backend (Tauri 2)                 │
│  ├─ Windowing   : macOS NSWindowSharingNone / Spaces   │
│  ├─ Audio Engine: cpal + rubato (Mic + System Mixer)   │
│  ├─ Vision      : xcap + imageops thumbnail (HiDPI)    │
│  ├─ Security    : Apple Keychain Native (`keyring`)    │
│  └─ Storage     : SQLite (`rusqlite`)                  │
└─────────────┬───────────────────────────┬──────────────┘
              │ SSE Streaming             │ WebSocket
┌─────────────▼───────────────┐ ┌─────────▼──────────────┐
│   Google Gemini REST API    │ │   Gemini Live API      │
│ (2.5 Flash / 3.5 Flash)     │ │ (2.0 Flash Live)       │
└─────────────────────────────┘ └────────────────────────┘
```

---

## Global Hotkeys Reference

| Shortcut | Action | Description |
|---|---|---|
| <kbd>⌘</kbd> + <kbd>↵</kbd> | **Run Assist** | Ask Gemini about current context or custom question |
| <kbd>⌘</kbd> + <kbd>Shift</kbd> + <kbd>X</kbd> | **Toggle Stealth Mode** | Toggle screen share protection & Dock presence (🥷 / 👀) |
| <kbd>⌘</kbd> + <kbd>Shift</kbd> + <kbd>H</kbd> | **Panic Hide / Show** | Instantly hide overlay or restore it |
| <kbd>⌘</kbd> + <kbd>Shift</kbd> + <kbd>T</kbd> | **Toggle Click-Through** | Pass mouse clicks to background windows |
| <kbd>⌘</kbd> + <kbd>Shift</kbd> + <kbd>A</kbd> | **Region Screenshot** | Interactive visual region selector |
| <kbd>⌘</kbd> + <kbd>Shift</kbd> + <kbd>S</kbd> | **Full Screenshot** | Capture active monitor into context |
| <kbd>⌘</kbd> + <kbd>Shift</kbd> + <kbd>1</kbd> | **"What to Say" Preset** | Suggests 2–3 immediate conversational options |
| <kbd>⌘</kbd> + <kbd>Shift</kbd> + <kbd>2</kbd> | **Follow-up Preset** | Generates 4–6 sharp follow-up questions |
| <kbd>⌘</kbd> + <kbd>Shift</kbd> + <kbd>3</kbd> | **Recap Preset** | Summarizes meeting context so far under 150 words |
| <kbd>⌘</kbd> + <kbd>Shift</kbd> + <kbd>4</kbd> | **Analyze Screen Preset** | Full multimodal inspection of visible screen |

---

## Getting Started

### Prerequisites

- **macOS**: 12.0 (Monterey) or later (Apple Silicon M1/M2/M3/M4 or Intel).
- **Node.js**: v18 or later (`node -v`).
- **Rust**: 1.77 or later (`rustc --version`).
- **Google Gemini API Key**: Free tier or paid key from [Google AI Studio](https://aistudio.google.com/).

### Installation & Development

1. **Clone the repository:**
   ```bash
   git clone https://github.com/your-username/notely.git
   cd notely
   ```

2. **Install frontend dependencies:**
   ```bash
   npm install
   ```

3. **Run in development mode:**
   ```bash
   npm run tauri dev
   ```

4. **Add your Gemini API Key:**
   - Open Notely, click `···` in the input bar and select **Settings** (or press the settings icon).
   - Paste your Gemini API key (`AIzaSy...`) and click **Save** (or press <kbd>Enter</kbd>).
   - The key is immediately secured inside your macOS Keychain.

---

## Building the Production App / DMG

To generate the optimized release `.app` bundle:

```bash
# Build frontend assets and native macOS bundle
npm run build
npx tauri build --bundles app
```

The output application will be located at:
```
src-tauri/target/release/bundle/macos/Notely.app
```

To install it to your Applications folder:
```bash
cp -R src-tauri/target/release/bundle/macos/Notely.app /Applications/
xattr -cr /Applications/Notely.app
open /Applications/Notely.app
```

---

## Setting Up System Audio Capture (Optional)

By default, Notely records microphone audio seamlessly. To also transcribe incoming sound from other meeting participants on macOS:

1. Install [BlackHole 2ch](https://github.com/ExistentialAudio/BlackHole) via Homebrew:
   ```bash
   brew install blackhole-2ch
   ```
2. Open **Audio MIDI Setup** on your Mac (`/Applications/Utilities/Audio MIDI Setup.app`).
3. Click the `+` button in the lower-left corner and select **Create Multi-Output Device**.
4. Check both your standard audio output (e.g. **MacBook Speakers** or **Headphones**) and **BlackHole 2ch**.
5. Set this Multi-Output Device as your macOS System Output. Notely will now automatically mix incoming call audio alongside your microphone.

---

## Recommended Model Settings

| Task | Recommended Model | Latency | Why |
|---|---|---|---|
| **Fast Assist** | `gemini-2.5-flash` | ~0.8s | Lowest time-to-first-token; concise bullet points |
| **Smart Assist & Notes** | `gemini-3.5-flash` | ~2.5s | Deep reasoning, high context window, full free-tier quota |
| **Live Audio Transcription** | `gemini-2.0-flash-live-001` | Real-time | Bidirectional low-latency audio stream |

---

## Privacy & Security Policy

- **No Remote Telemetry**: Notely contains zero telemetry, trackers, or proxy servers.
- **Direct API Communication**: Every API request is transmitted directly via TLS from your device to `generativelanguage.googleapis.com`.
- **Keychain Guarded**: API keys are saved using the macOS Security Framework (`SecItemAdd` / `SecItemCopyMatching`) under the service `com.notely.app`.
- **Ephemeral Screen Cache**: Screen captures are held temporarily in memory and deleted after generation unless explicitly configured otherwise.

---

### Windows 11 and meeting acceptance checks

- On Windows, use `powershell -File scripts/check-windows.ps1` after installing Node.js, Rust (MSVC), and the C++ Build Tools. The Windows CI workflow also builds an unsigned NSIS installer; its build has not been verified until that workflow or the Windows script passes.
- In a short test call, state a project name at the beginning, a decision halfway through, and an owner/deadline at the end. Confirm both microphone and other-participant speech appear. On Windows, the call must play through the default output device; on macOS, other participants need a routed loopback input.
- End the meeting, reopen it in History, generate notes, then restart Notely. Verify the beginning/middle/end transcript and the saved decision, owner, and deadline. Notes are explicitly generated, not automatic.
- Recognized transcript is checkpointed during recording. The UI shows at most 500 final lines; storage and notes retain the full recognized transcript. “Me”/“Others” label audio sources, not individual speakers. Network/audio failures can still leave gaps; never assume every spoken word was captured.

Suggestions follow the workspace context: profile-specific starters when idle, meeting help after recording starts, review actions after a meeting, and follow-ups for an existing chat. Meeting actions wait for captured speech; Revisit a topic asks for the topic before sending.

Use the header’s **Help (?)** button for permission checks, meeting audio setup, shortcuts, and troubleshooting. **Hide** uses an eye-off icon; restore Notely from the ghost in the menu bar/system tray or your configured Hide / show shortcut. Regenerate the shared ghost app/tray assets with `npm run icons:generate`.

The ghost menu also opens Settings, Help, Diagnostics, and saved work, with quick chat/capture actions and meeting controls. Protection and click-through use live checkmarks; quitting during a meeting saves it before closing.

Meeting transcription now has its own provider in Settings: **OpenAI** (default, `gpt-live-transcribe`) or **Gemini**. Only the selected audio provider’s key is needed for a meeting; Claude, OpenAI, and Gemini remain independent choices for chat and notes. The transcription section’s Test connection validates the live audio setup without sending microphone data. Save provider changes before the next meeting. OpenAI uses client-side speech detection and separate mic/system audio streams; verify quiet speech and both audio sources in a real test call.

---

## License

This project is licensed under the [MIT License](LICENSE).