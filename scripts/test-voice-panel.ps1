# Native integration check against the running Handy Voice app. Briefly covers
# its floating controls, then verifies automatic recovery without stealing focus.
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Windows.Forms;
public sealed class HandyPanelTestCover : Form {
    protected override bool ShowWithoutActivation { get { return true; } }
}
public static class HandyPanelTest {
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
    [StructLayout(LayoutKind.Sequential)] public struct Point { public int X, Y; }
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] private static extern IntPtr FindWindow(string cls, string title);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr window, out Rect rect);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] private static extern IntPtr WindowFromPoint(Point point);
    [DllImport("user32.dll")] private static extern IntPtr GetAncestor(IntPtr window, uint flags);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr window, IntPtr after, int x, int y, int w, int h, uint flags);
    public static IntPtr Panel() { return FindWindow(null, "Handy Voice Control"); }
    public static IntPtr At(int x, int y) { return GetAncestor(WindowFromPoint(new Point { X = x, Y = y }), 2); }
}
'@ -ReferencedAssemblies System.Windows.Forms
$panel = [HandyPanelTest]::Panel()
if ($panel -eq [IntPtr]::Zero -or ![HandyPanelTest]::IsWindowVisible($panel)) {
    throw 'Enable Wake-Talk and show its floating controls before running this test.'
}
$bounds = New-Object HandyPanelTest+Rect
if (![HandyPanelTest]::GetWindowRect($panel, [ref]$bounds)) { throw 'Cannot read panel bounds' }
$x = $bounds.Left + [int](($bounds.Right - $bounds.Left) / 2)
$y = $bounds.Top + [int](($bounds.Bottom - $bounds.Top) / 2)
$foreground = [HandyPanelTest]::GetForegroundWindow()
$cover = New-Object HandyPanelTestCover
try {
    $cover.ShowInTaskbar = $false
    $cover.FormBorderStyle = [Windows.Forms.FormBorderStyle]::None
    $cover.BackColor = [Drawing.Color]::Fuchsia
    $cover.TopMost = $true
    $cover.StartPosition = [Windows.Forms.FormStartPosition]::Manual
    $cover.Bounds = New-Object Drawing.Rectangle $bounds.Left, $bounds.Top, ($bounds.Right - $bounds.Left), ($bounds.Bottom - $bounds.Top)
    $cover.Show()
    [Windows.Forms.Application]::DoEvents()
    # HWND_TOPMOST + SWP_NOACTIVATE, with exactly the panel's bounds.
    if (![HandyPanelTest]::SetWindowPos($cover.Handle, [IntPtr](-1), $bounds.Left, $bounds.Top,
            ($bounds.Right - $bounds.Left), ($bounds.Bottom - $bounds.Top), 0x10)) {
        throw 'Cannot position the temporary cover window'
    }
    if ([HandyPanelTest]::At($x, $y) -ne $cover.Handle) { throw 'Test window did not cover the controls' }
    $until = [DateTime]::UtcNow.AddSeconds(4)
    while ([DateTime]::UtcNow -lt $until -and [HandyPanelTest]::At($x, $y) -ne $panel) {
        [Windows.Forms.Application]::DoEvents()
        Start-Sleep -Milliseconds 50
    }
    if ([HandyPanelTest]::At($x, $y) -ne $panel) { throw 'Controls did not recover their topmost position' }
    if ([HandyPanelTest]::GetForegroundWindow() -ne $foreground) { throw 'Recovering the controls stole keyboard focus' }
    Write-Output 'PASS: floating controls recover above another topmost window without changing keyboard focus'
    # The client margin must show the magenta window underneath, not a native
    # white frame or an opaque WebView background. CSS shadow may darken it.
    $sample = New-Object Drawing.Bitmap 1, 1
    $graphics = [Drawing.Graphics]::FromImage($sample)
    try {
        $graphics.CopyFromScreen(($bounds.Left + 1), $y, 0, 0, $sample.Size)
        $pixel = $sample.GetPixel(0, 0)
        if ($pixel.G -gt 24 -or $pixel.R -lt 120 -or $pixel.B -lt 120) {
            throw "Panel margin is not transparent: $pixel"
        }
    } finally {
        $graphics.Dispose()
        $sample.Dispose()
    }
    Write-Output 'PASS: floating controls have a transparent margin without an extra native frame'
} finally {
    $cover.Close()
    $cover.Dispose()
}
