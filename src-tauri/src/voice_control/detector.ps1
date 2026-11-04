# Embedded by the Rust executable. Uses the offline Windows desktop speech
# recognizer; no downloaded code, remote speech services, or transcript logging.
$ErrorActionPreference = 'Stop'
try {
    Add-Type -AssemblyName System.Speech
    Add-Type -ReferencedAssemblies System.Speech -TypeDefinition @'
using System;
using System.Diagnostics;
using System.Globalization;
using System.IO;
using System.Speech.Recognition;
using System.Threading;

public static class HandyWakeDetector {
    public static void Run(string phrase, int parentId, string waveFile) {
        RecognizerInfo english = null;
        foreach (var info in SpeechRecognitionEngine.InstalledRecognizers()) {
            if (info.Culture.Name == "en-US") { english = info; break; }
            if (english == null && info.Culture.TwoLetterISOLanguageName == "en") english = info;
        }
        if (english == null) throw new InvalidOperationException("Install an English desktop speech recognizer in Windows Language settings.");
        using (var parent = Process.GetProcessById(parentId))
        using (var recognizer = new SpeechRecognitionEngine(english))
        using (var finished = new ManualResetEvent(false)) {
            var output = TextWriter.Synchronized(Console.Out);
            var builder = new GrammarBuilder { Culture = english.Culture };
            builder.Append(phrase);
            recognizer.LoadGrammar(new Grammar(builder));
            recognizer.SpeechRecognized += (sender, args) => {
                if (args.Result.Confidence >= 0.85f && String.Equals(args.Result.Text, phrase, StringComparison.OrdinalIgnoreCase)) {
                    output.WriteLine("wake"); output.Flush();
                }
            };
            recognizer.RecognizeCompleted += (sender, args) => {
                if (args.Error != null) Console.Error.WriteLine(args.Error.Message);
                finished.Set();
            };
            // Wave input exists only for the deterministic development smoke test.
            if (String.IsNullOrEmpty(waveFile)) recognizer.SetInputToDefaultAudioDevice();
            else recognizer.SetInputToWaveFile(waveFile);
            recognizer.RecognizeAsync(RecognizeMode.Multiple);
            output.WriteLine("ready"); output.Flush();
            while (!finished.WaitOne(250)) {
                if (parent.HasExited) break;
            }
            recognizer.RecognizeAsyncCancel();
        }
    }
}
'@
    [HandyWakeDetector]::Run($env:HANDY_WAKE_PHRASE, [int]$env:HANDY_WAKE_PARENT, $env:HANDY_WAKE_TEST_WAV)
} catch {
    [Console]::Error.WriteLine($_.Exception.Message)
    exit 1
}
