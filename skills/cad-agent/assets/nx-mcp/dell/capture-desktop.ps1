# Runs inside the interactive session (scheduled task, LogonType Interactive).
# Captures the whole virtual desktop to PNG and lists top-level windows with titles.
$out = "$env:USERPROFILE\work\tools"
New-Item -ItemType Directory -Force -Path $out | Out-Null
Add-Type -TypeDefinition 'using System.Runtime.InteropServices; public class DpiFix { [DllImport("user32.dll")] public static extern bool SetProcessDPIAware(); }'
[DpiFix]::SetProcessDPIAware() | Out-Null
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
$vs = [System.Windows.Forms.SystemInformation]::VirtualScreen
$bmp = New-Object System.Drawing.Bitmap $vs.Width, $vs.Height
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($vs.Left, $vs.Top, 0, 0, $vs.Size)
$bmp.Save("$out\desktop.png", [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose(); $bmp.Dispose()
$lines = @("captured " + (Get-Date -Format "yyyy-MM-dd HH:mm:ss") + " virtual=" + $vs.Width + "x" + $vs.Height)
$lines += Get-Process | Where-Object { $_.MainWindowTitle } | ForEach-Object { "{0,6} {1,-14} [{2}]" -f $_.Id, $_.ProcessName, $_.MainWindowTitle }
$lines | Out-File -Encoding utf8 "$out\windows.txt"
