extern crate walkdir;

// -------------------------------------------------------------------------------------------------

use std::{
    env,
    fs::File,
    io::Write,
    path::{Path, PathBuf},
};

// -------------------------------------------------------------------------------------------------

#[derive(Debug)]
struct LinkExecutableInfo {
    path: PathBuf,
    test_name_suffix: String,
}

// -------------------------------------------------------------------------------------------------

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let file_name = Path::new(&out_dir).join("generated_tests.rs");
    let mut file = File::create(&file_name).unwrap();

    let link_executables = walkdir::WalkDir::new("tests/link_executables")
        .into_iter()
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let file_name = entry.path().file_name()?.to_str()?;
            if file_name.eq_ignore_ascii_case("link.exe") && entry.path().is_file() {
                Some(entry.path().to_owned())
            } else {
                None
            }
        })
        .map(|path| {
            let test_name_suffix: String = {
                let exe_directory = path.iter().rev().nth(1).unwrap();
                exe_directory
                    .to_string_lossy()
                    .chars()
                    .map(|c| if c.is_alphanumeric() { c } else { '_' })
                    .flat_map(|c| c.to_lowercase())
                    .collect()
            };

            LinkExecutableInfo {
                path,
                test_name_suffix,
            }
        });

    for link_executable in link_executables {
        writeln!(
            file,
            "#[test] fn patched_{}() {{ test_patched_link(r\"{}\"); }}",
            link_executable.test_name_suffix,
            link_executable.path.display()
        )
        .unwrap();
    }
}
