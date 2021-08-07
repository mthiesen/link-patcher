use common_failures::quick_main;
use failure::bail;
use failure::Fallible;
use failure::ResultExt;
use std::path::Path;
use std::path::PathBuf;
use walkdir::WalkDir;
use winapi::shared::minwindef::DWORD;

fn get_program_files_paths() -> Fallible<Vec<PathBuf>> {
    use winapi::shared::minwindef::MAX_PATH;
    use winapi::shared::winerror;
    use winapi::um::shlobj::SHGetFolderPathW;
    use winapi::um::shlobj::CSIDL_PROGRAM_FILES;
    use winapi::um::shlobj::CSIDL_PROGRAM_FILESX86;
    use winapi::um::winnt::LPWSTR;

    let mut buffer = [0u16; MAX_PATH];

    let mut paths = Vec::new();
    for csidl in [CSIDL_PROGRAM_FILESX86, CSIDL_PROGRAM_FILES] {
        unsafe {
            let result = SHGetFolderPathW(
                std::ptr::null_mut(),
                csidl,
                std::ptr::null_mut(),
                0 as DWORD,
                &mut buffer[0] as LPWSTR,
            );

            if winerror::IS_ERROR(result) {
                let error_code = winerror::HRESULT_CODE(result) as DWORD;
                bail!(linker_utils::get_windows_error_message(
                    "SHGetFolderPathW",
                    error_code
                ));
            }
        };

        if let Some(len) = buffer.iter().position(|c| *c == 0) {
            let path = String::from_utf16(&buffer[..len])
                .context("SHGetFolderPathW returned malformed UTF-16")?;
            paths.push(PathBuf::from(path));
        } else {
            bail!("SHGetFolderPathW did not return a null terminated string");
        }
    }

    Ok(paths)
}

fn path_to_file_name(path: &Path) -> &str {
    path.file_name()
        .unwrap_or_default()
        .to_str()
        .unwrap_or_default()
}

#[derive(Debug)]
enum PathType<'a> {
    HostTargetHierarchy(&'a str, &'a str),
    HostTargetSingle(&'a str),
    Bin,
    Unknown,
}

fn determine_path_type(path: &Path) -> PathType {
    let (parent_dir, parent_parent_dir) = {
        let mut component_iter = path.components().rev().skip(1).map(|path_component| {
            if let std::path::Component::Normal(os_str) = path_component {
                os_str.to_str().unwrap_or_default()
            } else {
                ""
            }
        });

        (
            component_iter.next().unwrap_or_default(),
            component_iter.next().unwrap_or_default(),
        )
    };

    if parent_dir.eq_ignore_ascii_case("bin") {
        return PathType::Bin;
    }

    if (parent_parent_dir.eq_ignore_ascii_case("Hostx64")
        || parent_parent_dir.eq_ignore_ascii_case("Hostx86"))
        && (parent_dir.eq_ignore_ascii_case("x86") || parent_dir.eq_ignore_ascii_case("x64"))
    {
        return PathType::HostTargetHierarchy(parent_parent_dir, parent_dir);
    }

    if parent_dir.eq_ignore_ascii_case("amd64")
        || parent_dir.eq_ignore_ascii_case("x86")
        || parent_dir.eq_ignore_ascii_case("x86_arm")
        || parent_dir.eq_ignore_ascii_case("x86_amd64")
        || parent_dir.eq_ignore_ascii_case("amd64_x86")
        || parent_dir.eq_ignore_ascii_case("amd64_arm")
    {
        return PathType::HostTargetSingle(parent_dir);
    }

    PathType::Unknown
}

fn find_dlls(path_type: &PathType, path: impl AsRef<Path>) -> Fallible<Vec<PathBuf>> {
    let find_dlls_in_dir = |dir: &Path| -> Fallible<Vec<PathBuf>> {
        let inner = |dir: &Path| -> Fallible<Vec<PathBuf>> {
            let mut found_dlls = Vec::new();

            let dir = std::fs::read_dir(dir).context("failed to read directory")?;
            for entry in dir {
                let entry = entry.context("failed to read directory entry")?;
                let path = entry.path();

                if !path.is_file() {
                    continue;
                }

                let file_name = path_to_file_name(&path);

                let is_dll = file_name.len() > 4
                    && file_name[file_name.len() - 4..].eq_ignore_ascii_case(".dll");
                if !is_dll {
                    continue;
                }

                let is_mspdb = file_name.len() > 5 && file_name[..5].eq_ignore_ascii_case("mspdb");
                let is_tbbmalloc = file_name.eq_ignore_ascii_case("tbbmalloc.dll");
                if is_mspdb || is_tbbmalloc {
                    found_dlls.push(path.to_owned());
                }
            }

            Ok(found_dlls)
        };

        Ok(inner(dir).context(format!("failed to find PDB DLLs in \"{}\"", dir.display()))?)
    };

    let parent_dir = path
        .as_ref()
        .parent()
        .expect("executable path has to have a parent directory");

    let mut found_dlls = find_dlls_in_dir(&parent_dir)?;

    if found_dlls.is_empty() {
        match path_type {
            PathType::HostTargetHierarchy(_, target_arch) => {
                // With the Host<HOST_ARCH>\<TARGET_ARCH>\link.exe path format, the PDB DLLs
                // are sometimes stored in a directory for another target architecture.
                let parent_parent_dir = parent_dir.parent().expect(
                    "executable path of type HostTargetHierarchy has to have two parent directories",
                );
                let mut search_dir = parent_parent_dir.to_owned();
                search_dir.push(if *target_arch == "x86" { "x64" } else { "x86" });
                found_dlls = find_dlls_in_dir(&search_dir)?;
            }
            PathType::HostTargetSingle(dir) => {
                if let Some(parent_parent_dir) = parent_dir.parent() {
                    if dir.len() > 4 && dir[..4].eq_ignore_ascii_case("x86_") {
                        found_dlls = find_dlls_in_dir(&parent_parent_dir)?;

                        if found_dlls.is_empty() {
                            let mut search_dir = parent_dir.to_owned();
                            search_dir.push("../../../Common7/IDE");
                            if search_dir.is_dir() {
                                found_dlls = find_dlls_in_dir(&search_dir)?;
                            }
                        }
                    } else if dir.len() > 6 && dir[..6].eq_ignore_ascii_case("amd64_") {
                        let mut search_dir = parent_parent_dir.to_owned();
                        search_dir.push("amd64");
                        found_dlls = find_dlls_in_dir(&search_dir)?;
                    }
                }
            }
            PathType::Bin => {
                let mut search_dir = parent_dir.to_owned();
                search_dir.push("../../Common7/IDE");
                if search_dir.is_dir() {
                    found_dlls = find_dlls_in_dir(&search_dir)?;
                }
            }
            PathType::Unknown => {}
        }
    }

    Ok(found_dlls)
}

#[derive(Debug)]
struct LinkProgramInfo {
    path: PathBuf,
    product_name: String,
    version: String,
    architecture: linker_utils::Architecture,
    crc32: u32,
    dlls: Vec<PathBuf>,
}

impl LinkProgramInfo {
    fn new(path: impl AsRef<Path>) -> Fallible<LinkProgramInfo> {
        let version_info = linker_utils::get_version_info(path.as_ref())
            .context("failed to retrieve version info")?;

        if let Some(file_description) = version_info.file_description {
            const EXPECTED_FILE_DESCRIPTION: &str = "Microsoft® Incremental Linker";
            if EXPECTED_FILE_DESCRIPTION != file_description {
                bail!(
                    "file description is \"{}\", expected: \"{}\"",
                    file_description,
                    EXPECTED_FILE_DESCRIPTION
                );
            }
        } else {
            bail!("file has no file description");
        }

        let product_name = match version_info.product_name {
            Some(product_name) => product_name,
            None => bail!("file has no product name"),
        };

        let product_version = version_info.product_version.unwrap_or_default();
        if product_version.is_empty() {
            bail!("file has no product version");
        }

        let path_type = determine_path_type(path.as_ref());
        let architecture = linker_utils::get_architecture(path.as_ref())
            .context("failed to determine architecture")?;

        let dlls = find_dlls(&path_type, &path).context("failed to find DLL dependencies")?;
        if dlls.is_empty() {
            bail!("failed to find any DLL dependencies");
        }

        let crc32 = linker_utils::calculate_crc32(&path).context("failed to calculate CRC32")?;

        Ok(LinkProgramInfo {
            path: path.as_ref().to_owned(),
            product_name,
            version: product_version,
            architecture,
            dlls,
            crc32,
        })
    }

    fn target_directory_name(&self) -> PathBuf {
        format!(
            "{}_{:?}_{:08X}",
            self.version, self.architecture, self.crc32
        )
        .into()
    }
}

fn print_error(error: &failure::Error) -> Fallible<()> {
    use std::io::Write;

    let stdout = std::io::stdout();
    let mut handle = stdout.lock();

    writeln!(handle, "error: {}", error).context("failed to write to stdout")?;
    for cause in error.iter_causes() {
        writeln!(handle, "caused by: {}", cause).context("failed to write to stdout")?;
    }

    Ok(())
}

fn make_writeable(path: impl AsRef<Path>) -> Fallible<()> {
    let mut permissions = path.as_ref().metadata()?.permissions();
    permissions.set_readonly(false);
    std::fs::set_permissions(path.as_ref(), permissions)?;

    Ok(())
}

fn archive_link_binaries(
    target_base_dir: impl AsRef<Path>,
    link_program_info: &LinkProgramInfo,
) -> Fallible<()> {
    let target_directory: PathBuf = [
        target_base_dir.as_ref(),
        &link_program_info.target_directory_name(),
    ]
    .iter()
    .collect();

    if target_directory.is_dir() {
        println!("SKIPPING: {}", link_program_info.path.display());
        println!("reason: target directory already exists");
        println!();
        return Ok(());
    }

    println!("FOUND: {}", link_program_info.path.display());
    println!("    Product Name: {}", link_program_info.product_name);
    println!("    Version     : {}", link_program_info.version);
    println!("    Architecture: {:?}", link_program_info.architecture);
    println!("    CRC32       : {:08X}", link_program_info.crc32);
    println!(
        "Copying linker binaries to \"{}\" ...",
        target_directory.display()
    );
    println!();

    std::fs::create_dir_all(&target_directory).with_context(|_| {
        format!(
            "failed to create target directory \"{}\"",
            target_directory.display()
        )
    })?;

    let files_to_copy =
        std::iter::once(&link_program_info.path).chain(link_program_info.dlls.iter());
    for src_file in files_to_copy {
        let mut dst_file = target_directory.to_owned();
        dst_file.push(
            src_file
                .file_name()
                .expect("src path has to end with a file name"),
        );

        std::fs::copy(&src_file, &dst_file).with_context(|_| {
            format!(
                "failed to copy \"{}\" to \"{}\"",
                src_file.display(),
                dst_file.display()
            )
        })?;

        // Make sure that the copied files are writable. Subsequent patch operations could fail
        // otherwise.
        make_writeable(&dst_file).context(format!(
            "failed to make \"{}\" writeable",
            dst_file.display()
        ))?;
    }

    Ok(())
}

fn run() -> Fallible<()> {
    let target_base_dir = linker_utils::get_link_executable_base_dir()
        .context("failed to get link executable base dir")?;

    println!("Target directory is \"{}\"", target_base_dir.display());

    for path_files_path in get_program_files_paths()? {
        println!(
            "Scanning \"{}\" for Microsoft Linkers ...",
            path_files_path.display()
        );
        println!();

        for entry in WalkDir::new(path_files_path)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.into_path();
            if path.is_file() {
                let file_name = path_to_file_name(&path);
                if file_name.eq_ignore_ascii_case("link.exe") {
                    match LinkProgramInfo::new(&path) {
                        Ok(link_program_info) => {
                            archive_link_binaries(&target_base_dir, &link_program_info)
                                .with_context(|_| {
                                    format!(
                                        "failed to archive link binaries for \"{}\"",
                                        path.display()
                                    )
                                })?
                        }
                        Err(err) => {
                            println!("SKIPPING: {}", path.display());
                            print_error(&err)?;
                            println!();
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

quick_main!(run);
