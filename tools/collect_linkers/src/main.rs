use common_failures::quick_main;
use failure::bail;
use failure::Fallible;
use failure::ResultExt;
use std::path::Path;
use std::path::PathBuf;
use walkdir::WalkDir;
use winapi::shared::minwindef::DWORD;

fn get_windows_error_message(function_name: &str, error_code: DWORD) -> String {
    use winapi::shared::ntdef::WCHAR;
    use winapi::um::winbase::FormatMessageW;
    use winapi::um::winbase::FORMAT_MESSAGE_FROM_SYSTEM;
    use winapi::um::winbase::FORMAT_MESSAGE_IGNORE_INSERTS;

    const LANG_ID: DWORD = 0x0800 as DWORD; // MAKELANGID(LANG_SYSTEM_DEFAULT, SUBLANG_SYS_DEFAULT)
    const FLAGS: DWORD = FORMAT_MESSAGE_FROM_SYSTEM | FORMAT_MESSAGE_IGNORE_INSERTS;

    let mut buffer = [0 as WCHAR; 2048];

    let result = unsafe {
        FormatMessageW(
            FLAGS,
            std::ptr::null_mut(),
            error_code,
            LANG_ID,
            buffer.as_mut_ptr(),
            buffer.len() as DWORD,
            std::ptr::null_mut(),
        )
    };

    if result == 0 {
        format!("{} failed: os error code {}", function_name, error_code)
    } else {
        let mut message = String::from_utf16_lossy(&buffer[..result as usize]);

        // Remove trailing whitespace. In particular the CRLF that FormatMessage inserts at the end.
        while let Some(last_char) = message.chars().last() {
            if last_char.is_whitespace() {
                message.pop();
            } else {
                break;
            }
        }

        format!(
            "{} failed: {} (os error code {})",
            function_name, message, error_code
        )
    }
}

fn get_last_windows_error_message(function_name: &str) -> String {
    use winapi::um::errhandlingapi::GetLastError;
    let error_code = unsafe { GetLastError() };
    get_windows_error_message(function_name, error_code)
}

fn get_program_files_x86_path() -> Fallible<PathBuf> {
    use winapi::shared::minwindef::MAX_PATH;
    use winapi::shared::winerror;
    use winapi::um::shlobj::SHGetFolderPathW;
    use winapi::um::shlobj::CSIDL_PROGRAM_FILESX86;
    use winapi::um::winnt::LPWSTR;

    let mut buffer = [0u16; MAX_PATH];

    unsafe {
        let result = SHGetFolderPathW(
            std::ptr::null_mut(),
            CSIDL_PROGRAM_FILESX86,
            std::ptr::null_mut(),
            0 as DWORD,
            &mut buffer[0] as LPWSTR,
        );

        if winerror::IS_ERROR(result) {
            let error_code = winerror::HRESULT_CODE(result) as DWORD;
            bail!(get_windows_error_message("SHGetFolderPathW", error_code));
        }
    };

    if let Some(len) = buffer.iter().position(|c| *c == 0) {
        let path = String::from_utf16(&buffer[..len])
            .context("SHGetFolderPathW returned malformed UTF-16")?;
        Ok(PathBuf::from(path))
    } else {
        bail!("SHGetFolderPathW did not return a null terminated string");
    }
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
enum Architecture {
    X64,
    X86,
}

fn get_architecture(path: impl AsRef<Path>) -> Fallible<Architecture> {
    use winapi::um::winbase::GetBinaryTypeW;

    let wide_path =
        widestring::U16CString::from_os_str(path.as_ref()).expect("path cannot contain 0");

    let result = unsafe {
        let mut result: DWORD = 0;
        let success = GetBinaryTypeW(wide_path.as_ptr(), &mut result as *mut DWORD) != 0;
        if !success {
            bail!(get_last_windows_error_message("GetBinaryTypeW"));
        }
        result
    };

    const SCS_32BIT_BINARY: DWORD = 0;
    const SCS_64BIT_BINARY: DWORD = 6;

    Ok(match result {
        SCS_32BIT_BINARY => Architecture::X86,
        SCS_64BIT_BINARY => Architecture::X64,
        _ => bail!("file is of unsupported architecture"),
    })
}

fn calculate_crc32(path: impl AsRef<Path>) -> Fallible<u32> {
    use std::fs::File;
    use std::io::Read;

    let mut file = File::open(path.as_ref())
        .with_context(|_| format!("failed to open \"{}\" for reading", path.as_ref().display()))?;

    let mut hasher = crc32fast::Hasher::new();

    let mut buffer = [0u8; 1024 * 16];
    loop {
        match file.read(&mut buffer) {
            Ok(bytes_read) => {
                if bytes_read > 0 {
                    hasher.update(&buffer[..bytes_read]);
                } else {
                    return Ok(hasher.finalize());
                }
            }
            Err(err) => match err.kind() {
                std::io::ErrorKind::Interrupted => (),
                _ => Err(err).with_context(|_| {
                    format!("failed to read from \"{}\"", path.as_ref().display())
                })?,
            },
        }
    }
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

fn find_pdb_dlls(path_type: &PathType, path: impl AsRef<Path>) -> Fallible<Vec<PathBuf>> {
    let find_pdb_dlls_in_dir = |dir: &Path| -> Fallible<Vec<PathBuf>> {
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
                if is_mspdb {
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

    let mut found_dlls = find_pdb_dlls_in_dir(&parent_dir)?;

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
                found_dlls = find_pdb_dlls_in_dir(&search_dir)?;
            }
            PathType::HostTargetSingle(dir) => {
                if let Some(parent_parent_dir) = parent_dir.parent() {
                    if dir.len() > 4 && dir[..4].eq_ignore_ascii_case("x86_") {
                        found_dlls = find_pdb_dlls_in_dir(&parent_parent_dir)?;

                        if found_dlls.is_empty() {
                            let mut search_dir = parent_dir.to_owned();
                            search_dir.push("../../../Common7/IDE");
                            if search_dir.is_dir() {
                                found_dlls = find_pdb_dlls_in_dir(&search_dir)?;
                            }
                        }
                    } else if dir.len() > 6 && dir[..6].eq_ignore_ascii_case("amd64_") {
                        let mut search_dir = parent_parent_dir.to_owned();
                        search_dir.push("amd64");
                        found_dlls = find_pdb_dlls_in_dir(&search_dir)?;
                    }
                }
            }
            PathType::Bin => {
                let mut search_dir = parent_dir.to_owned();
                search_dir.push("../../Common7/IDE");
                if search_dir.is_dir() {
                    found_dlls = find_pdb_dlls_in_dir(&search_dir)?;
                }
            }
            PathType::Unknown => {}
        }
    }

    Ok(found_dlls)
}

#[derive(Debug, Default)]
struct VersionInfo {
    file_description: Option<String>,
    product_version: Option<String>,
    product_name: Option<String>,
}

fn get_version_info(path: impl AsRef<Path>) -> Fallible<VersionInfo> {
    use winapi::shared::minwindef::LPVOID;
    use winapi::shared::minwindef::PUINT;
    use winapi::shared::minwindef::UINT;
    use winapi::shared::winerror::ERROR_RESOURCE_TYPE_NOT_FOUND;
    use winapi::um::errhandlingapi::GetLastError;
    use winapi::um::winver::GetFileVersionInfoSizeW;
    use winapi::um::winver::GetFileVersionInfoW;
    use winapi::um::winver::VerQueryValueW;

    let wide_path =
        widestring::U16CString::from_os_str(path.as_ref()).expect("path cannot contain 0");

    let version_info_len = {
        let mut dummy = 0 as DWORD;
        unsafe { GetFileVersionInfoSizeW(wide_path.as_ptr(), &mut dummy as *mut DWORD) }
    };

    if version_info_len == 0 {
        let error_code = unsafe { GetLastError() };
        if error_code == ERROR_RESOURCE_TYPE_NOT_FOUND {
            // We don't consider it an error if there is no version information present.
            return Ok(VersionInfo::default());
        } else {
            bail!(get_windows_error_message(
                "GetFileVersionInfoSizeW",
                error_code
            ));
        }
    }

    let version_info = {
        let mut version_info = vec![0u8; version_info_len as usize];
        if unsafe {
            GetFileVersionInfoW(
                wide_path.as_ptr(),
                0 as DWORD,
                version_info_len,
                version_info.as_mut_ptr() as LPVOID,
            )
        } == 0
        {
            bail!(get_last_windows_error_message("GetFileVersionInfoW"));
        }

        version_info
    };

    let query_value = |info| -> Option<String> {
        let sub_block =
            widestring::U16CString::from_str(format!(r"\StringFileInfo\040904B0\{}", info))
                .expect("Embedded 0 char not expected");

        let wide_slice: &[u16] = unsafe {
            let mut len = 0 as UINT;
            let mut string_ptr = std::ptr::null_mut();

            if VerQueryValueW(
                version_info.as_ptr() as LPVOID,
                sub_block.as_ptr(),
                &mut string_ptr,
                &mut len as PUINT,
            ) == 0
            {
                &[]
            } else {
                std::slice::from_raw_parts(string_ptr as *const u16, len as usize)
            }
        };

        if wide_slice.is_empty() {
            return None;
        }
        if let Ok(mut string) = String::from_utf16(wide_slice) {
            if let Some(last_char) = string.chars().last() {
                if last_char == '\u{0}' {
                    string.pop();
                }
            }
            Some(string)
        } else {
            None
        }
    };

    Ok(VersionInfo {
        file_description: query_value("FileDescription"),
        product_version: query_value("ProductVersion"),
        product_name: query_value("ProductName"),
    })
}

#[derive(Debug)]
struct LinkProgramInfo {
    path: PathBuf,
    product_name: String,
    version: String,
    architecture: Architecture,
    crc32: u32,
    pdb_dlls: Vec<PathBuf>,
}

impl LinkProgramInfo {
    fn new(path: impl AsRef<Path>) -> Fallible<LinkProgramInfo> {
        let version_info =
            get_version_info(path.as_ref()).context("failed to retrieve version info")?;

        if let Some(file_description) = version_info.file_description {
            const EXPECTED_FILE_DESCRIPTION: &str = "Microsoft® Incremental Linker";
            if file_description != EXPECTED_FILE_DESCRIPTION {
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
        let architecture =
            get_architecture(path.as_ref()).context("failed to determine architecture")?;

        let pdb_dlls = find_pdb_dlls(&path_type, &path).context("failed to find PDB DLLs")?;
        if pdb_dlls.is_empty() {
            bail!("failed to find any PDB DLLs");
        }

        let crc32 = calculate_crc32(&path).context("failed to calculate CRC32")?;

        Ok(LinkProgramInfo {
            path: path.as_ref().to_owned(),
            product_name,
            version: product_version,
            architecture,
            pdb_dlls,
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
        std::iter::once(&link_program_info.path).chain(link_program_info.pdb_dlls.iter());
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
    let target_base_dir = {
        let mut target_base_dir = [&env!("CARGO_MANIFEST_DIR"), &"..", &"..", &"tests"]
            .iter()
            .collect::<PathBuf>()
            .canonicalize()
            .context("failed to canonicalize target base dir")?;

        target_base_dir.push("link_executables");

        target_base_dir
    };

    println!("Target directory is \"{}\"", target_base_dir.display());

    let program_files_x86_path = get_program_files_x86_path()?;
    println!(
        "Scanning \"{}\" for Microsoft Linkers ...",
        program_files_x86_path.display()
    );
    println!();

    for entry in WalkDir::new(program_files_x86_path)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.into_path();
        if path.is_file() {
            let file_name = path_to_file_name(&path);
            if file_name.eq_ignore_ascii_case("link.exe") {
                match LinkProgramInfo::new(&path) {
                    Ok(link_program_info) => archive_link_binaries(
                        &target_base_dir,
                        &link_program_info,
                    )
                    .with_context(|_| {
                        format!("failed to archive link binaries for \"{}\"", path.display())
                    })?,
                    Err(err) => {
                        println!("SKIPPING: {}", path.display());
                        print_error(&err)?;
                        println!();
                    }
                }
            }
        }
    }

    Ok(())
}

quick_main!(run);
