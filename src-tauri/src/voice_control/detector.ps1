# Embedded by Rust. Offline Windows desktop recognition; no remote services.
$ErrorActionPreference = 'Stop'
try {
    Add-Type -AssemblyName System.Speech
    Add-Type -ReferencedAssemblies System.Speech -TypeDefinition @'
using System;
using System.Diagnostics;
using System.Globalization;
using System.IO;
using System.Runtime.InteropServices;
using System.Speech.Recognition;
using System.Threading;

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
                    using (var recognizer = new SpeechRecognitionEngine(english)) {
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
                        else recognizer.SetInputToDefaultAudioDevice();
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
