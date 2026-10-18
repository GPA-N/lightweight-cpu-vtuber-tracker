<#
.SYNOPSIS
    Downloads the pinned face models into models/ and verifies SHA-256 against models/SHA256SUMS.

.DESCRIPTION
    Files that already match their SHA-256 are skipped. A file that exists but does not match is
    deleted and reported as a failure (re-run to fetch a fresh copy). A download that fails or
    whose hash does not match is reported as a failure and nothing is written to models/.
    Exit code is 0 only when every selected file is present and verified.

.PARAMETER IncludeFallback
    Opt-in. Also fetches the fallback model (karie666666/face-au-arkit-blendshapes): model.onnx and
    model.onnx.data. Private evaluation only, not for shipping.
#>
[CmdletBinding()]
param(
    [switch]$IncludeFallback
)

$ErrorActionPreference = 'Stop'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$root      = Split-Path -Parent $PSScriptRoot
$modelsDir = Join-Path $root 'models'
$sumsPath  = Join-Path $modelsDir 'SHA256SUMS'

if (-not (Test-Path -LiteralPath $modelsDir)) {
    New-Item -ItemType Directory -Path $modelsDir | Out-Null
}
if (-not (Test-Path -LiteralPath $sumsPath)) {
    Write-Host "ERROR: $sumsPath not found"
    exit 1
}

# Expected hashes, sha256sum format: "<sha256>  <name>"
$expected = @{}
foreach ($line in Get-Content -LiteralPath $sumsPath) {
    if ($line -match '^([0-9a-fA-F]{64})  (.+)$') {
        $expected[$Matches[2]] = $Matches[1].ToLowerInvariant()
    }
}

# Pinned sources. Revisions are commit hashes, not branch names.
$models = @(
    @{ Name = 'face_detector.onnx';
       Url  = 'https://huggingface.co/fernandotonon/QtMeshEditor-models/resolve/d14119a40dfeef208e4e724dfaceb2640d2df95b/mocap/face/face_detector.onnx';
       Fallback = $false },
    @{ Name = 'face_landmarks.onnx';
       Url  = 'https://huggingface.co/senty-au/face_landmarks_detector-ONNX/resolve/337d58218b5b1cc597ca3c67360880b920f6ce7b/onnx/model.onnx';
       Fallback = $false },
    @{ Name = 'face_blendshapes.onnx';
       Url  = 'https://huggingface.co/fernandotonon/QtMeshEditor-faceblendshapes-onnx/resolve/495864b0bae88b0b926b8da8cb85f9022ceaa402/face_blendshapes.onnx';
       Fallback = $false },
    @{ Name = 'model.onnx';
       Url  = 'https://huggingface.co/karie666666/face-au-arkit-blendshapes/resolve/7696eb728a3559538b7930b1d44fa0db1a5baec8/model.onnx';
       Fallback = $true },
    @{ Name = 'model.onnx.data';
       Url  = 'https://huggingface.co/karie666666/face-au-arkit-blendshapes/resolve/7696eb728a3559538b7930b1d44fa0db1a5baec8/model.onnx.data';
       Fallback = $true }
)

$failures = 0
foreach ($m in $models) {
    if ($m.Fallback -and -not $IncludeFallback) { continue }

    $name = $m.Name
    if (-not $expected.ContainsKey($name)) {
        Write-Host "ERROR: no hash for $name in SHA256SUMS"
        $failures++
        continue
    }
    $want = $expected[$name]
    $dest = Join-Path $modelsDir $name

    if (Test-Path -LiteralPath $dest) {
        $got = (Get-FileHash -LiteralPath $dest -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($got -eq $want) {
            Write-Host "SKIP  $name (already verified)"
            continue
        }
        Remove-Item -LiteralPath $dest -Force
        Write-Host "MISMATCH ${name}: deleted (expected $want, got $got). Re-run to fetch a fresh copy."
        $failures++
        continue
    }

    $tmp = "$dest.part"
    try {
        Invoke-WebRequest -Uri $m.Url -OutFile $tmp -UseBasicParsing
    } catch {
        Remove-Item -LiteralPath $tmp -Force -ErrorAction SilentlyContinue
        Write-Host "ERROR download $name failed: $($_.Exception.Message)"
        $failures++
        continue
    }

    $got = (Get-FileHash -LiteralPath $tmp -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($got -ne $want) {
        Remove-Item -LiteralPath $tmp -Force
        Write-Host "MISMATCH downloaded $name (expected $want, got $got): discarded"
        $failures++
        continue
    }

    Move-Item -LiteralPath $tmp -Destination $dest
    Write-Host "OK    $name (downloaded, verified)"
}

if ($failures -gt 0) {
    Write-Host "FAILED: $failures problem(s)"
    exit 1
}
Write-Host "All selected models verified."
exit 0
