use failure::bail;
use failure::Fallible;
use failure::ResultExt;
use std::path::Path;
use std::path::PathBuf;
use winapi::shared::minwindef::DWORD;

pub fn calculate_crc32(path: impl AsRef<Path>) -> Fallible<u32> {
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

pub fn get_windows_error_message(function_name: &str, error_code: DWORD) -> String {
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

pub fn get_last_windows_error_message(function_name: &str) -> String {
    use winapi::um::errhandlingapi::GetLastError;
    let error_code = unsafe { GetLastError() };
    get_windows_error_message(function_name, error_code)
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum Architecture {
    X64,
    X86,
}

pub fn get_architecture(path: impl AsRef<Path>) -> Fallible<Architecture> {
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

#[derive(Debug, Default)]
pub struct VersionInfo {
    pub file_description: Option<String>,
    pub product_version: Option<String>,
    pub product_name: Option<String>,
}

pub fn get_version_info(path: impl AsRef<Path>) -> Fallible<VersionInfo> {
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
                .expect("embedded 0 char not expected");

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

pub fn get_link_executable_base_dir() -> Fallible<PathBuf> {
    let mut dir = [&env!("CARGO_MANIFEST_DIR"), &"..", &"..", &"tests"]
        .iter()
        .collect::<PathBuf>()
        .canonicalize()
        .context("failed to canonicalize path")?;

    dir.push("link_executables");
    Ok(dir)
}
