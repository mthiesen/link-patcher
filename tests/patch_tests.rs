extern crate itertools;
extern crate link_patcher;
extern crate tempfile;
extern crate walkdir;
extern crate winreg;

// -------------------------------------------------------------------------------------------------

use itertools::Itertools;
use link_patcher::exe_tools;
use std::{
    ffi::{OsStr, OsString},
    fs::{self, File},
    path::{Path, PathBuf},
    process::Command
};
use tempfile::TempDir;
use walkdir::WalkDir;

// -------------------------------------------------------------------------------------------------

fn windows_sdk_dirs() -> impl Iterator<Item = PathBuf> {
    use winreg::{enums::*, RegKey, HKEY};

    const PATHS: [&str; 2] = [
        r"Software\Microsoft\Microsoft SDKs\Windows",
        r"Software\Wow6432Node\Microsoft\Microsoft SDKs\Windows"
    ];

    const PREDEF_KEYS: [HKEY; 2] = [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE];

    let open_keys = PATHS
        .iter()
        .cartesian_product(PREDEF_KEYS.iter())
        .filter_map(|(path, predef_key)| RegKey::predef(*predef_key).open_subkey(path).ok());

    let current_install_folders = open_keys
        .clone()
        .filter_map(|key| key.get_value::<String, _>("CurrentInstallFolder").ok());

    let installation_folders = open_keys.flat_map(|key| {
        key.enum_keys()
            .filter_map(|sub_key| {
                let sub_key = key.open_subkey(sub_key.ok()?).ok()?;
                sub_key.get_value::<String, _>("InstallationFolder").ok()
            }).collect::<Vec<_>>()
    });

    current_install_folders
        .chain(installation_folders)
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
        .unique()
}

// -------------------------------------------------------------------------------------------------

#[derive(Debug)]
struct Kernel32Libs {
    x86_lib: PathBuf,
    x64_lib: PathBuf
}

// -------------------------------------------------------------------------------------------------

fn installed_kernel32_libs() -> Option<Kernel32Libs> {
    let kernel32_libs = windows_sdk_dirs()
        .flat_map(walkdir::WalkDir::new)
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let file_name = entry.path().file_name()?.to_str()?;
            if file_name.eq_ignore_ascii_case("kernel32.lib") && entry.path().is_file() {
                Some(entry.path().to_owned())
            } else {
                None
            }
        });

    let mut x86_lib = None;
    let mut x64_lib = None;

    for lib in kernel32_libs {
        if let Some(parent_dir) = lib.iter().rev().nth(1).and_then(|d| d.to_str()) {
            if parent_dir.eq_ignore_ascii_case("lib") || parent_dir.eq_ignore_ascii_case("x86") {
                x86_lib = x86_lib.or_else(|| Some(lib.clone()));
            } else if parent_dir.eq_ignore_ascii_case("x64") {
                x64_lib = x64_lib.or_else(|| Some(lib.clone()));
            }
        }

        if x86_lib.is_some() && x64_lib.is_some() {
            break;
        }
    }

    if x86_lib.is_some() && x64_lib.is_some() {
        Some(Kernel32Libs {
            x86_lib: x86_lib.unwrap(),
            x64_lib: x64_lib.unwrap()
        })
    } else {
        None
    }
}

// -------------------------------------------------------------------------------------------------

#[derive(Debug)]
struct TestFiles {
    tempdir: TempDir,
    x86_exe: PathBuf,
    x64_exe: PathBuf
}

// -------------------------------------------------------------------------------------------------

fn link_file(
    linker_path: impl AsRef<OsStr>,
    out_path: impl AsRef<OsStr>,
    input_args: &[impl AsRef<OsStr>]
) {
    let mut out_arg = OsString::from("/OUT:");
    out_arg.push(&out_path);
    let status = Command::new(linker_path)
        .arg(out_arg)
        .arg("/NOLOGO")
        .arg("/ENTRY:main")
        .args(input_args)
        .status()
        .unwrap();
    assert!(status.success());

    let status = Command::new(out_path).status().unwrap();
    assert_eq!(1337, status.code().unwrap());
}

// -------------------------------------------------------------------------------------------------

fn link_test_files(linker_path: impl AsRef<OsStr>) -> TestFiles {
    let tempdir = TempDir::new().unwrap();

    let kernel32_libs = installed_kernel32_libs().unwrap();

    let x86_exe = tempdir.path().join("test_x86.exe");
    link_file(
        &linker_path,
        &x86_exe,
        &[
            OsStr::new("/MACHINE:X86"),
            OsStr::new(r"tests\objs\x86\add.o"),
            OsStr::new(r"tests\objs\x86\main.o"),
            kernel32_libs.x86_lib.as_os_str()
        ]
    );

    let x64_exe = tempdir.path().join("test_x64.exe");
    link_file(
        &linker_path,
        &x64_exe,
        &[
            OsStr::new("/MACHINE:X64"),
            OsStr::new(r"tests\objs\x64\add.o"),
            OsStr::new(r"tests\objs\x64\main.o"),
            kernel32_libs.x64_lib.as_os_str()
        ]
    );

    TestFiles {
        tempdir,
        x86_exe,
        x64_exe
    }
}

// -------------------------------------------------------------------------------------------------

fn has_rich_header(exe_path: impl AsRef<Path>) -> bool {
    let file = File::open(exe_path).unwrap();
    if let Ok(Some(_)) = exe_tools::read_rich_header(file) {
        true
    } else {
        false
    }
}

// -------------------------------------------------------------------------------------------------

fn copy_dir(src: impl AsRef<Path>, dest: impl AsRef<Path>) {
    for entry in WalkDir::new(src).min_depth(1).max_depth(1) {
        let entry = entry.unwrap();

        let src_path = entry.path();
        let dest_path = dest.as_ref().join(src_path.file_name().unwrap());
        fs::copy(src_path, dest_path).unwrap();
    }
}

// -------------------------------------------------------------------------------------------------

fn test_patched_link(linker_path: impl AsRef<OsStr>) {
    let patched_dir = TempDir::new().unwrap();
    copy_dir(
        Path::new(&linker_path).parent().unwrap(),
        patched_dir.path()
    );

    let linker_file_name = Path::new(&linker_path).file_name().unwrap();
    let patched_linker_path = patched_dir.path().join(linker_file_name);

    let backup_file_name = link_patcher::run(&patched_linker_path, true, || Ok(true))
        .unwrap()
        .unwrap();

    let patched_test_files = link_test_files(patched_linker_path);
    assert!(!has_rich_header(patched_test_files.x86_exe));
    assert!(!has_rich_header(patched_test_files.x64_exe));

    let unpatched_test_files = link_test_files(backup_file_name);
    assert!(has_rich_header(unpatched_test_files.x86_exe));
    assert!(has_rich_header(unpatched_test_files.x64_exe));
}

include!(concat!(env!("OUT_DIR"), "/generated_tests.rs"));
