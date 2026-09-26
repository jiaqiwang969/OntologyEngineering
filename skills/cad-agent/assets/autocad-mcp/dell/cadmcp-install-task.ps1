# Registers/starts the CadMcpRelay scheduled task: runs relay_server.py in the caller's interactive session
# (so it can reach AutoCAD's COM server on the desktop). Idempotent; run from any session.
$ErrorActionPreference = "Stop"
$base = "$env:USERPROFILE\work\cadmcp"
$pyw  = "$env:USERPROFILE\work\venvs\cad-mcp\Scripts\pythonw.exe"
$name = "CadMcpRelay"

$action    = New-ScheduledTaskAction -Execute $pyw -Argument "`"$base\relay_server.py`"" -WorkingDirectory $base
$principal = New-ScheduledTaskPrincipal -UserId $env:USERNAME -LogonType Interactive -RunLevel Limited
$trigger   = New-ScheduledTaskTrigger -AtLogOn -User $env:USERNAME
$settings  = New-ScheduledTaskSettingsSet -ExecutionTimeLimit (New-TimeSpan -Seconds 0) `
             -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -MultipleInstances IgnoreNew `
             -RestartCount 3 -RestartInterval (New-TimeSpan -Minutes 1) -StartWhenAvailable
Register-ScheduledTask -TaskName $name -Action $action -Principal $principal -Trigger $trigger -Settings $settings -Force | Out-Null
Start-ScheduledTask -TaskName $name
Start-Sleep -Seconds 3
$t = Get-ScheduledTask -TaskName $name
"task $name state: $($t.State)"
$port = 39901
$l = Get-NetTCPConnection -LocalPort $port -State Listen -ErrorAction SilentlyContinue
if ($l) { "relay listening on 127.0.0.1:$port (pid $($l.OwningProcess))" } else { "WARNING: nothing listening on $port yet; see $base\logs\relay.log" }
