# Captures BananaV's own OS windows (main window and dialogs) to PNG files,
# to check native dialog windows, which the headless tests embed instead.
# It starts a new BananaV process, never touches the mouse or keyboard,
# captures only that process's windows and then closes it.
#
#   ./tools/capture_windows.ps1 -Exe target/release/bananav.exe `
#       -Open "Options.Preferences;Options.CustomizeKeyboard" -OutDir target/janelas
#
# -Open lists Global keyboard-map ids run at startup (BANANAV_OPEN).
param(
    [string]$Exe,
    [string]$Open,
    [string]$OutDir,
    [int]$WaitMs = 5000
)
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public static class Win {
    public delegate bool EnumProc(IntPtr h, IntPtr l);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc f, IntPtr l);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] public static extern IntPtr GetWindow(IntPtr h, uint cmd);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint flags);
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
    public static List<IntPtr> Of(uint pid) {
        var list = new List<IntPtr>();
        EnumWindows((h, l) => { uint p; GetWindowThreadProcessId(h, out p); if (p == pid && IsWindowVisible(h)) list.Add(h); return true; }, IntPtr.Zero);
        return list;
    }
    public static string Title(IntPtr h) { var sb = new StringBuilder(512); GetWindowText(h, sb, 512); return sb.ToString(); }
}
"@
$env:BANANAV_OPEN = $Open
$p = Start-Process -FilePath $Exe -PassThru
Start-Sleep -Milliseconds $WaitMs
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$i = 0
$titles = @{}
foreach ($h in [Win]::Of([uint32]$p.Id)) {
    $t = [Win]::Title($h)
    $titles[$h.ToInt64()] = $t
}
foreach ($h in [Win]::Of([uint32]$p.Id)) {
    $r = New-Object Win+RECT
    [Win]::GetWindowRect($h, [ref]$r) | Out-Null
    $w = $r.R - $r.L; $hh = $r.B - $r.T
    if ($w -le 0 -or $hh -le 0) { continue }
    $bmp = New-Object System.Drawing.Bitmap $w, $hh
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $dc = $g.GetHdc()
    [Win]::PrintWindow($h, $dc, 2) | Out-Null
    $g.ReleaseHdc($dc); $g.Dispose()
    $owner = [Win]::GetWindow($h, 4)
    $ownerTitle = if ($owner -ne [IntPtr]::Zero) { [Win]::Title($owner) } else { "-" }
    $file = Join-Path $OutDir ("win_{0}.png" -f $i)
    $bmp.Save($file, [System.Drawing.Imaging.ImageFormat]::Png); $bmp.Dispose()
    "{0}: '{1}' {2}x{3} at {4},{5} owner='{6}' -> {7}" -f $i, [Win]::Title($h), $w, $hh, $r.L, $r.T, $ownerTitle, $file
    $i++
}
Stop-Process -Id $p.Id -Force
