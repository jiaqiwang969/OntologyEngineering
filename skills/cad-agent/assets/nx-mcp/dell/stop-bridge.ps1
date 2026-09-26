# Ask the running NX_MCP bridge journal to stop (it polls for this flag), then wait for NX to exit.
$dir  = "$env:USERPROFILE\work\nxmcp"
$desc = Join-Path $env:LOCALAPPDATA "nx-mcp\bridge.json"
$pidv = $null
if (Test-Path $desc) { try { $pidv = (Get-Content $desc -Raw | ConvertFrom-Json).pid } catch {} }
New-Item -ItemType File -Force -Path "$dir\stop.flag" | Out-Null
Write-Output "stop flag written"
for ($i = 0; $i -lt 60; $i++) {
    Start-Sleep 1
    $alive = $pidv -and (Get-Process -Id $pidv -ErrorAction SilentlyContinue)
    if (-not $alive -and -not (Test-Path $desc)) { Write-Output "bridge stopped"; exit 0 }
}
Write-Output "bridge still alive after 60 s (pid $pidv); descriptor present: $(Test-Path $desc)"; exit 1
