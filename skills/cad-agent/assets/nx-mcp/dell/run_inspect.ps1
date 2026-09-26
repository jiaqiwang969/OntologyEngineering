# Launch the standalone inspection journal detached (survives the SSH session). Usage:
#   powershell -File run_inspect.ps1 -Asm <prt> -Out <json>
# Do not start two runs at once: the journal reads the config path from INSPECT_CFG (per-run file),
# and the fixed inspect_config.json is also refreshed as a fallback.
param([Parameter(Mandatory=$true)][string]$Asm, [Parameter(Mandatory=$true)][string]$Out, [int]$VolumeMaxBytes = 5000000, [switch]$NoVolume, [switch]$NoOccBbox)
$cfg = @{ asm = $Asm; out = $Out; log = "$Out.log"; volume = (-not $NoVolume); volume_max_bytes = $VolumeMaxBytes; occ_bbox = (-not $NoOccBbox) }
$json = $cfg | ConvertTo-Json
$cfgPath = "$Out.config.json"
$journalDir = Join-Path $env:USERPROFILE 'work\journals'
[IO.File]::WriteAllText($cfgPath, $json, (New-Object Text.UTF8Encoding $false))
[IO.File]::WriteAllText((Join-Path $journalDir 'inspect_config.json'), $json, (New-Object Text.UTF8Encoding $false))
Remove-Item $Out, "$Out.log", "$Out.FATAL.txt" -ErrorAction SilentlyContinue
$stdout = "$Out.stdout.txt"
$cmd = 'cmd.exe /c "set INSPECT_CFG={0}&& "C:\Program Files\Siemens\NX 2412\NXBIN\run_journal.exe" "{1}" > "{2}" 2>&1"' -f $cfgPath, (Join-Path $journalDir 'inspect_asm_standalone.py'), $stdout
$r = Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{ CommandLine = $cmd; CurrentDirectory = $journalDir }
Write-Output ("launched run_journal pid={0} rc={1}" -f $r.ProcessId, $r.ReturnValue)
