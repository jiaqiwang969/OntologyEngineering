# Start the NX_MCP batch bridge (headless NX via run_journal) detached from this SSH session.
# Usage: powershell -File start-bridge.ps1 [journal.py]   (default: NX_MCP\examples\start_nx_bridge.py)
param([string]$Journal = "$env:USERPROFILE\work\NX_MCP\examples\start_nx_bridge.py")
$ErrorActionPreference = "Stop"
$dir  = "$env:USERPROFILE\work\nxmcp"
$stop = "$dir\stop.flag"
$desc = Join-Path $env:LOCALAPPDATA "nx-mcp\bridge.json"
New-Item -ItemType Directory -Force -Path $dir, "$env:USERPROFILE\work\journals" | Out-Null
if (Test-Path $desc) {
    try {
        $d = Get-Content $desc -Raw | ConvertFrom-Json
        $c = New-Object Net.Sockets.TcpClient; $c.Connect("127.0.0.1", $d.port); $c.Close()
        Write-Output "bridge already running: port $($d.port) pid $($d.pid) ($($d.nx_version))"; exit 0
    } catch { Remove-Item $desc -ErrorAction SilentlyContinue }
}
Remove-Item $stop -ErrorAction SilentlyContinue
Add-Content "$dir\bridge.log" ("`n===== {0} start-bridge {1}" -f (Get-Date -Format s), $Journal)
$cmd = 'cmd.exe /c ""{0}\bridge.cmd" "{1}""' -f $dir, $Journal
$r = Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{ CommandLine = $cmd; CurrentDirectory = $dir }
if ($r.ReturnValue -ne 0) { Write-Output "Win32_Process.Create failed: $($r.ReturnValue)"; exit 1 }
Write-Output "launcher pid $($r.ProcessId); waiting for $desc"
for ($i = 0; $i -lt 120; $i++) {
    Start-Sleep 1
    if (Test-Path $desc) { Get-Content $desc -Raw; exit 0 }
    if (-not (Get-Process -Id $r.ProcessId -ErrorAction SilentlyContinue)) {
        Write-Output "launcher exited before the bridge came up; tail of bridge.log:"
        Get-Content "$dir\bridge.log" -Tail 30; exit 1
    }
}
Write-Output "timeout waiting for bridge descriptor; tail of bridge.log:"; Get-Content "$dir\bridge.log" -Tail 30; exit 1
