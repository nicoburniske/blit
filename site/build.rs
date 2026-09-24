use std::{env, fs, io::Write, path::PathBuf};

use serde::Deserialize;

fn main() {
    println!("cargo:rerun-if-changed=timelapse.toml");
    let history: History =
        toml::from_str(&fs::read_to_string("timelapse.toml").expect("read timelapse.toml"))
            .expect("valid timelapse.toml");
    assert!(!history.revision.is_empty(), "timelapse needs a revision");
    let path = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("timelapse.rs");
    let mut output = std::io::BufWriter::new(fs::File::create(path).unwrap());
    writeln!(output, "[").unwrap();
    for revision in history.revision {
        writeln!(
            output,
            "Revision {{ commit: {:?}, date: {:?}, title: {:?}, source: {:?}, note: {:?}, code: {:?}, ..Default::default() }},",
            revision.commit,
            revision.date,
            revision.title,
            revision.source,
            revision.note,
            revision.code.trim_end(),
        )
        .unwrap();
    }
    writeln!(output, "]").unwrap();
    output.flush().unwrap();
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct History {
    revision: Vec<Revision>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Revision {
    commit: String,
    date: String,
    title: String,
    source: String,
    #[serde(default)]
    note: String,
    code: String,
}
