# Installs the InstantHMR body model for `live --body`.
#
# Downloads instanthmr.onnx from huggingface.co/momolesang/InstantHMR (77 MB, fp16 weights), checks its
# SHA-256, and cuts it down to the two joint outputs the tracker reads (joints_2d, joints_3d), which
# writes models\upper_body\instanthmr_joints.onnx. Needs Python with the `onnx` package (pip install onnx).
#
# The weights are under the SAM license (they distil facebook/sam-3d-body-dinov3). They are not part of
# this repository and are not redistributed here: read the license on the model page before you use them.
# The download goes into its own folder under the temp directory and is parsed as data only.
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$url = 'https://huggingface.co/momolesang/InstantHMR/resolve/main/instanthmr.onnx'
$sha256 = 'F717558094F57D7C9CD084D7981874DBCC3F08E7308501EB98C12844E2CCE3B7'
$work = Join-Path ([IO.Path]::GetTempPath()) 'instanthmr-download'
New-Item -ItemType Directory -Force $work | Out-Null
$full = Join-Path $work 'instanthmr.onnx'
curl.exe -L --fail -s -o $full $url
if ((Get-FileHash $full -Algorithm SHA256).Hash -ne $sha256) { throw 'instanthmr.onnx does not match the expected SHA-256' }
$out = Join-Path $root 'models\upper_body\instanthmr_joints.onnx'
$py = "import sys, onnx.utils; onnx.utils.extract_model(sys.argv[1], sys.argv[2], ['image', 'cliff_cond'], ['joints_2d', 'joints_3d'])"
python -c $py $full $out
Remove-Item -Recurse -Force $work
"installed $out ($([int]((Get-Item $out).Length / 1MB)) MB)"