let root = ($env.FILE_PWD | path dirname)
cd $root
let path = "gui/cpu/src/extension.rs"
let original = (open --raw $path)
let module = '\n#[cfg(test)]\n#[path = "../../../experiments/shadow_benchmark.rs"]\nmod benchmark;\n' | str replace --all '\n' (char newline)
([$original $module] | str join) | save --force $path
let result = (nix develop --command cargo test --release -p blit-cpu --lib shadow_extension_cost -- --ignored --nocapture | complete)
$original | save --force $path
(($result.stdout | str trim --right) + (char newline)) | save --force "experiments/shadow_benchmark.txt"
print $result.stdout
print $result.stderr
if $result.exit_code != 0 { error make {msg: "shadow benchmark failed"} }
