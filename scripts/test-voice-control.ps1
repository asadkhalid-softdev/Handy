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
    public static IntPtr FindMain() { return FindWindow(null, "Handy"); }
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
}
'@
$rootPath = Split-Path $PSScriptRoot -Parent
$executable = Join-Path $rootPath 'src-tauri\target\voice-preview\Handy Voice.exe'
if (!(Test-Path $executable)) { throw 'Build the standalone preview first.' }
$image = [IO.File]::ReadAllBytes($executable)
$peOffset = [BitConverter]::ToInt32($image, 0x3c)
$subsystem = [BitConverter]::ToUInt16($image, $peOffset + 24 + 68)
if ($subsystem -ne 2) { throw 'The packaged preview must use the Windows GUI subsystem, without a console.' }
Write-Output 'PASS: packaged preview does not allocate a console'
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
    function Wait-Label([string]$label, [bool]$requireToggle = $false) {
        $until = [DateTime]::UtcNow.AddSeconds(15)
        while ([DateTime]::UtcNow -lt $until) {
            $condition = New-Object System.Windows.Automation.PropertyCondition(
                [System.Windows.Automation.AutomationElement]::NameProperty, $label)
            if ($requireToggle) {
                $toggle = New-Object System.Windows.Automation.PropertyCondition(
                    [System.Windows.Automation.AutomationElement]::IsTogglePatternAvailableProperty, $true)
                $condition = New-Object System.Windows.Automation.AndCondition($condition, $toggle)
            }
            $found = $root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
            if ($null -ne $found) { return $found }
            Start-Sleep -Milliseconds 200
        }
        throw "Control did not reach state: $label (this test requires an English UI)."
    }
    $null = Wait-Label 'Resume microphone listening'
    $style = [HandyVoiceWindowProbe]::GetWindowLong($hwnd, -20)
    if (!($style -band 8)) { throw 'Control is not always on top.' }
    if (!($style -band 134217728)) { throw 'Control could steal the typing focus.' }
    if ($root.Current.BoundingRectangle.Width -gt 3 * $root.Current.BoundingRectangle.Height) {
        throw 'Floating control is not compact.'
    }
    Write-Output 'PASS: compact, paused, topmost control without activation'

    $floatingRoot = $root
    $gear = Wait-Label 'Open Wake-Talk settings'
    $gear.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
    Start-Sleep -Milliseconds 500
    $mainWindow = [HandyVoiceWindowProbe]::FindMain()
    if ($mainWindow -eq [IntPtr]::Zero) { throw 'Settings did not open the main window.' }
    $root = [System.Windows.Automation.AutomationElement]::FromHandle($mainWindow)
    $feature = Wait-Label 'Enable Wake-Talk' $true
    $feature.GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Toggle()
    $null = Wait-Label 'Wake-Talk disabled'
    if ([HandyVoiceWindowProbe]::IsWindowVisible($hwnd)) { throw 'Disabled feature left the control visible.' }
    $feature.GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Toggle()
    $null = Wait-Label 'Microphone paused'
    if (![HandyVoiceWindowProbe]::IsWindowVisible($hwnd)) { throw 'Enabling the feature did not restore the control.' }
    $root = $floatingRoot
    Write-Output 'PASS: settings opens Wake-Talk; feature switch hides/restores the control'
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
