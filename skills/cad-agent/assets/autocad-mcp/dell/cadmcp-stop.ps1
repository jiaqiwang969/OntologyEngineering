# Stops the relay task and any relay/server python processes of the cad-mcp venv. AutoCAD itself is left running.
Stop-ScheduledTask -TaskName "CadMcpRelay" -ErrorAction SilentlyContinue
Get-CimInstance Win32_Process -Filter "name='python.exe' or name='pythonw.exe'" |
  Where-Object { $_.CommandLine -match "cadmcp\\(relay_server|acad_mcp_server)\.py" } |
  ForEach-Object { "killing pid $($_.ProcessId)"; Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
"relay stopped"
