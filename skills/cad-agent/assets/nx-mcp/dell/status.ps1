# Report NX_MCP bridge state: descriptor, loopback port answer, run_journal processes.
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$desc = Join-Path $env:LOCALAPPDATA "nx-mcp\bridge.json"
if (Test-Path $desc) {
    $d = Get-Content $desc -Raw | ConvertFrom-Json
    $ok = $false
    try { $c = New-Object Net.Sockets.TcpClient; $c.Connect("127.0.0.1", $d.port); $c.Close(); $ok = $true } catch {}
    Write-Output ("descriptor: port={0} pid={1} nx={2} protocol={3}" -f $d.port, $d.pid, $d.nx_version, $d.protocol_version)
    Write-Output ("port answers: {0}" -f $ok)
    Write-Output ("bridge pid alive: {0}" -f [bool](Get-Process -Id $d.pid -ErrorAction SilentlyContinue))
} else { Write-Output "no descriptor at $desc (bridge not running)" }
Write-Output ("stop flag present: {0}" -f (Test-Path "$env:USERPROFILE\work\nxmcp\stop.flag"))
Get-Process run_journal, ugraf -ErrorAction SilentlyContinue | Select-Object Name, Id, StartTime, SessionId | Format-Table -AutoSize | Out-String -Width 120
