cargo build

$Target = "target\x86_64-pc-windows-msvc\debug"

New-Item -Path $Target\.hotpatch -ItemType Directory -Force
Copy-Item -Path $Target\erfps2.dll -Destination $Target\.hotpatch\erfps2.dll -Force
Copy-Item -Path dist\erfps2.toml -Destination $Target\.hotpatch\erfps2.toml -Force

$Env:RUST_LOG = "trace"
Start-Process -FilePath "me3.exe" -ArgumentList "launch -g er --disable-arxan --savefile DEV.sl2 --native $Target\.hotpatch\erfps2.dll"

Get-Content "G:\SteamLibrary\steamapps\common\ELDEN RING\Game\erfps2.log" -Wait -Tail 1
