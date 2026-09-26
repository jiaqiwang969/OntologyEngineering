# Health check for the AutoCAD MCP relay. Prints task state, listener, AutoCAD process and recent log lines.
$base = "$env:USERPROFILE\work\cadmcp"
$t = Get-ScheduledTask -TaskName "CadMcpRelay" -ErrorAction SilentlyContinue
if ($t) { "task CadMcpRelay: $($t.State)" } else { "task CadMcpRelay: NOT REGISTERED (run cadmcp-install-task.ps1)" }
$l = Get-NetTCPConnection -LocalPort 39901 -State Listen -ErrorAction SilentlyContinue
if ($l) { "relay: listening on 127.0.0.1:39901 (pid $($l.OwningProcess), session $((Get-Process -Id $l.OwningProcess).SessionId))" } else { "relay: NOT listening" }
$a = Get-Process acad -ErrorAction SilentlyContinue
if ($a) { $a | ForEach-Object { "acad: pid $($_.Id) session $($_.SessionId) responding=$($_.Responding) title='$($_.MainWindowTitle)'" } } else { "acad: not running (start with Start-ScheduledTask CadMcp-StartAcad)" }
"--- relay.log (tail)"
Get-Content "$base\logs\relay.log" -Tail 8 -ErrorAction SilentlyContinue
"--- acad_mcp_server.log (tail)"
Get-Content "$base\logs\acad_mcp_server.log" -Tail 8 -ErrorAction SilentlyContinue
