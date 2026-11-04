# Handy Voice for Windows

This personal fork adds offline English wake-phrase dictation and a movable,
always-on-top microphone control. Parakeet Unified remains the transcription
model. The Windows desktop speech recognizer detects the wake phrase; it is
separate from Handy's speech-to-text models and does not send audio to a server.

## Use

1. Launch Handy Voice and open the **Wake-Talk** tab. The floating control
   starts paused on a fresh install.
2. Enable Wake-Talk, choose a wake word or phrase (default: “hey handy”)
   and a silence timeout in seconds (0.1–10, at most one decimal place), then save. Changes require listening to be paused.
3. Press Play in Wake-Talk or the floating control. Hover over its play/pause
   icon to check the listening state.
4. Say “hey handy”, wait for the soft readiness bell and pink recording
   indicator, then dictate. The wake phrase is not included in the transcription.
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

A short, soft bell sounds only after a wake-triggered recording receives real
microphone audio, immediately before the indicator turns pink. It replaces the
usual start sound for wake dictation and works even with general audio feedback
disabled. It uses the selected output device and feedback volume (at reduced
loudness); setting that volume to zero silences it. Playback failure does not
prevent dictation.

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

## Codex-Talk

The separate **Codex-Talk** tab sends local transcriptions to Windows Codex
desktop, without pasting into the focused application. It defaults to thread
`01a1a777-2733-73c3-999d-c7d0e0f7cfaf`, wake phrase **hey codex**, and shortcut
**Ctrl+Alt+Shift+Space**. The feature starts disabled. Open your destination
thread in Windows Codex desktop, check its accessibility in Handy, then enable
Codex-Talk. Editing a thread ID automatically checks it; **Check again** refreshes
that result. The displayed model is the thread's current selection. Delivery
inherits its model, reasoning settings, permissions and workspace.

Play/Pause controls Codex wake listening. The dedicated shortcut works while
Codex-Talk is enabled, even with wake listening paused. Press it to begin and
press again or stop speaking to send. Pause cancels a Codex-owned take; disabling
Codex-Talk also unregisters its shortcut. Normal Wake-Talk and normal shortcuts
keep their existing paste behavior. Overlapping wake phrases and conflicting
Handy shortcuts are rejected. Both routes share one recorder; triggers received
while another recording or transcription is busy do not change its destination.

An active Codex turn receives the message immediately as steering input. If the
thread is idle, the desktop starts the next turn in that same thread. Codex-Talk
never selects a model. A failed delivery retains its text and original thread in
the tab until retry or discard (in memory until Handy exits). After a timeout,
check the thread before retrying: the desktop may already have accepted it.
The **Codex conversation** section loads the actual selected desktop thread,
including messages sent outside Handy, and refreshes every five seconds while
this tab is open. **Clear Codex conversation** uses the desktop's supported
edit-earlier-message operation to remove the old turns and start a fresh reset
turn in the same thread. A brief reset message and acknowledgement remain.
The thread ID, current model and workspace instructions are preserved; files
are not reverted. Clearing is blocked while Handy is recording/sending or Codex
is processing a turn. Threads whose first turn contains attachments are refused
to avoid carrying those attachments into a reset.

The separate **Handy delivery log** lists up to 100 deliveries for this Handy
session. **Clear delivery log** clears only that list, retained failed text and
delivery notices. It does not clear the Codex conversation or recordings in
Handy's History tab. The log also clears when Handy exits.

Stopping listening cannot recall a message already accepted by Codex. Normal
Handy audio history and retention still apply; the final text is sent to Codex
for model processing.

This Windows integration connects to the running desktop's local `codex-ipc`
named pipe and versioned thread-follower protocol. It uses the desktop's account,
not WSL Codex or a separately launched app-server. This is a private desktop
interface and may need updates when Codex changes it. No network listener,
credential copying, separate terminal, or replacement Codex process is needed.
Keep Codex desktop and the destination thread open. Connection and delivery
errors appear in Codex-Talk and Handy's normal notice UI.
