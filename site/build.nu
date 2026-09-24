#!/usr/bin/env nu

def main [] {
    let site = $env.FILE_PWD
    let target = $site | path dirname | path join target
    cd ($site | path dirname)
    with-env {
        CARGO_ENCODED_RUSTFLAGS: ""
        CARGO_PROFILE_RELEASE_OPT_LEVEL: "s"
        CARGO_PROFILE_RELEASE_LTO: "true"
        CARGO_PROFILE_RELEASE_CODEGEN_UNITS: "1"
        CARGO_PROFILE_RELEASE_PANIC: "abort"
        CARGO_PROFILE_RELEASE_STRIP: "symbols"
    } {
        cargo -Z build-std=std,panic_abort build --locked --release -p blit-site --target wasm32-unknown-unknown --target-dir $target
        if $env.LAST_EXIT_CODE != 0 { error make { msg: "site build failed" } }
    }
    let output = $site | path join dist
    mkdir $output
    for asset in [index.html main.js icon.svg timelapse.toml] {
        cp ($site | path join $asset) ($output | path join $asset)
    }
    cp web/host.js ($output | path join host.js)
    cp ($target | path join wasm32-unknown-unknown release blit_site.wasm) ($output | path join blit_site.wasm)
    print $"built ($output)"
}
