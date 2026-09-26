$ErrorActionPreference = "SilentlyContinue"
"=== power scheme ==="; powercfg /getactivescheme
foreach ($q in @(@("SUB_VIDEO","VIDEOIDLE","显示器关闭"), @("SUB_SLEEP","STANDBYIDLE","睡眠"), @("SUB_SLEEP","HIBERNATEIDLE","休眠"), @("SUB_NONE","CONSOLELOCK","唤醒需登录(CONSOLELOCK)"))) {
  $o = powercfg /query SCHEME_CURRENT $q[0] $q[1] 2>$null | Select-String "Power Setting Index|电源设置索引"
  "{0}: {1}" -f $q[2], (($o | ForEach-Object { $_.Line.Trim() }) -join " | ")
}
"=== screen saver (HKCU) ==="
$d = Get-ItemProperty "HKCU:\Control Panel\Desktop"
"ScreenSaveActive={0} ScreenSaverIsSecure={1} ScreenSaveTimeOut={2} SCRNSAVE.EXE={3}" -f $d.ScreenSaveActive, $d.ScreenSaverIsSecure, $d.ScreenSaveTimeOut, $d."SCRNSAVE.EXE"
"=== policies ==="
$p = Get-ItemProperty "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System"; "InactivityTimeoutSecs={0}" -f $p.InactivityTimeoutSecs
$pd = Get-ItemProperty "HKCU:\Software\Policies\Microsoft\Windows\Control Panel\Desktop"; if ($pd) { "HKCU policy desktop: " + (($pd.PSObject.Properties | Where-Object { $_.Name -notlike "PS*" } | ForEach-Object { $_.Name + "=" + $_.Value }) -join ", ") } else { "HKCU policy desktop: none" }
$pm = Get-ItemProperty "HKLM:\SOFTWARE\Policies\Microsoft\Windows\Control Panel\Desktop"; if ($pm) { "HKLM policy desktop: " + (($pm.PSObject.Properties | Where-Object { $_.Name -notlike "PS*" } | ForEach-Object { $_.Name + "=" + $_.Value }) -join ", ") } else { "HKLM policy desktop: none" }
$w = Get-ItemProperty "HKCU:\Software\Microsoft\Windows NT\CurrentVersion\Winlogon"; "DynamicLock EnableGoodbye={0}" -f $w.EnableGoodbye
"=== MDM / domain ==="
"PartOfDomain={0}" -f (Get-CimInstance Win32_ComputerSystem).PartOfDomain
"MDM enrollments: {0}" -f ((Get-ChildItem "HKLM:\SOFTWARE\Microsoft\Enrollments" -ErrorAction SilentlyContinue | Measure-Object).Count)
"=== session / lock state ==="
"LogonUI running={0}" -f ((Get-Process LogonUI -ErrorAction SilentlyContinue) -ne $null)
quser 2>$null
