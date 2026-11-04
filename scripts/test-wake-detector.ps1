# Offline smoke test: synthesizes WAVs and never opens a microphone.
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Speech
$detector = Join-Path $PSScriptRoot '..\src-tauri\src\voice_control\detector.ps1'
$testDir = Join-Path ([IO.Path]::GetTempPath()) ('handy-wake-' + [Guid]::NewGuid())
New-Item -ItemType Directory $testDir | Out-Null
try {
    foreach ($case in @(
        @{ Name = 'wake'; Text = 'hey handy'; Expected = $true },
        @{ Name = 'unrelated'; Text = 'the weather is sunny this afternoon'; Expected = $false }
    )) {
        $wave = Join-Path $testDir ($case.Name + '.wav')
        $speech = New-Object System.Speech.Synthesis.SpeechSynthesizer
        try {
            $speech.SelectVoiceByHints([System.Speech.Synthesis.VoiceGender]::NotSet,
                [System.Speech.Synthesis.VoiceAge]::NotSet, 0, [Globalization.CultureInfo]::GetCultureInfo('en-US'))
            $speech.SetOutputToWaveFile($wave)
            $speech.Speak($case.Text)
        } finally { $speech.Dispose() }
        $env:HANDY_WAKE_PHRASE = 'hey handy'
        $env:HANDY_WAKE_PARENT = $PID.ToString()
        $env:HANDY_WAKE_TEST_WAV = $wave
        $output = Join-Path $testDir ($case.Name + '.txt')
        $errors = Join-Path $testDir ($case.Name + '.errors.txt')
        $process = Start-Process -FilePath "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe" `
            -ArgumentList @('-NoProfile', '-NonInteractive', '-File', ('"' + $detector + '"')) `
            -RedirectStandardOutput $output -RedirectStandardError $errors -Wait -PassThru
        if ($process.ExitCode -ne 0) { throw (Get-Content $errors -Raw) }
        $lines = @(Get-Content $output)
        if ($lines -notcontains 'ready') { throw "Detector did not become ready" }
        if (($lines -contains 'wake') -ne $case.Expected) { throw "Unexpected result for $($case.Name)" }
        Write-Output "PASS: $($case.Name)"
    }
} finally {
    Remove-Item Env:HANDY_WAKE_PHRASE,Env:HANDY_WAKE_PARENT,Env:HANDY_WAKE_TEST_WAV -ErrorAction SilentlyContinue
    Remove-Item $testDir -Recurse -Force
}
