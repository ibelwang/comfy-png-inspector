# Start the viewer, bring its window to the front, screenshot just that window, close it.
param(
    [Parameter(Mandatory = $true)][string]$Exe,
    [string]$Image = '',
    [Parameter(Mandatory = $true)][string]$Out
)

Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public struct RECT { public int Left, Top, Right, Bottom; }
public static class Win32 {
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int cmd);
}
"@

$arguments = if ([string]::IsNullOrWhiteSpace($Image)) { @() } else { @('"' + $Image + '"') }
$p = Start-Process -FilePath $Exe -ArgumentList $arguments `
    -RedirectStandardOutput ($Out + '.out.log') -RedirectStandardError ($Out + '.err.log') `
    -PassThru -NoNewWindow
Start-Sleep -Seconds 3
$p.Refresh()
if ($p.HasExited) {
    Write-Output "EXITED code=$($p.ExitCode)"
    Get-Content ($Out + '.err.log') -ErrorAction SilentlyContinue | Select-Object -First 20
    exit 1
}
$handle = $p.MainWindowHandle
if ($handle -eq [IntPtr]::Zero) {
    Write-Output "NO WINDOW"
    $p.Kill()
    exit 1
}

[Win32]::ShowWindow($handle, 9) | Out-Null   # SW_RESTORE
[Win32]::SetForegroundWindow($handle) | Out-Null
Start-Sleep -Milliseconds 700

$rect = New-Object RECT
[Win32]::GetWindowRect($handle, [ref]$rect) | Out-Null
$width = $rect.Right - $rect.Left
$height = $rect.Bottom - $rect.Top
Write-Output "WINDOW ${width}x${height}"

$bitmap = New-Object System.Drawing.Bitmap $width, $height
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
$graphics.CopyFromScreen($rect.Left, $rect.Top, 0, 0, (New-Object System.Drawing.Size $width, $height))
$bitmap.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$graphics.Dispose()
$bitmap.Dispose()

$p.Kill()
Write-Output "SAVED $Out"
