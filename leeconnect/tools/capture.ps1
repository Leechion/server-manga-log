param(
    [string]$OutPath = "leeconnect\target\ui-v2\screenshot.png"
)

Add-Type -AssemblyName System.Drawing
Add-Type -ReferencedAssemblies System.Drawing -TypeDefinition @"
using System;
using System.Drawing;
using System.Runtime.InteropServices;

public class WinSnap {
    [DllImport("user32.dll")] public static extern IntPtr FindWindow(string lpClassName, string lpWindowName);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern IntPtr GetWindowDC(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern int ReleaseDC(IntPtr hWnd, IntPtr hDC);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hwnd, IntPtr hdcBlt, uint nFlags);
    [DllImport("gdi32.dll")] public static extern bool BitBlt(IntPtr hdcDest, int nXDest, int nYDest, int nWidth, int nHeight, IntPtr hdcSrc, int nXSrc, int nYSrc, int dwRop);

    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }

    public static Bitmap Snap(IntPtr hWnd) {
        RECT rect;
        GetWindowRect(hWnd, out rect);
        int width = rect.Right - rect.Left;
        int height = rect.Bottom - rect.Top;
        if (width <= 0 || height <= 0) return null;

        Bitmap bmp = new Bitmap(width, height);
        using (Graphics g = Graphics.FromImage(bmp)) {
            IntPtr hdcDest = g.GetHdc();
            bool ok = PrintWindow(hWnd, hdcDest, 2);
            if (!ok) {
                IntPtr hdcSrc = GetWindowDC(hWnd);
                BitBlt(hdcDest, 0, 0, width, height, hdcSrc, 0, 0, 0x00CC0020);
                ReleaseDC(hWnd, hdcSrc);
            }
            g.ReleaseHdc(hdcDest);
        }
        return bmp;
    }
}
"@ -ErrorAction SilentlyContinue

$hwnd = [IntPtr]::Zero
$procs = Get-Process -Name "gui" -ErrorAction SilentlyContinue
foreach ($p in $procs) {
    if ($p.MainWindowHandle -ne [IntPtr]::Zero) {
        $hwnd = $p.MainWindowHandle
        break
    }
}

if ($hwnd -eq [IntPtr]::Zero) {
    $hwnd = [WinSnap]::FindWindow($null, "LeeConnect")
}

if ($hwnd -eq [IntPtr]::Zero) {
    Write-Error "LeeConnect window not found"
    exit 1
}

[WinSnap]::SetForegroundWindow($hwnd) | Out-Null
Start-Sleep -Milliseconds 200

$bmp = [WinSnap]::Snap($hwnd)
if ($bmp -eq $null) {
    Write-Error "Failed to capture window"
    exit 1
}

$dir = [System.IO.Path]::GetDirectoryName($OutPath)
if (-not [string]::IsNullOrEmpty($dir) -and -not (Test-Path $dir)) {
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
}

$bmp.Save($OutPath, [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()
Write-Host "Screenshot saved to $OutPath"
