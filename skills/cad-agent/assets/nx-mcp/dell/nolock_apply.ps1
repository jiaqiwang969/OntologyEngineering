# Disable the screen saver for user jiaqi (it was Mystify.scr after 300 s idle; a running screen saver blocks
# SendInput/screen capture from automation). Registry + live SystemParametersInfo so no logoff is needed.
$ErrorActionPreference = "SilentlyContinue"
Set-ItemProperty "HKCU:\Control Panel\Desktop" -Name ScreenSaveActive -Value "0"
Set-ItemProperty "HKCU:\Control Panel\Desktop" -Name ScreenSaveTimeOut -Value "0"
Remove-ItemProperty "HKCU:\Control Panel\Desktop" -Name "SCRNSAVE.EXE"
Add-Type -TypeDefinition @"
using System; using System.Runtime.InteropServices;
public class SPI { [DllImport("user32.dll", SetLastError=true)] public static extern bool SystemParametersInfo(uint a, uint p, IntPtr v, uint f);
  [DllImport("user32.dll", SetLastError=true)] public static extern bool SystemParametersInfoGet(uint a, uint p, ref bool v, uint f);
  [DllImport("user32.dll", EntryPoint="SystemParametersInfo")] public static extern bool GetBool(uint a, uint p, ref bool v, uint f); }
"@
$ok = [SPI]::SystemParametersInfo(17, 0, [IntPtr]::Zero, 3)   # SPI_SETSCREENSAVEACTIVE = 17, off, update ini + broadcast
$ok2 = [SPI]::SystemParametersInfo(15, 0, [IntPtr]::Zero, 3)  # SPI_SETSCREENSAVETIMEOUT = 15 -> 0
$act = $false; [SPI]::GetBool(16, 0, [ref]$act, 0) | Out-Null   # SPI_GETSCREENSAVEACTIVE
$d = Get-ItemProperty "HKCU:\Control Panel\Desktop"
$line = "session={0} set_active_ok={1} set_timeout_ok={2} live_active={3} reg ScreenSaveActive={4} TimeOut={5} SCRNSAVE={6} at {7}" -f (Get-Process -Id $PID).SessionId, $ok, $ok2, $act, $d.ScreenSaveActive, $d.ScreenSaveTimeOut, $d."SCRNSAVE.EXE", (Get-Date -Format "HH:mm:ss")
$line | Out-File -Encoding utf8 -Append "$env:USERPROFILE\work\tools\nolock-result.txt"
Write-Output $line
