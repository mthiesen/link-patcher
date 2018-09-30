extern crate byteorder;
extern crate capstone;
extern crate common_failures;
#[macro_use]
extern crate failure;
extern crate itertools;
#[macro_use]
extern crate lazy_static;
#[cfg(test)]
extern crate tempfile;
extern crate yansi;

// -------------------------------------------------------------------------------------------------

pub mod exe_tools;
pub mod patch_gen;

// -------------------------------------------------------------------------------------------------

use common_failures::prelude::*;
use itertools::Itertools;
use std::{
    ffi::OsString,
    fmt,
    fs::{self, File, OpenOptions},
    io::{prelude::*, SeekFrom},
    path::{Path, PathBuf}
};

// -------------------------------------------------------------------------------------------------

// Similar to std::fs::copy() but fails if the to file already exists.
fn copy_file(from: impl AsRef<Path>, to: impl AsRef<Path>) -> Result<()> {
    if to.as_ref().exists() {
        bail!(
            "A file with the name \"{}\" already exists.",
            to.as_ref().display()
        );
    }

    fs::copy(from, to)?;

    Ok(())
}

// -------------------------------------------------------------------------------------------------

fn create_backup_file(file_name: impl AsRef<Path>) -> Result<PathBuf> {
    let backup_extension = {
        let mut backup_extension = OsString::from("backup");
        if let Some(original_extension) = file_name.as_ref().extension() {
            backup_extension.push(".");
            backup_extension.push(original_extension);
        }
        backup_extension
    };

    let backup_file_name = {
        let mut backup_file_name: PathBuf = file_name.as_ref().into();
        backup_file_name.set_extension(backup_extension);
        backup_file_name
    };

    copy_file(&file_name, &backup_file_name).with_context(|_| {
        format!(
            "Failed to create backup copy \"{}\" of file \"{}\".",
            backup_file_name.display(),
            file_name.as_ref().display()
        )
    })?;

    Ok(backup_file_name)
}

// -------------------------------------------------------------------------------------------------

#[cfg(test)]
mod test_create_backup_file {
    use super::*;
    use tempfile::TempDir;

    fn create_dummy_file(path: &Path) {
        let mut file = File::create(path).unwrap();
        file.write_all(b"Hello, World!").unwrap();
    }

    #[test]
    fn correct_backup_file_name() {
        let tempdir = TempDir::new().unwrap();

        let file_name = tempdir.path().join("test.exe");
        create_dummy_file(&file_name);
        let backup_file_name = create_backup_file(&file_name).unwrap();

        let expected_backup_file_name = tempdir.path().join("test.backup.exe");
        assert_eq!(expected_backup_file_name, backup_file_name);
        assert!(backup_file_name.is_file());

        let file_name = tempdir.path().join("test");
        create_dummy_file(&file_name);
        let backup_file_name = create_backup_file(&file_name).unwrap();

        let expected_backup_file_name = tempdir.path().join("test.backup");
        assert_eq!(expected_backup_file_name, backup_file_name);
        assert!(backup_file_name.is_file());
    }

    #[test]
    fn correct_backup_file_content() {
        let tempdir = TempDir::new().unwrap();

        let file_name = tempdir.path().join("test.exe");
        create_dummy_file(&file_name);
        let backup_file_name = create_backup_file(&file_name).unwrap();

        let mut file_content = String::new();
        File::open(backup_file_name)
            .unwrap()
            .read_to_string(&mut file_content)
            .unwrap();
        assert_eq!("Hello, World!", file_content);
    }

    #[test]
    fn fails_if_backup_file_exists() {
        let tempdir = TempDir::new().unwrap();

        let file_name = tempdir.path().join("test.exe");
        create_dummy_file(&file_name);
        let expected_backup_file_name = tempdir.path().join("test.backup.exe");
        create_dummy_file(&expected_backup_file_name);

        assert!(create_backup_file(&file_name).is_err());
    }
}

// -------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Eq, PartialEq)]
struct Patch {
    offset: u64,
    original_code: Vec<u8>,
    patched_code: Vec<u8>
}

impl Patch {
    fn apply(&self, mut stream: impl Read + Write + Seek) -> Result<()> {
        assert_eq!(self.original_code.len(), self.patched_code.len());

        stream.seek(SeekFrom::Start(self.offset))?;
        let mut buffer = vec![0u8; self.original_code.len()];
        stream.read_exact(&mut buffer)?;
        if buffer != self.original_code {
            bail!("Wrong data found at patch position.");
        }

        stream.seek(SeekFrom::Start(self.offset))?;
        stream.write_all(&self.patched_code)?;

        Ok(())
    }
}

impl fmt::Display for Patch {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fn fmt_u8_slice(slice: &[u8], f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "[")?;
            write!(
                f,
                "{}",
                slice.iter().map(|byte| format!("{:02X}", byte)).join(", ")
            )?;
            write!(f, "]")?;
            Ok(())
        }

        writeln!(f, "At offset {}", self.offset)?;
        write!(f, "replace ")?;
        fmt_u8_slice(&self.original_code, f)?;
        writeln!(f)?;
        write!(f, "with    ")?;
        fmt_u8_slice(&self.patched_code, f)?;
        writeln!(f)?;
        Ok(())
    }
}

// -------------------------------------------------------------------------------------------------

#[cfg(test)]
mod test_patch {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn apply_patch() {
        let mut data = vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
        let patch = Patch {
            offset: 4,
            original_code: vec![4, 5, 6, 7],
            patched_code: vec![10, 11, 12, 13]
        };

        patch.apply(Cursor::new(&mut data)).unwrap();

        assert_eq!(vec![0, 1, 2, 3, 10, 11, 12, 13, 8, 9], data);
    }

    #[test]
    fn apply_patch_fails_if_wrong_original_data() {
        let mut data = vec![0, 1, 2, 3, 4, 99, 6, 7, 8, 9];
        let patch = Patch {
            offset: 4,
            original_code: vec![4, 5, 6, 7],
            patched_code: vec![10, 11, 12, 13]
        };

        assert!(patch.apply(Cursor::new(&mut data)).is_err());
    }

    #[test]
    fn apply_patch_fails_if_stream_too_short() {
        let mut data = vec![0, 1, 2, 3, 4, 5];
        let patch = Patch {
            offset: 4,
            original_code: vec![4, 5, 6, 7],
            patched_code: vec![10, 11, 12, 13]
        };

        assert!(patch.apply(Cursor::new(&mut data)).is_err());
    }
}

// -------------------------------------------------------------------------------------------------

fn find_patch(mut reader: impl Read + Seek) -> Result<Patch> {
    let arch = exe_tools::determine_architecture(&mut reader)
        .context("Failed to determine exe architecture.")?;

    let code_section =
        exe_tools::find_code_section(&mut reader).context("Failed to find exe code section.")?;

    let code = {
        let mut buffer = vec![0u8; code_section.len];
        reader
            .seek(SeekFrom::Start(code_section.offset))
            .context("Failed to read exe code section.")?;
        reader
            .read_exact(&mut buffer[..])
            .context("Failed to read exe code section.")?;
        buffer
    };

    Ok(patch_gen::find_patch(arch, code_section.offset, &code[..])
        .context("Failed to generate patch.")?)
}

// -------------------------------------------------------------------------------------------------

pub fn run(
    input_file: impl AsRef<Path>,
    apply_patch: bool,
    confirm_apply_patch: impl FnOnce() -> Result<bool>
) -> Result<Option<PathBuf>> {
    let patch = {
        let file = File::open(&input_file)
            .with_context(|_| format!("Failed to open \"{}\".", input_file.as_ref().display()))?;
        find_patch(file)?
    };

    println!("Patch found:");
    println!("{}", patch);

    const WARNING_MESSAGES: &[&str] = &[
        "WARNING:",
        "You apply this patch at your own risk!",
        "The patched executable may exhibit unintended behavior!",
        "The author of this program accepts no responsibility for any damages!"
    ];

    for msg in WARNING_MESSAGES.iter() {
        println!("{}", yansi::Paint::red(msg));
    }

    if apply_patch && confirm_apply_patch()? {
        let backup_file_name = create_backup_file(&input_file)?;
        println!(
            "Created backup copy of input file: \"{}\"",
            backup_file_name.display()
        );

        let mut file = OpenOptions::new()
            .create_new(false)
            .read(true)
            .write(true)
            .open(&input_file)
            .with_context(|_| {
                format!(
                    "Failed to open \"{}\" for writing.",
                    input_file.as_ref().display()
                )
            })?;

        patch.apply(&mut file).with_context(|_| {
            format!(
                "Failed to apply patch to \"{}\".",
                input_file.as_ref().display()
            )
        })?;

        println!("Patch applied to \"{}\".", input_file.as_ref().display());

        return Ok(Some(backup_file_name));
    }

    Ok(None)
}
