# Embedded by Rust. Offline Windows desktop recognition; no remote services.
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
try {
    Add-Type -AssemblyName System.Speech
    Add-Type -ReferencedAssemblies System.Speech -TypeDefinition @'
using System;
using System.Diagnostics;
using System.Globalization;
using System.IO;
using System.Runtime.InteropServices;
using System.Speech.Recognition;
using System.Speech.AudioFormat;
using System.Threading;

// A device-bound PCM stream for System.Speech. Its default-input API ignores
// Handy's microphone selector and can activate a Bluetooth headset instead.
public sealed class HandyMicrophoneStream : Stream {
    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct InputCaps {
        public ushort Manufacturer, Product;
        public uint Version;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)] public string Name;
        public uint Formats;
        public ushort Channels, Reserved;
    }
    [StructLayout(LayoutKind.Sequential, Pack = 2)]
    private struct WaveFormat {
        public ushort Tag, Channels;
        public uint SampleRate, BytesPerSecond;
        public ushort BlockAlign, Bits, Extra;
    }
    [StructLayout(LayoutKind.Sequential)]
    private struct WaveHeader {
        public IntPtr Data;
        public uint Length, Recorded;
        public UIntPtr User;
        public uint Flags, Loops;
        public IntPtr Next;
        public UIntPtr Reserved;
    }
    [DllImport("winmm.dll")] private static extern uint waveInGetNumDevs();
    [DllImport("winmm.dll", CharSet = CharSet.Unicode, EntryPoint = "waveInGetDevCapsW")]
    private static extern uint waveInGetDevCaps(UIntPtr id, out InputCaps caps, uint size);
    [DllImport("winmm.dll")] private static extern uint waveInOpen(out IntPtr handle, uint device, ref WaveFormat format, IntPtr callback, UIntPtr instance, uint flags);
    [DllImport("winmm.dll")] private static extern uint waveInPrepareHeader(IntPtr handle, IntPtr header, uint size);
    [DllImport("winmm.dll")] private static extern uint waveInUnprepareHeader(IntPtr handle, IntPtr header, uint size);
    [DllImport("winmm.dll")] private static extern uint waveInAddBuffer(IntPtr handle, IntPtr header, uint size);
    [DllImport("winmm.dll")] private static extern uint waveInStart(IntPtr handle);
    [DllImport("winmm.dll")] private static extern uint waveInReset(IntPtr handle);
    [DllImport("winmm.dll")] private static extern uint waveInClose(IntPtr handle);

    private const int BufferSize = 1600; // 50 ms of 16 kHz mono PCM16.
    private static readonly uint HeaderSize = (uint)Marshal.SizeOf(typeof(WaveHeader));
    private readonly object gate = new object();
    private readonly AutoResetEvent available = new AutoResetEvent(false);
    private readonly IntPtr[] headers = new IntPtr[4], buffers = new IntPtr[4];
    private readonly bool[] prepared = new bool[4];
    private IntPtr handle;
    private int closed, current, offset;
    private long position;

    public static string[] InputNames() {
        var names = new string[waveInGetNumDevs()];
        for (uint i = 0; i < names.Length; i++) {
            InputCaps caps;
            Check(waveInGetDevCaps(new UIntPtr(i), out caps, (uint)Marshal.SizeOf(typeof(InputCaps))), "enumerate input");
            names[i] = caps.Name;
        }
        return names;
    }
    private static string DeviceName(string name) {
        // Some Intel drivers expose "Intelr" in WinMM but "Intel registered"
        // in WASAPI. Normalize the registered/trademark symbols explicitly.
        return name.Replace("\u00ae", "r").Replace("\u2122", "tm");
    }
    public static int SelectDevice(string selected, string[] names) {
        selected = DeviceName(selected);
        // WinMM names have a 31-character limit. Prefer an exact match, then
        // allow only a UNIQUE truncated prefix of the selected full name.
        int match = -1;
        for (int i = 0; i < names.Length; i++) {
            if (!String.Equals(selected, DeviceName(names[i]), StringComparison.OrdinalIgnoreCase)) continue;
            if (match >= 0) throw new InvalidOperationException("The selected microphone has an ambiguous Windows device name. Rename it in Windows Sound settings.");
            match = i;
        }
        if (match >= 0) return match;
        for (int i = 0; i < names.Length; i++) {
            if (names[i].Length != 31 || !selected.StartsWith(DeviceName(names[i]), StringComparison.OrdinalIgnoreCase)) continue;
            if (match >= 0) throw new InvalidOperationException("The selected microphone has an ambiguous Windows device name. Rename it in Windows Sound settings.");
            match = i;
        }
        if (match < 0) throw new InvalidOperationException("The selected microphone is unavailable: " + selected + ". No other microphone was opened.");
        return match;
    }
    private static void Check(uint result, string operation) {
        if (result != 0) throw new IOException("Cannot " + operation + " on the selected microphone (Windows audio error " + result + ").");
    }
    public HandyMicrophoneStream(string selected) {
        uint device = (uint)SelectDevice(selected, InputNames());
        try {
            var format = new WaveFormat { Tag = 1, Channels = 1, SampleRate = 16000, BytesPerSecond = 32000, BlockAlign = 2, Bits = 16 };
            Check(waveInOpen(out handle, device, ref format, available.SafeWaitHandle.DangerousGetHandle(), UIntPtr.Zero, 0x00050000), "open input");
            for (int i = 0; i < headers.Length; i++) {
                buffers[i] = Marshal.AllocHGlobal(BufferSize);
                headers[i] = Marshal.AllocHGlobal((int)HeaderSize);
                var header = new WaveHeader { Data = buffers[i], Length = BufferSize };
                Marshal.StructureToPtr(header, headers[i], false);
                Check(waveInPrepareHeader(handle, headers[i], HeaderSize), "prepare input buffer");
                prepared[i] = true;
                Check(waveInAddBuffer(handle, headers[i], HeaderSize), "queue input buffer");
            }
            Check(waveInStart(handle), "start input");
        } catch { Dispose(); throw; }
    }
    public override int Read(byte[] target, int start, int count) {
        if (target == null) throw new ArgumentNullException("target");
        if (start < 0 || count < 0 || start > target.Length - count) throw new ArgumentOutOfRangeException();
        if (count == 0) return 0;
        int total = 0;
        // SAPI treats a short COM-stream read as EOF, so live reads must fill
        // the requested size unless capture is being disposed.
        while (total < count && Interlocked.CompareExchange(ref closed, 0, 0) == 0) {
            lock (gate) {
                if (closed != 0) return total;
                var header = (WaveHeader)Marshal.PtrToStructure(headers[current], typeof(WaveHeader));
                if ((header.Flags & 1) != 0) { // WHDR_DONE
                    int size = Math.Min(count - total, (int)header.Recorded - offset);
                    if (size > 0) {
                        Marshal.Copy(IntPtr.Add(header.Data, offset), target, start + total, size);
                        offset += size;
                        position += size;
                        total += size;
                    }
                    if (offset == header.Recorded) {
                        Check(waveInAddBuffer(handle, headers[current], HeaderSize), "requeue input buffer");
                        current = (current + 1) % headers.Length;
                        offset = 0;
                    }
                    if (size > 0) continue;
                }
            }
            // Polling also lets disposal end a pending read without requiring
            // a native callback to touch managed objects during teardown.
            try { available.WaitOne(50); } catch (ObjectDisposedException) { return total; }
        }
        return total;
    }
    protected override void Dispose(bool disposing) {
        if (Interlocked.Exchange(ref closed, 1) != 0) return;
        lock (gate) {
            if (handle != IntPtr.Zero) waveInReset(handle);
            for (int i = 0; i < headers.Length; i++) {
                if (prepared[i]) waveInUnprepareHeader(handle, headers[i], HeaderSize);
                if (headers[i] != IntPtr.Zero) Marshal.FreeHGlobal(headers[i]);
                if (buffers[i] != IntPtr.Zero) Marshal.FreeHGlobal(buffers[i]);
            }
            if (handle != IntPtr.Zero) waveInClose(handle);
            handle = IntPtr.Zero;
        }
        available.Dispose();
        base.Dispose(disposing);
    }
    public override bool CanRead { get { return closed == 0; } }
    public override bool CanSeek { get { return false; } }
    public override bool CanWrite { get { return false; } }
    // System.Speech's COM wrapper queries Length even for live streams.
    // An effectively unbounded length keeps it reading until disposal/EOF.
    public override long Length { get { return Int64.MaxValue; } }
    public override long Position { get { return position; } set { throw new NotSupportedException(); } }
    public override void Flush() { }
    public override long Seek(long value, SeekOrigin origin) {
        if ((origin == SeekOrigin.Current && value == 0) || (origin == SeekOrigin.Begin && value == position)) return position;
        throw new NotSupportedException("Live microphone input cannot be rewound.");
    }
    public override void SetLength(long value) { throw new NotSupportedException(); }
    public override void Write(byte[] data, int start, int count) { throw new NotSupportedException(); }
}

public static class HandyWakeDetector {
    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct SessionLevel1 {
        public uint SessionId;
        public int State;
        public int Flags;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 33)] public string Station;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 21)] public string User;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 18)] public string Domain;
        public long Logon, Connect, Disconnect, LastInput, Current;
        public uint IncomingBytes, OutgoingBytes, IncomingFrames, OutgoingFrames, IncomingCompressed, OutgoingCompressed;
    }
    [StructLayout(LayoutKind.Sequential)]
    private struct SessionInfo { public uint Level; public SessionLevel1 Data; }
    [DllImport("wtsapi32.dll", EntryPoint = "WTSQuerySessionInformationW", SetLastError = true)]
    private static extern bool QuerySession(IntPtr server, int sessionId, int infoClass, out IntPtr buffer, out uint bytes);
    [DllImport("wtsapi32.dll")]
    private static extern void WTSFreeMemory(IntPtr buffer);

    public static bool SessionAllowsAudio(int state, int flags) {
        // WTSActive and WTS_SESSIONSTATE_UNLOCK (Windows 10 and later).
        return state == 0 && flags == 1;
    }
    public static bool ConsoleAvailable(int sessionId) {
        IntPtr buffer;
        uint bytes;
        if (!QuerySession(IntPtr.Zero, sessionId, 25, out buffer, out bytes))
            throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error(), "Cannot read Windows session state");
        try {
            if (bytes < Marshal.SizeOf(typeof(SessionInfo))) throw new InvalidOperationException("Incomplete Windows session information");
            var info = (SessionInfo)Marshal.PtrToStructure(buffer, typeof(SessionInfo));
            return info.Level == 1 && SessionAllowsAudio(info.Data.State, info.Data.Flags);
        } finally { WTSFreeMemory(buffer); }
    }
    public static bool InactiveSessionError(Exception error) {
        for (var current = error; current != null; current = current.InnerException) {
            if (current.HResult == unchecked((int)0x80045063) ||
                current.Message.IndexOf("non-active console session", StringComparison.OrdinalIgnoreCase) >= 0) return true;
        }
        return false;
    }
    private static void State(TextWriter output, ref string previous, string next) {
        if (previous == next) return;
        previous = next;
        output.WriteLine(next); output.Flush();
    }
    public static void Run(string phrase, int parentId, string waveFile) {
        RecognizerInfo english = null;
        foreach (var info in SpeechRecognitionEngine.InstalledRecognizers()) {
            if (info.Culture.Name == "en-US") { english = info; break; }
            if (english == null && info.Culture.TwoLetterISOLanguageName == "en") english = info;
        }
        if (english == null) throw new InvalidOperationException("Install an English desktop speech recognizer in Windows Language settings.");
        using (var parent = Process.GetProcessById(parentId)) {
            var output = TextWriter.Synchronized(Console.Out);
            string previous = null;
            bool offline = !String.IsNullOrEmpty(waveFile);
            // Deterministic lifecycle testing is allowed only with WAV input.
            string steps = offline ? Environment.GetEnvironmentVariable("HANDY_WAKE_TEST_SESSION_STEPS") : null;
            string[] sequence = String.IsNullOrEmpty(steps) ? null : steps.Split(',');
            int sequenceIndex = 0;
            Func<bool> available = () => sequence != null
                ? sequence[Math.Min(sequenceIndex++, sequence.Length - 1)] == "unlocked"
                : offline || ConsoleAvailable(parent.SessionId);
            while (!parent.HasExited) {
                if (!available()) {
                    State(output, ref previous, "suspended");
                    Thread.Sleep(250);
                    continue;
                }
                try {
                    State(output, ref previous, "starting");
                    bool suspended = false;
                    Exception recognitionError = null;
                    using (var finished = new ManualResetEvent(false))
                    using (var recognizer = new SpeechRecognitionEngine(english))
                    using (var input = offline || String.IsNullOrEmpty(Environment.GetEnvironmentVariable("HANDY_WAKE_MICROPHONE"))
                        ? null : new HandyMicrophoneStream(Environment.GetEnvironmentVariable("HANDY_WAKE_MICROPHONE"))) {
                        var builder = new GrammarBuilder { Culture = english.Culture };
                        builder.Append(phrase);
                        recognizer.LoadGrammar(new Grammar(builder));
                        recognizer.SpeechRecognized += (sender, args) => {
                            if (args.Result.Confidence >= 0.85f && String.Equals(args.Result.Text, phrase, StringComparison.OrdinalIgnoreCase)) {
                                output.WriteLine("wake"); output.Flush();
                            }
                        };
                        recognizer.RecognizeCompleted += (sender, args) => {
                            recognitionError = args.Error;
                            try { finished.Set(); } catch (ObjectDisposedException) { }
                        };
                        if (offline) recognizer.SetInputToWaveFile(waveFile);
                        else if (input != null) {
                            recognizer.SetInputToAudioStream(input, new SpeechAudioFormatInfo(16000, AudioBitsPerSample.Sixteen, AudioChannel.Mono));
                            output.WriteLine("input:" + Environment.GetEnvironmentVariable("HANDY_WAKE_MICROPHONE")); output.Flush();
                        } else recognizer.SetInputToDefaultAudioDevice();
                        recognizer.RecognizeAsync(RecognizeMode.Multiple);
                        State(output, ref previous, "ready");
                        while (!parent.HasExited) {
                            if (!available()) { suspended = true; break; }
                            if (finished.WaitOne(250)) break;
                        }
                        // Disposal releases the microphone before waiting for unlock.
                        if (suspended) State(output, ref previous, "suspended");
                    }
                    if (parent.HasExited) return;
                    if (suspended) continue;
                    if (recognitionError != null) throw recognitionError;
                    // WAV completion ends a smoke test; live input stopping is an error.
                    if (offline) return;
                    throw new InvalidOperationException("The Windows wake recognizer stopped unexpectedly.");
                } catch (Exception error) {
                    // The session may change between the state query and SAPI startup.
                    if (InactiveSessionError(error) || !available()) {
                        State(output, ref previous, "suspended");
                        Thread.Sleep(1000);
                        continue;
                    }
                    throw;
                }
            }
        }
    }
}
'@
    if ($env:HANDY_WAKE_TEST_PROBE -eq '1') {
        [HandyWakeDetector]::ConsoleAvailable((Get-Process -Id $PID).SessionId)
    } else {
        [HandyWakeDetector]::Run($env:HANDY_WAKE_PHRASE, [int]$env:HANDY_WAKE_PARENT, $env:HANDY_WAKE_TEST_WAV)
    }
} catch {
    [Console]::Error.WriteLine($_.Exception.Message)
    exit 1
}
