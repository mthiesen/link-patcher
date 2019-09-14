use common_failures::quick_main;
use failure::Fallible;
use failure::ResultExt;
use std::fs::File;
use std::io::prelude::*;
use std::io::BufWriter;
use std::path::Path;
use std::path::PathBuf;

#[derive(Debug)]
struct PatchInfo {
    product_name: String,
    product_version: String,
    architecture: linker_utils::Architecture,
    crc32: u32,
    patch: link_patcher::Patch,
}

impl PatchInfo {
    fn to_strings(&self) -> Vec<String> {
        fn bytes_to_string(bytes: &[u8]) -> String {
            use std::fmt::Write;
            let mut string = String::with_capacity(bytes.len() * 4);
            for (index, byte) in bytes.iter().enumerate() {
                write!(&mut string, "{:02X}", byte).expect("writing to a string cannot fail");
                let is_last = index == bytes.len() - 1;
                if !is_last {
                    write!(&mut string, ", ").expect("writing to a string cannot fail");
                }
            }
            string
        }

        let mut strings = Vec::with_capacity(7);
        strings.push(self.product_name.trim_start_matches("Microsoft® ").to_owned());
        strings.push(self.product_version.clone());
        strings.push(match self.architecture {
            linker_utils::Architecture::X86 => "x86".to_owned(),
            linker_utils::Architecture::X64 => "x64".to_owned(),
        });
        strings.push(format!("{:08X}", self.crc32));
        strings.push(format!("{}", self.patch.offset));
        strings.push(bytes_to_string(&self.patch.original_code));
        strings.push(bytes_to_string(&self.patch.patched_code));
        strings
    }
}

fn generate_patch_info(path: impl AsRef<Path>) -> Fallible<PatchInfo> {
    let version_info =
        linker_utils::get_version_info(path.as_ref()).context("failed to retrieve version info")?;
    let architecture = linker_utils::get_architecture(path.as_ref())
        .context("failed to determine linker architecture")?;
    let crc32 = linker_utils::calculate_crc32(path.as_ref())
        .context("failed to to calculate CRC32 of linker executable")?;

    let patch = link_patcher::find_patch(
        File::open(path.as_ref()).context("failed to open linker executable for reading")?,
    )
    .context("failed to find patch for linker")?;

    Ok(PatchInfo {
        product_name: version_info.product_name.unwrap_or_default(),
        product_version: version_info.product_version.unwrap_or_default(),
        architecture,
        crc32,
        patch,
    })
}

fn write_patch_table(writer: &mut dyn Write, patch_infos: &[PatchInfo]) -> Fallible<()> {
    const CAPTIONS: [&str; 7] = [
        "Product Name",
        "Version",
        "Arch",
        "CRC32",
        "Offset",
        "Original Bytes",
        "Patch Bytes",
    ];

    // Initialize column widths with the width of the captions.
    let mut column_widths = [0; 7];
    for (len, column_width) in CAPTIONS
        .iter()
        .map(|s| s.chars().count())
        .zip(column_widths.iter_mut())
    {
        *column_width = len;
    }

    // Determine the required column widths by evaluating the length of every value.
    for patch_info in patch_infos {
        for (len, column_width) in patch_info
            .to_strings()
            .into_iter()
            .map(|s| s.len())
            .zip(column_widths.iter_mut())
        {
            if len > *column_width {
                *column_width = len;
            }
        }
    }

    let gen_prefix_and_suffix = || {
        let mut index = 0;
        let len = CAPTIONS.len();
        std::iter::from_fn(move || {
            let is_first = index == 0;
            let is_last = index == len - 1;
            index += 1;

            if index > len {
                None
            } else {
                Some((
                    if is_first { "| " } else { "" },
                    if is_last { " |\n" } else { " | " },
                ))
            }
        })
    };

    // Write caption line.
    for ((caption, column_width), (prefix, suffix)) in CAPTIONS
        .iter()
        .zip(column_widths.iter())
        .zip(gen_prefix_and_suffix())
    {
        write!(
            writer,
            "{0}{1:2$}{3}",
            prefix, caption, column_width, suffix
        )?;
    }

    // Write separator line.
    for (column_width, (prefix, suffix)) in column_widths.iter().zip(gen_prefix_and_suffix()) {
        write!(writer, "{0}{1:-<2$}{3}", prefix, "", column_width, suffix)?;
    }

    // Write content lines.
    for strings in patch_infos.iter().map(|i| i.to_strings()) {
        for ((string, column_width), (prefix, suffix)) in strings
            .iter()
            .zip(column_widths.iter())
            .zip(gen_prefix_and_suffix())
        {
            write!(writer, "{0}{1:2$}{3}", prefix, string, column_width, suffix)?;
        }
    }

    Ok(())
}

fn run() -> Fallible<()> {
    let base_dir = linker_utils::get_link_executable_base_dir()
        .context("failed to get link executable base dir")?;

    let mut patch_infos = Vec::new();
    for entry in walkdir::WalkDir::new(base_dir)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.into_path();
        if path.is_file() {
            let file_name = path
                .file_name()
                .unwrap_or_default()
                .to_str()
                .unwrap_or_default();
            if file_name.eq_ignore_ascii_case("link.exe") {
                println!("Generating patch info for \"{}\" ...", path.display());
                patch_infos.push(generate_patch_info(&path).with_context(|_| {
                    format!("failed to generate patch info for \"{}\"", path.display())
                })?);
            }
        }
    }

    let readme_md_file_name = [&env!("CARGO_MANIFEST_DIR"), &"..", &"..", &"README.md"]
        .iter()
        .collect::<PathBuf>()
        .canonicalize()
        .context("failed to canonicalize path")?;

    println!(
        "Replacing patch table in \"{}\" ...",
        readme_md_file_name.display()
    );

    let readme_content = File::open(&readme_md_file_name)
        .and_then(|mut file| {
            let mut content = Vec::new();
            match file.read_to_end(&mut content) {
                Ok(_) => Ok(content),
                Err(err) => Err(err),
            }
        })
        .context("failed to read README.md")?;

    let mut readme_file = BufWriter::new(
        File::create(&readme_md_file_name).context("failed to open README.md for writing")?,
    );

    // Copy content from README.md until the start of the patch table.
    for line in readme_content.lines() {
        let line = line.expect("reading from Vec<u8> cannot fail");
        if line.starts_with("| Product Name") {
            break;
        }
        writeln!(&mut readme_file, "{}", line).context("failed to write to README.md")?;
    }

    // Write the new patch table to the end of README.md.
    write_patch_table(&mut readme_file, &patch_infos).context("failed to write patch table")?;

    Ok(())
}

quick_main!(run);
