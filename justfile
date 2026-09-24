set shell := ["nu", "-c"]

site port="8766":
    nu site/build.nu
    @print "Open http://localhost:{{port}}/#evolution"
    nix shell nixpkgs#miniserve -c miniserve site/dist --index index.html --interfaces 127.0.0.1 --port {{quote(port)}}
