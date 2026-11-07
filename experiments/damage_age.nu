let root = ($env.FILE_PWD | path dirname)
cd $root
let path = "gui/cpu/src/lib.rs"
let original = (open --raw $path)
let marker = "        let screen = self.screen();"
if not ($original | str contains $marker) { error make {msg: "renderer changed"} }
let extra = '        if !self.pass_damage.passes.is_empty() && !self.damage.is_empty() { self.damage.update(input.display_list, self.screen(), input.scale.x, 0); }'
let broken = ($original | str replace $marker ([$extra $marker] | str join (char newline)))
$broken | save --force $path
let result = (nix develop --command cargo test -p blit-cpu --example extensions pass_barriers_read_current_frame_and_respect_nested_clips | complete)
$original | save --force $path
if $result.exit_code == 0 { error make {msg: "double damage updates unexpectedly passed the buffer age regression"} }
if not ($result.stdout | str contains "assertion") { error make {msg: $result.stderr} }
print "double damage updates fail the alternating buffer regression as expected"
print $result.stdout
