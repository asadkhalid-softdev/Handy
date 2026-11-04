# Interactive Windows integration test for the standalone preview. The optional
# switch briefly opens the default mic, then verifies Pause kills the detector.
param([switch]$ExerciseMicrophone)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class HandyVoiceWindowProbe {
    [DllImport("user32.dll", CharSet=CharSet.Unicode)]
    private static extern IntPtr FindWindow(string cls, string title);
    [DllImport("user32.dll", EntryPoint="GetWindowLongW")]
    public static extern int GetWindowLong(IntPtr hwnd, int index);
    public static IntPtr Find() { return FindWindow(null, "Handy Voice Control"); }
}
'@
$rootPath = Split-Path $PSScriptRoot -Parent
$executable = Join-Path $rootPath 'src-tauri\target\voice-preview\Handy Voice.exe'
if (!(Test-Path $executable)) { throw 'Build the standalone preview first.' }
if (Get-Process -Name 'Handy Voice' -ErrorAction SilentlyContinue) {
    throw 'Quit the running preview before this test. Official Handy can remain running.'
}
$process = Start-Process $executable -ArgumentList '--start-hidden' -PassThru
try {
    $deadline = [DateTime]::UtcNow.AddSeconds(20)
    $hwnd = [IntPtr]::Zero
    while ($hwnd -eq [IntPtr]::Zero -and [DateTime]::UtcNow -lt $deadline) {
        Start-Sleep -Milliseconds 200
        $hwnd = [HandyVoiceWindowProbe]::Find()
    }
    if ($hwnd -eq [IntPtr]::Zero) { throw 'Floating control window was not created.' }
    $root = [System.Windows.Automation.AutomationElement]::FromHandle($hwnd)
    function Wait-Label([string]$label) {
        $until = [DateTime]::UtcNow.AddSeconds(15)
        while ([DateTime]::UtcNow -lt $until) {
            $condition = New-Object System.Windows.Automation.PropertyCondition(
                [System.Windows.Automation.AutomationElement]::NameProperty, $label)
            $found = $root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
            if ($null -ne $found) { return $found }
            Start-Sleep -Milliseconds 200
        }
        throw "Control did not reach state: $label (this test requires an English UI)."
    }
    $null = Wait-Label 'Microphone paused'
    $style = [HandyVoiceWindowProbe]::GetWindowLong($hwnd, -20)
    if (!($style -band 8)) { throw 'Control is not always on top.' }
    if (!($style -band 134217728)) { throw 'Control could steal the typing focus.' }
    Write-Output 'PASS: visible, paused, topmost control without activation'
    if ($ExerciseMicrophone) {
        $resume = Wait-Label 'Resume microphone listening'
        $resume.GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Toggle()
        $null = Wait-Label 'Listening for wake phrase'
        $helpers = @(Get-CimInstance Win32_Process -Filter "ParentProcessId = $($process.Id)" |
            Where-Object Name -EQ 'powershell.exe')
        if ($helpers.Count -eq 0) { throw 'Detector helper did not start.' }
        $pause = Wait-Label 'Pause microphone listening'
        $pause.GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Toggle()
        $null = Wait-Label 'Microphone paused'
        Start-Sleep -Milliseconds 500
        $helpers = @(Get-CimInstance Win32_Process -Filter "ParentProcessId = $($process.Id)" |
            Where-Object Name -EQ 'powershell.exe')
        if ($helpers.Count -ne 0) { throw 'Pause left a wake detector running.' }
        Write-Output 'PASS: native resume/pause; no detector process after pause'
    }
} finally {
    Stop-Process -Id $process.Id -ErrorAction SilentlyContinue
}
