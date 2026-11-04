# Deterministic lock/unlock lifecycle test. Uses WAV input, never a live mic.
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Speech
$detector = Join-Path $PSScriptRoot '..\src-tauri\src\voice_control\detector.ps1'
$testDir = Join-Path ([IO.Path]::GetTempPath()) ('handy-session-' + [Guid]::NewGuid())
New-Item -ItemType Directory $testDir | Out-Null
$child = $null
try {
    $env:HANDY_WAKE_TEST_PROBE = '1'
    $null = & $detector
    if (![HandyWakeDetector]::SessionAllowsAudio(0, 1)) { throw 'Unlocked session should allow audio' }
    foreach ($state in @(@(0,0), @(4,1), @(0,-1))) {
        if ([HandyWakeDetector]::SessionAllowsAudio($state[0], $state[1])) { throw 'Locked/disconnected/unknown session allowed audio' }
    }
    $inactive = New-Object Runtime.InteropServices.COMException('Session inactive', -2147200925)
    if (![HandyWakeDetector]::InactiveSessionError($inactive)) { throw 'Inactive SAPI HRESULT was not recognized' }
    if ([HandyWakeDetector]::InactiveSessionError([InvalidOperationException]::new('Other audio failure'))) { throw 'A real audio error was hidden' }
    Remove-Item Env:HANDY_WAKE_TEST_PROBE
    Write-Output 'PASS: Windows session states and transient SAPI error classification'

    $wave = Join-Path $testDir 'wake.wav'
    $speech = New-Object System.Speech.Synthesis.SpeechSynthesizer
    try {
        $speech.SelectVoiceByHints([System.Speech.Synthesis.VoiceGender]::NotSet,
            [System.Speech.Synthesis.VoiceAge]::NotSet, 0, [Globalization.CultureInfo]::GetCultureInfo('en-US'))
        $speech.SetOutputToWaveFile($wave)
        $speech.Speak('hey handy')
    } finally { $speech.Dispose() }
    $env:HANDY_WAKE_PHRASE = 'hey handy'
    $env:HANDY_WAKE_PARENT = $PID.ToString()
    $env:HANDY_WAKE_TEST_WAV = $wave
    $env:HANDY_WAKE_TEST_SESSION_STEPS = 'locked,unlocked,locked,unlocked'
    $output = Join-Path $testDir 'lifecycle.txt'
    $errors = Join-Path $testDir 'errors.txt'
    $child = Start-Process "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe" `
        -ArgumentList @('-NoProfile','-NonInteractive','-File', ('"' + $detector + '"')) `
        -WindowStyle Hidden -RedirectStandardOutput $output -RedirectStandardError $errors -PassThru
    $null = $child.Handle
    if (!$child.WaitForExit(20000)) { throw 'Lock/unlock test timed out' }
    $child.WaitForExit()
    $child.Refresh()
    if ($child.ExitCode -ne 0) { throw (Get-Content $errors -Raw) }
    $states = @((Get-Content $output) | Where-Object { $_ -in @('ready','suspended') })
    if (($states -join ',') -ne 'suspended,ready,suspended,ready') { throw "Unexpected lifecycle: $states" }
    if ((Get-Item $errors).Length -ne 0) { throw (Get-Content $errors -Raw) }
    Write-Output 'PASS: locks suspend and unlocks restart recognition without a fatal error'

    $env:HANDY_WAKE_TEST_SESSION_STEPS = 'locked'
    $output = Join-Path $testDir 'paused.txt'
    $child = Start-Process "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe" `
        -ArgumentList @('-NoProfile','-NonInteractive','-File', ('"' + $detector + '"')) `
        -WindowStyle Hidden -RedirectStandardOutput $output -RedirectStandardError $errors -PassThru
    $until = [DateTime]::UtcNow.AddSeconds(10)
    while ([DateTime]::UtcNow -lt $until -and ((Get-Content $output -ErrorAction SilentlyContinue) -notcontains 'suspended')) {
        Start-Sleep -Milliseconds 100
    }
    if ((Get-Content $output) -notcontains 'suspended') { throw 'Listener did not wait for unlock' }
    Stop-Process -Id $child.Id -Force
    $child.WaitForExit()
    if ((Get-Content $output) -contains 'ready') { throw 'Paused listener resumed unexpectedly' }
    Write-Output 'PASS: stopping a suspended helper prevents automatic resume'
} finally {
    if ($null -ne $child -and !$child.HasExited) { Stop-Process -Id $child.Id -Force }
    Remove-Item Env:HANDY_WAKE_PHRASE,Env:HANDY_WAKE_PARENT,Env:HANDY_WAKE_TEST_WAV,Env:HANDY_WAKE_TEST_SESSION_STEPS,Env:HANDY_WAKE_TEST_PROBE -ErrorAction SilentlyContinue
    Remove-Item $testDir -Recurse -Force
}
