param([Parameter(Mandatory=$true)][string]$JobDir,[Parameter(Mandatory=$true)][string]$RootDir,[Parameter(Mandatory=$true)][string]$RunJournal)
$ErrorActionPreference='Stop'
$ProgressPreference='SilentlyContinue'
$statePath=Join-Path $JobDir 'state.json'
if(!(Test-Path -LiteralPath (Join-Path $JobDir 'stage.json'))){throw 'Job upload has not completed staging'}
function Write-State($data) {
  $data.updated_at=[DateTime]::UtcNow.ToString('o')
  $tmp=$statePath+'.tmp'
  [IO.File]::WriteAllText($tmp,($data | ConvertTo-Json -Depth 12),[Text.UTF8Encoding]::new($false))
  Move-Item -LiteralPath $tmp -Destination $statePath -Force
}
# The stable root is shared by all jobs on this host. Never rotate it to evade a hold.
$lock=[IO.File]::Open((Join-Path $RootDir '.nx-writer.lock'),[IO.FileMode]::OpenOrCreate,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None)
try {
  if(Test-Path -LiteralPath $statePath){throw 'Single-use job has already been dispatched; inspect state'}
  Get-ChildItem -LiteralPath $RootDir -Directory | ForEach-Object {
    $f=Join-Path $_.FullName 'state.json'
    if(Test-Path -LiteralPath $f){
      $old=Get-Content -Raw -LiteralPath $f | ConvertFrom-Json
      if($old.state -in @('STARTING','RUNNING','UNKNOWN_AFTER_DISPATCH')){throw ('Unresolved earlier job: '+$_.Name)}
    }
  }
  if(!(Test-Path -LiteralPath $RunJournal -PathType Leaf)){throw 'NX run_journal.exe missing'}
  $spec=Get-Content -Raw -LiteralPath (Join-Path $JobDir 'job.json') | ConvertFrom-Json
  foreach($file in $spec.files.PSObject.Properties){
    $full=Join-Path $JobDir $file.Name
    if((Get-FileHash -LiteralPath $full -Algorithm SHA256).Hash.ToLower() -ne $file.Value.sha256){throw ('Input changed: '+$file.Name)}
  }
  $state=@{schema='cad-agent.nx-direct-state/v1'; state='STARTING'; job_id=$spec.job_id; hostname=$env:COMPUTERNAME; executable=$RunJournal; journal_sha256=$spec.files.'journal.py'.sha256; controller_pid=$PID}
  Write-State $state
  $env:CAD_AGENT_NX_JOB=$JobDir
  $worker=Join-Path $JobDir 'worker.py'
  $process=Start-Process -FilePath $RunJournal -ArgumentList ('"'+$worker+'"') -WorkingDirectory $JobDir -RedirectStandardOutput (Join-Path $JobDir 'stdout.txt') -RedirectStandardError (Join-Path $JobDir 'stderr.txt') -PassThru
  $nativeHandle=$process.Handle
  $state.state='RUNNING'; $state.native_pid=$process.Id; $state.native_start_utc=$process.StartTime.ToUniversalTime().ToString('o'); Write-State $state
  $process.WaitForExit(); $process.Refresh()
  $state.exit_code=$process.ExitCode
  $resultPath=Join-Path $JobDir 'native-result.json'
  $ok=$false
  if(Test-Path -LiteralPath $resultPath){$result=Get-Content -Raw -LiteralPath $resultPath | ConvertFrom-Json; $ok=($result.status -eq 'ok' -and $result.job_id -eq $spec.job_id)}
  $state.state=$(if($state.exit_code -eq 0 -and $ok){'COMPLETED'}else{'FAILED'})
  Write-State $state
  $state | ConvertTo-Json -Depth 12
} finally {$lock.Dispose()}
