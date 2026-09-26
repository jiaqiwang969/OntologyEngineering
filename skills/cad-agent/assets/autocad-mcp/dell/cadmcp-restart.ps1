# Restarts the relay (kills leftover relay/server pythons of the cad-mcp venv first).
$base = "$env:USERPROFILE\work\cadmcp"
Stop-ScheduledTask -TaskName "CadMcpRelay" -ErrorAction SilentlyContinue
Get-CimInstance Win32_Process -Filter "name='python.exe' or name='pythonw.exe'" |
  Where-Object { $_.CommandLine -match "cadmcp\\(relay_server|acad_mcp_server)\.py" } |
  ForEach-Object { "killing pid $($_.ProcessId): $($_.CommandLine)"; Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
Start-Sleep -Seconds 1
Start-ScheduledTask -TaskName "CadMcpRelay"
Start-Sleep -Seconds 3
& "$base\cadmcp-status.ps1"
