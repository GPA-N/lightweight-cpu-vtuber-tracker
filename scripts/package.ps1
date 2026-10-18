# Gathers everything live.exe needs into dist/vtuber-tracker/.
# Idempotent: wipes and recreates only dist/vtuber-tracker/. Never touches anything else.
# Usage: powershell -NoProfile -ExecutionPolicy Bypass -File scripts/package.ps1

$ErrorActionPreference = 'Stop'

$root   = Split-Path -Parent $PSScriptRoot
$distRoot = Join-Path $root 'dist'
$bundle = Join-Path $distRoot 'vtuber-tracker'

# Guard: the only directory this script deletes must be dist/vtuber-tracker under the repo.
if ((Split-Path -Leaf $bundle) -ne 'vtuber-tracker' -or
    (Split-Path -Leaf $distRoot) -ne 'dist' -or
    -not $bundle.StartsWith($distRoot + [IO.Path]::DirectorySeparatorChar)) {
    throw "Refusing to clean unexpected path: $bundle"
}

$exeSrc = Join-Path $root 'target\release\live.exe'
if (-not (Test-Path $exeSrc)) { throw "Missing $exeSrc. Build it yourself with cargo (this script does not build)." }

# Staleness check: warn if live.exe is older than the newest Rust source under crates/.
$exeTime = (Get-Item $exeSrc).LastWriteTime
$newestSrc = Get-ChildItem (Join-Path $root 'crates') -Recurse -Filter *.rs | Sort-Object LastWriteTime -Descending | Select-Object -First 1
if ($newestSrc -and $newestSrc.LastWriteTime -gt $exeTime) {
    Write-Warning ("live.exe ({0}) is older than {1} ({2}). Rebuild before shipping." -f $exeTime, $newestSrc.FullName, $newestSrc.LastWriteTime)
}

# Models: only the 3 shipped files, verified against models/SHA256SUMS.
$modelNames = @('face_detector.onnx', 'face_landmarks.onnx', 'face_blendshapes.onnx')
$sumsPath = Join-Path $root 'models\SHA256SUMS'
if (-not (Test-Path $sumsPath)) { throw "Missing $sumsPath" }
$sumLines = Get-Content $sumsPath | Where-Object { $_.Trim() -ne '' }
$shipLines = @()
foreach ($name in $modelNames) {
    $line = $sumLines | Where-Object { $_ -match ("\s" + [regex]::Escape($name) + "$") } | Select-Object -First 1
    if (-not $line) { throw "No SHA256SUMS entry for $name" }
    $expected = ($line -split '\s+')[0].ToLower()
    $src = Join-Path $root ("models\" + $name)
    if (-not (Test-Path $src)) { throw "Missing model $src" }
    $actual = (Get-FileHash $src -Algorithm SHA256).Hash.ToLower()
    if ($actual -ne $expected) { throw "Hash mismatch for ${name}: expected $expected, got $actual" }
    $shipLines += $line
}

# Clean and recreate the bundle directory only.
if (Test-Path $bundle) { Remove-Item -LiteralPath $bundle -Recurse -Force }
New-Item -ItemType Directory -Force -Path $bundle, (Join-Path $bundle 'models'), (Join-Path $bundle 'data'), (Join-Path $bundle 'out') | Out-Null

Copy-Item $exeSrc (Join-Path $bundle 'live.exe')

foreach ($name in $modelNames) {
    Copy-Item (Join-Path $root ("models\" + $name)) (Join-Path $bundle ("models\" + $name))
}
# Filtered checksums: only the shipped files (SHA256SUMS in the repo also lists the private fallback model).
Set-Content -LiteralPath (Join-Path $bundle 'models\SHA256SUMS') -Value $shipLines -Encoding ascii

Copy-Item (Join-Path $root 'data\blendshape_map.json') (Join-Path $bundle 'data\blendshape_map.json')

# Upper-body models (live.exe --body), verified against models/SHA256SUMS the same way.
$bodyNames = @('pose_detector.onnx', 'pose_landmarks.onnx', 'palm_detection_mediapipe_2023feb.onnx',
               'handpose_estimation_mediapipe_2023feb.onnx', 'cigpose-m_coco_256x192.onnx')
New-Item -ItemType Directory -Force -Path (Join-Path $bundle 'models\upper_body') | Out-Null
foreach ($name in $bodyNames) {
    $line = $sumLines | Where-Object { $_ -match ("\supper_body/" + [regex]::Escape($name) + "$") } | Select-Object -First 1
    if (-not $line) { throw "No SHA256SUMS entry for upper_body/$name" }
    $src = Join-Path $root ("models\upper_body\" + $name)
    if (-not (Test-Path $src)) { throw "Missing model $src" }
    if ((Get-FileHash $src -Algorithm SHA256).Hash.ToLower() -ne ($line -split '\s+')[0].ToLower()) { throw "Hash mismatch for $name" }
    Copy-Item $src (Join-Path $bundle ("models\upper_body\" + $name))
    $shipLines += $line
}
Set-Content -LiteralPath (Join-Path $bundle 'models\SHA256SUMS') -Value $shipLines -Encoding ascii

# OpenVINO CPU runtime (task 11.2): CPU plugin and the ONNX reader only, about 70 MB, found next to live.exe.
# Source: an OpenVINO libs folder (default: the project's private Python venv). Skipped with a warning when
# missing; the app then falls back to ONNX Runtime.
$ovLibs = if ($env:OPENVINO_LIBS) { $env:OPENVINO_LIBS } else { Join-Path $root 'fixtures\private\venv_rt\Lib\site-packages\openvino\libs' }
$ovDlls = @('openvino.dll', 'openvino_c.dll', 'openvino_intel_cpu_plugin.dll', 'openvino_onnx_frontend.dll', 'tbb12.dll', 'tbbmalloc.dll')
if (Test-Path $ovLibs) {
    foreach ($dll in $ovDlls) {
        $p = Join-Path $ovLibs $dll
        if (-not (Test-Path $p)) { throw "Missing OpenVINO file $p" }
        Copy-Item $p (Join-Path $bundle $dll)
    }
} else {
    Write-Warning "OpenVINO libs not found at $ovLibs; bundle runs on ONNX Runtime only."
}

# Samples: copy the whole samples/ tree (Seed-san.vrm and any sample folders).
$samplesSrc = Join-Path $root 'samples'
if (Test-Path $samplesSrc) {
    Copy-Item -LiteralPath $samplesSrc -Destination $bundle -Recurse -Force
} else {
    Write-Warning 'samples/ not found; bundle has no avatar.'
}

# run.bat: cd to its own folder so relative paths work from any cwd.
$runBat = @'
@echo off
setlocal
cd /d "%~dp0"
"%~dp0live.exe" --vrm "samples\Seed-san.vrm" --map "data\blendshape_map.json" --models "models" --out "out\live" %*
endlocal
'@
Set-Content -LiteralPath (Join-Path $bundle 'run.bat') -Value ($runBat -replace "`r?`n", "`r`n") -Encoding ascii

$readme = @'
VTuber Tracker - live face tracking into a VRM avatar (Windows)

RUN
  run.bat                      start with defaults (Seed-san avatar, camera 0, 30 s)
  run.bat --seconds 4          extra flags are passed through

FLAGS (live.exe)
  --vrm PATH        VRM 1.0 avatar           default samples\Seed-san.vrm
  --map PATH        blendshape map JSON      default data\blendshape_map.json
  --models DIR      ONNX models folder       default models
  --camera INDEX    webcam index             default 0
  --threads N       CPU threads for ORT      default: heuristic
  --seconds N       run length in seconds    default 30
  --out DIR         report output folder     default out\live
  --no-mirror       do not mirror the image
  --body            also track arms, hands and fingers (models\upper_body)
  --limb NAME       blaze (default, has 3D depth) or cigpose (2D only), the arm model used with --body

Output: out\live\live_report.txt and out\live\live_stats.csv.

CONTENTS
  live.exe            tracker and avatar window (ONNX Runtime is linked in; no DLL needed)
  *.dll               OpenVINO CPU runtime (optional; used for the CIGPose arm model)
  models\             face models, models\upper_body\ (body, hand and CIGPose-m models), SHA256SUMS
  data\               blendshape_map.json
  samples\            VRM avatar(s)
  out\                created on first run

REQUIREMENTS
  Windows 10/11 with a webcam (Media Foundation) and a GPU driver for DirectX 12.
  Runs on CPU for inference.

LICENCES
  live.exe and the ONNX models are project/third-party builds; see the repo docs.
  samples\Seed-san.vrm is licensed under VRM Public License 1.0 and is marked in the
  repo as private spike use only. Do not redistribute this bundle with that file
  unless the licence permits it.
  samples\ folders other than Seed-san carry their own licence files; check each one.
'@
Set-Content -LiteralPath (Join-Path $bundle 'README.txt') -Value ($readme -replace "`r?`n", "`r`n") -Encoding ascii

$sizeMB = [math]::Round(((Get-ChildItem $bundle -Recurse -File | Measure-Object Length -Sum).Sum) / 1MB, 1)
Write-Output "Bundle: $bundle"
Write-Output "Size: $sizeMB MB"
