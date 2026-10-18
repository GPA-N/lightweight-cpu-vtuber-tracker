# Poses an avatar from one photo and saves screenshot, landmark overlay and JSON dump.
# Still mode only: APP_STILL_BODY is always set, so the camera path is never reached.
# The process is killed after a timeout if it has not exited on its own.
# usage: scripts\photo_to_avatar.ps1 -Model seed|twist -Photo hands_19 [-Exe target\debug\app.exe] [-Vis 0.5]
param(
    [Parameter(Mandatory = $true)][ValidateSet('seed', 'twist')][string]$Model,
    [Parameter(Mandatory = $true)][string]$Photo,
    [string]$Exe = 'target\debug\app.exe',
    [string]$Vis = '',
    [int]$TimeoutSeconds = 150
)
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$photoPath = "fixtures\private\upper_body_photos\$Photo.png"
if (-not (Test-Path $photoPath)) { throw "missing photo $photoPath" }
$out = Join-Path $root 'fixtures\private\out\photo_to_avatar'
New-Item -ItemType Directory -Force $out | Out-Null
$vrm = if ($Model -eq 'seed') { 'samples\Seed-san.vrm' } else { 'samples\VRM1_Constraint_Twist_Sample\VRM1_Constraint_Twist_Sample.vrm' }
$env:APP_STILL_BODY = $photoPath
$env:AVATAR_SHOT = "$out\${Model}_$Photo.png"
$env:APP_BODY_OVERLAY = "$out\${Model}_${Photo}_lm.png"
$env:APP_BODY_DUMP = "$out\${Model}_$Photo.json"
if ($Vis -ne '') { $env:APP_BODY_VIS = $Vis } else { Remove-Item Env:APP_BODY_VIS -ErrorAction SilentlyContinue }
if (-not $env:APP_STILL_BODY) { throw 'APP_STILL_BODY not set; refusing to start the live camera path' }
$log = "$out\${Model}_$Photo.txt"
$p = Start-Process -FilePath $Exe -ArgumentList '--vrm', $vrm -PassThru -NoNewWindow `
    -RedirectStandardOutput $log -RedirectStandardError "$out\${Model}_$Photo.err"
if (-not $p.WaitForExit($TimeoutSeconds * 1000)) {
    Stop-Process -Id $p.Id -Force
    Write-Output "TIMEOUT: killed pid $($p.Id)"
}
Get-Content $log | Where-Object { $_ -match '^(still body|pose |  |LIMBS|RENDERED)' }

