# Generic UI driver for the interactive session (scheduled task CAD-UIActions).
# Reads $env:USERPROFILE\work\tools\ui-actions.json: {"actions":[{"op":"activate"},{"op":"click","x":641,"y":47},
#   {"op":"sleep","ms":800},{"op":"snap","path":"C:\\...\\a.png"},{"op":"keys","text":"%{F8}"},{"op":"type","text":"C:\\path"}]}
# "keys" = SendKeys syntax, "type" = literal text (SendKeys special characters escaped). Writes ui-result.json.
$tools = "$env:USERPROFILE\work\tools"
$req = Get-Content "$tools\ui-actions.json" -Raw -Encoding UTF8 | ConvertFrom-Json
$result = @{ ok = $false; error = $null; done = 0; started = (Get-Date -Format "yyyy-MM-dd HH:mm:ss") }
try {
  Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public class UIDrv {
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int X, int Y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint dwFlags, uint dx, uint dy, uint dwData, UIntPtr dwExtraInfo);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindowW(string cls, string title);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT r);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }
  [DllImport("user32.dll")] public static extern int GetWindowText(IntPtr hWnd, System.Text.StringBuilder text, int count);
  [DllImport("user32.dll")] public static extern bool GetCursorPos(out POINT lpPoint);
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X; public int Y; }
}
"@
  [UIDrv]::SetProcessDPIAware() | Out-Null
  Add-Type -AssemblyName System.Windows.Forms
  Add-Type -AssemblyName System.Drawing
  function Snap($path) {
    $b = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
    $bmp = New-Object System.Drawing.Bitmap $b.Width, $b.Height
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($b.Left, $b.Top, 0, 0, $b.Size)
    $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $bmp.Dispose()
  }
  function Click($x, $y, $dbl) {
    [UIDrv]::SetCursorPos($x, $y) | Out-Null
    Start-Sleep -Milliseconds 120
    [UIDrv]::mouse_event(2, 0, 0, 0, [UIntPtr]::Zero); [UIDrv]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero)
    if ($dbl) { Start-Sleep -Milliseconds 90; [UIDrv]::mouse_event(2, 0, 0, 0, [UIntPtr]::Zero); [UIDrv]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero) }
  }
  function Esc($s) { return ($s -replace '([+^%~(){}\[\]])', '{$1}') }
  foreach ($a in $req.actions) {
    switch ($a.op) {
      "activate" {
        $p = Get-Process -Name ugraf -ErrorAction Stop | Where-Object { $_.MainWindowHandle -ne 0 } | Sort-Object StartTime -Descending | Select-Object -First 1
        if (-not $p) { throw "no NX window" }
        $h = $p.MainWindowHandle
        if ([UIDrv]::IsIconic($h)) { [UIDrv]::ShowWindow($h, 9) | Out-Null }
        [UIDrv]::SetForegroundWindow($h) | Out-Null
        Start-Sleep -Milliseconds 800
        $nr = New-Object UIDrv+RECT; [UIDrv]::GetWindowRect($h, [ref]$nr) | Out-Null
        $result.nxrect = @($nr.Left, $nr.Top, $nr.Right, $nr.Bottom); $script:nx = $nr
        Click ([int](($nr.Left + $nr.Right) / 2)) ($nr.Top + 12) $false
        Start-Sleep -Milliseconds 600
      }
      "click"  { Click ([int]$a.x) ([int]$a.y) $false }
      "findwin" {
        $hw = [UIDrv]::FindWindowW($null, $a.title)
        if ($hw -eq [IntPtr]::Zero) { if ($a.require) { throw ("window not found: " + $a.title) } else { $script:win = $null } }
        else { $rr = New-Object UIDrv+RECT; [UIDrv]::GetWindowRect($hw, [ref]$rr) | Out-Null; $script:win = $rr; $result.found = @($rr.Left, $rr.Top, $rr.Right, $rr.Bottom) }
      }
      "requirepixel" {
        $bm = New-Object System.Drawing.Bitmap 1, 1; $gg = [System.Drawing.Graphics]::FromImage($bm)
        $gg.CopyFromScreen([int]$a.x, [int]$a.y, 0, 0, (New-Object System.Drawing.Size 1, 1)); $c = $bm.GetPixel(0, 0); $gg.Dispose(); $bm.Dispose()
        $result.pixel = @($c.R, $c.G, $c.B)
        $dr = [Math]::Abs($c.R - [int]$a.rgb[0]) + [Math]::Abs($c.G - [int]$a.rgb[1]) + [Math]::Abs($c.B - [int]$a.rgb[2])
        if ($dr -gt [int]$a.tol) { throw ("pixel guard failed at " + $a.x + "," + $a.y + ": " + $c.R + "," + $c.G + "," + $c.B) }
      }
      "requirepixelnx" {
        if ($script:nx -eq $null) { throw "requirepixelnx before activate" }
        $ax = $script:nx.Left + [int]$a.x; $ay = $script:nx.Top + [int]$a.y
        $bm = New-Object System.Drawing.Bitmap 1, 1; $gg = [System.Drawing.Graphics]::FromImage($bm)
        $gg.CopyFromScreen($ax, $ay, 0, 0, (New-Object System.Drawing.Size 1, 1)); $c = $bm.GetPixel(0, 0); $gg.Dispose(); $bm.Dispose()
        $result.pixel = @($c.R, $c.G, $c.B)
        $dr = [Math]::Abs($c.R - [int]$a.rgb[0]) + [Math]::Abs($c.G - [int]$a.rgb[1]) + [Math]::Abs($c.B - [int]$a.rgb[2])
        if ($dr -gt [int]$a.tol) { throw ("pixel guard failed at " + $ax + "," + $ay + ": " + $c.R + "," + $c.G + "," + $c.B) }
      }
      "clicknx" { if ($script:nx -eq $null) { throw "clicknx before activate" }; Click ($script:nx.Left + [int]$a.x) ($script:nx.Top + [int]$a.y) $false }
      "clickwin" { if ($script:win -eq $null) { throw "clickwin without window" }; Click ($script:win.Left + [int]$a.x) ($script:win.Top + [int]$a.y) $false }
      "dblclick" { Click ([int]$a.x) ([int]$a.y) $true }
      "sleep"  { Start-Sleep -Milliseconds ([int]$a.ms) }
      "snap"   { Snap $a.path }
      "keys"   { [System.Windows.Forms.SendKeys]::SendWait($a.text) }
      "type"   { [System.Windows.Forms.SendKeys]::SendWait((Esc $a.text)) }
    }
    $result.done++
  }
  $fg = [UIDrv]::GetForegroundWindow(); $sb = New-Object System.Text.StringBuilder 256; [UIDrv]::GetWindowText($fg, $sb, 256) | Out-Null
  $result.foreground = $sb.ToString()
  $pt = New-Object UIDrv+POINT; [UIDrv]::GetCursorPos([ref]$pt) | Out-Null; $result.cursor = @($pt.X, $pt.Y)
  $result.user = [Security.Principal.WindowsIdentity]::GetCurrent().Name
  $result.elevated = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
  $result.ok = $true
} catch {
  $result.error = $_.Exception.Message
}
$result | ConvertTo-Json -Compress | Out-File -Encoding utf8 "$tools\ui-result.json"
