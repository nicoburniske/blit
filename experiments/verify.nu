let root = ($env.FILE_PWD | path dirname)
cd $root
let result = (nix develop --command rustc experiments/borrowed_commands.rs -o /tmp/blit-borrowed-commands | complete)
if $result.exit_code == 0 { error make {msg: "borrowed commands unexpectedly survived an arena reset"} }
if not ($result.stderr | str contains "E0502") { error make {msg: $result.stderr} }
print "borrowed references prevent resetting the retained frame arena as expected"
nix develop --command cargo run -p blit-gui --release --example extension_storage
