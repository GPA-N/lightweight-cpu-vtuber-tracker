# Downloads the Seed-san sample avatar (VRM 1.0) into samples\Seed-san.vrm and verifies its SHA-256.
# License: VRM Public License 1.0 (see docs/notes/seed-san.md). Private spike use only; do not commit or redistribute.
$ErrorActionPreference = 'Stop'

$url      = 'https://raw.githubusercontent.com/vrm-c/vrm-specification/master/samples/Seed-san/vrm/Seed-san.vrm'
$expected = '624D0D554BC205BBDC33E22A68A2C3C20EDEBB3E573011EAD8878A65E5329B23'
$root     = Split-Path -Parent $PSScriptRoot
$dir      = Join-Path $root 'samples'
$dest     = Join-Path $dir 'Seed-san.vrm'

New-Item -ItemType Directory -Force -Path $dir | Out-Null

if (Test-Path $dest) {
    $have = (Get-FileHash -Algorithm SHA256 $dest).Hash
    if ($have -eq $expected) { Write-Host "SKIP  Seed-san.vrm (already verified)"; exit 0 }
    Remove-Item $dest
}

Invoke-WebRequest -Uri $url -OutFile $dest
$have = (Get-FileHash -Algorithm SHA256 $dest).Hash
if ($have -ne $expected) {
    Remove-Item $dest
    Write-Host "MISMATCH  Seed-san.vrm (got $have, expected $expected); file deleted"
    exit 1
}
Write-Host "OK    Seed-san.vrm verified ($((Get-Item $dest).Length) bytes)"
