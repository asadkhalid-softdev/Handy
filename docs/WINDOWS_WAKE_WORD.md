# Handy Voice for Windows

This personal fork adds offline English wake-phrase dictation and a movable,
always-on-top microphone control. Parakeet Unified remains the transcription
model. The Windows desktop speech recognizer detects the wake phrase; it is
separate from Handy's speech-to-text models and does not send audio to a server.

## Use

1. Launch Handy Voice and open the **Wake-Talk** tab. The floating control
   starts paused on a fresh install.
2. Enable Wake-Talk, choose a wake word or phrase (default: “hey handy”)
   and a silence timeout, then save. Changes require listening to be paused.
3. Press Play in Wake-Talk or the floating control. Hover over its play/pause
   icon to check the listening state.
4. Say “hey handy”, wait for the existing recording indicator to turn pink,
   then dictate. The wake phrase is not included in the transcription.
5. After silence, Handy finishes transcription and uses the configured paste
   method. For Tabby, use Shift+Insert. Automatic Enter remains off by default.

The floating control contains only drag, play/pause and settings icons. Its
settings icon opens Wake-Talk directly. Drag the grip to move the control;
its screen position is saved. A saved
position outside the available monitor work areas is reset at the next launch.
Disabling **Enable Wake-Talk** stops listening and hides the floating control.
Re-enable it from Wake-Talk to show the control again, with listening paused.

Pause leaves the feature and floating control available, but kills the wake
detector and cancels any voice-owned recording or pending
transcription. A recording already pasted cannot be recalled. Pause is saved
across restarts and login. Keyboard dictation still works on explicit shortcut
presses while background listening is paused.

## Limits and microphone behavior

- Windows only, with an installed English **desktop speech recognition** engine.
  The startup error explains when the engine or default mic is unavailable.
- Wake detection uses the default Windows microphone. Choose the same microphone
  in Handy, or leave Handy on its system default. The detector and recorder use
  separate shared Windows audio streams. There is no raw audio pre-roll: wait
  for the pink indicator before speaking the dictation.
- Use Wake-phrase dictation instead of Handy's independent Always-on Microphone
  option. Their simultaneous use is rejected so Pause can release background
  listening predictably.
- Idle audio is processed locally by Windows and is not saved by this feature.
  Dictations still follow Handy's normal history and retention settings.
- Speech detection uses Handy's recording VAD. Its hangover adds to the configured
  silence timeout. With VAD disabled, a fixed audio-level threshold is used.
- A session with no speech is cancelled after 10 seconds. Continuous recording
  stops after two minutes. Wake events received while another take or file is
  processing never queue another recording.
- This is a constrained Windows speech grammar, not a speaker-verification
  system. Other people or speakers saying the phrase may activate it. The first
  version uses a conservative recognition-confidence threshold of 0.85.

## Build and verify

Follow BUILD.md for Rust, Bun, MSVC, CMake and Vulkan SDK prerequisites. Then:

```powershell
bun install --frozen-lockfile
bun run build
bun run lint
bun run check:translations
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --locked
cargo test --manifest-path src-tauri/Cargo.toml --locked --lib
powershell -NoProfile -File scripts/test-wake-detector.ps1
bun run tauri build --no-bundle
```

The detector smoke test synthesizes a wake phrase and unrelated speech to WAV
files and verifies recognition without opening a microphone. Rust tests cover
silence and maximum-duration deadlines, start failures, and stale session tokens.
Actual headset latency, false activations in room noise, dragging across displays,
and paste into the desired application also require a Windows desktop test.

For a standalone unsigned development preview without an installer:

```powershell
powershell -NoProfile -File scripts/build-voice-preview.ps1
```

It produces `src-tauri/target/voice-preview/Handy Voice.exe` and the required
runtime files. Keep that folder intact. This preview embeds the frontend and
does not need a development web server. It does not replace the installed app.

On an interactive Windows desktop with the preview paused and an English UI:

```powershell
powershell -NoProfile -File scripts/test-voice-control.ps1
# Also open the default microphone briefly and verify native resume/pause:
powershell -NoProfile -File scripts/test-voice-control.ps1 -ExerciseMicrophone
```

The app identity is `dev.asadkhalid.handyvoice`, with product name **Handy Voice**.
It has separate settings, logs, login registration and WebView data from official
Handy (`com.pais.handy`). Do not run both with the same recording shortcut.
Official automatic updates are disabled in the fork to preserve these changes.
Models in the shared Hugging Face cache can be reused.

## Publish the personal fork

The intended GitHub owner is **asadkhalid-softdev**. No credentials are included.
After signing in with that account, run this from the repository root:

```bash
gh auth login --hostname github.com --git-protocol https --web
bash scripts/publish-voice-fork.sh
```

Development now uses `main` on the personal fork. `origin` points to
`asadkhalid-softdev/Handy`; `upstream` points to `cjpais/Handy`. The initial
`feat/windows-wake-word-control` branch remains available as a checkpoint.

The script verifies the authenticated account and fork parent before changing
remotes or pushing the current main or feature branch. It never pushes to cjpais/Handy
and does not open a pull request. Upstream currently has a feature freeze; a
future upstream proposal must follow its contribution and discussion requirements.
