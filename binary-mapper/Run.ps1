$ELDENRING_PATH = $Env:ELDENRING_PATH ?? "G:\SteamLibrary\steamapps\common\ELDEN RING\Game\"

cargo run --manifest-path fromsoftware-rs/Cargo.toml -p binary-mapper -- `
    map --profile binary-mapper/profile.toml --exe "$ELDENRING_PATH/eldenring.exe" > src/rva/ww.rs

cargo run --manifest-path fromsoftware-rs/Cargo.toml -p binary-mapper -- `
    map --profile binary-mapper/profile.toml --exe "$ELDENRING_PATH/eldenring.jp.exe" > src/rva/jp.rs
