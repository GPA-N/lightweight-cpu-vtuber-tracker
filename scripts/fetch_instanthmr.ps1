# Installs the InstantHMR body model for `live --body`.
#
# Downloads the two files the tracker reads from huggingface.co/momolesang/InstantHMR (v1.1) into
# models\upper_body and checks their SHA-256:
#   instanthmr-nano.onnx  the network (80 MB)
#   mhr-body.onnx         the MHR body model that turns the network's parameters into 3D keypoints (13 MB)
# The person detector (models\upper_body\rfdetr_nano.onnx) is not downloaded here: export it with the
# rfdetr Python package (see the README, "Build and test").
#
# The weights are under the SAM license; the nano model is for non-commercial use (see the model page).
# The release zip ships them with licenses\SAM-LICENSE.txt; this script is for developers.
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$base = 'https://huggingface.co/momolesang/InstantHMR/resolve/main'
$files = @{
    'instanthmr-nano.onnx' = 'A9E78FB89649D59BC8DF294C6D23CD460168003B6419386BB053D353C3F85A69'
    'mhr-body.onnx'        = 'ECA2224D724F91B9E4F4BD7C86F6EAFC74AB59FF0494BFB462E375CD04DAC0FB'
}
$outDir = Join-Path $root 'models\upper_body'
New-Item -ItemType Directory -Force $outDir | Out-Null
foreach ($name in $files.Keys) {
    $out = Join-Path $outDir $name
    curl.exe -L --fail -s -o $out "$base/$name"
    if ((Get-FileHash $out -Algorithm SHA256).Hash -ne $files[$name]) {
        Remove-Item $out
        throw "$name does not match the expected SHA-256"
    }
    "installed $out ($([int]((Get-Item $out).Length / 1MB)) MB)"
}
