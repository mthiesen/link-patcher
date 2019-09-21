# link-patcher

It is a poorly kept secret that the Microsoft Linker emits an undocumented data structure between the DOS stub executable and the actual Windows executable data. This data structure contains (poorly) encrypted information about the environment in which the executable was produced. The information can be used in computer forensics to identify authors of executables.

This excellent [article](http://bytepointer.com/articles/the_microsoft_rich_header.htm) explains the so-called 'Rich' header in great detail.

This tool can automatically patch any version of the Microsoft Linker so that it does not produce the 'Rich' header.

The images below compare an executable produced by an unpatched linker and one produced by a linker patched with this tool:

| Before | After |
| :-----:|:-----:|
![before](https://raw.githubusercontent.com/mthiesen/link-patcher/master/images/before.png) | ![after](https://raw.githubusercontent.com/mthiesen/link-patcher/master/images/after.png)

# Usage

```
link-patcher 1.0.0
Malte Thiesen <malte@kamalook.de>
Patches the Microsoft Linker so that it produces executables without the 'Rich' header

USAGE:
    link-patcher.exe [FLAGS] <input_file>

FLAGS:
    -a, --apply_patch    Applies the patch to the executable after a manual confirmation. A back-up of the original file
                         is created.
    -h, --help           Prints help information
    -V, --version        Prints version information

ARGS:
    <input_file>
```

![usage_example](https://raw.githubusercontent.com/mthiesen/link-patcher/master/images/usage_example.png)

# How does this work?

The function that writes the 'Rich' header is called `IMAGE::CbBuildProdidBlock()` as revealed by the debug information for `link.exe` on the Microsoft public symbol server. The article linked above suggests a manual process that involves patching all call sites of this function. The patch lets the linker generate and write the structure but removes the instruction that advances the write pointer, so that the next chunk of data written overwrites the 'Rich' header.

I am using a different approach to reduce the amount of analysis I have to do. It is not trivial to find all call sites of `IMAGE::CbBuildProdidBlock()` automatically. Instead, I aim to patch the function itself, so that it always returns 0. Usually, the function returns the size of the generated header structure in bytes. If this is always 0, the effect is the same as with the other patch: the header data is created and written, but the write pointer is not advanced.

This is a rough overview of the patching process:
1. Find a range of bytes in the executable code segment where the two constants used by the function (`Rich` and `DanS`) appear in close proximity.
2. Disassemble the range of bytes using the excellent [Capstone-rs](https://github.com/capstone-rust/capstone-rs) crate.
3. Find the next `ret` instruction after the last usage of the constants.
4. Scan back to find the last modification of `eax` before `ret`. This is where the return value is set.
5. Replace this instruction with `xor eax, eax` and pad the remaining instruction bytes with `nop`. This sets the return value to 0.

As you can see, this approach is not very sophisticated, for instance, I don't do any flow analysis, I just assume that the next `ret` instruction is the one that is actually taken. Simple as it may be, in practice, the tool works just fine. It reliably finds correct patches for all versions of `link.exe` that I could get hold of.

The table below lists the found patches. I am using integration tests to verify that the patched linker executables are still working and do not produce a 'Rich' header.

| Product Name         | Version       | Arch | CRC32    | Offset | Original Bytes | Patch Bytes    |
| -------------------- | ------------- | ---- | -------- | ------ | -------------- | -------------- |
| Visual Studio® 2012  | 11.00.60610.1 | x86  | B3394C37 | 131156 | 8B, 45, F0     | 33, C0, 90     |
| Visual Studio® 2013  | 12.00.31101.0 | x64  | C437E09D | 71872  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2013  | 12.00.31101.0 | x86  | 25E2A7D2 | 196317 | 8B, 45, F4     | 33, C0, 90     |
| Visual Studio® 2015  | 14.00.23506.0 | x64  | 290A1F33 | 191599 | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2015  | 14.00.23506.0 | x64  | 469132E2 | 191599 | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2015  | 14.00.23506.0 | x64  | C8329F25 | 191599 | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2015  | 14.00.23506.0 | x86  | 4DB6C257 | 275951 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2015  | 14.00.23506.0 | x86  | 8CE0E765 | 275951 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2015  | 14.00.23506.0 | x86  | B0318E7D | 275951 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2017  | 14.16.27023.1 | x64  | 4A2BBDF9 | 43507  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2017  | 14.16.27023.1 | x64  | D0C167D8 | 43507  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2017  | 14.16.27023.1 | x86  | CDCFB8B4 | 194755 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2017  | 14.16.27023.1 | x86  | F1B9D588 | 194755 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2017  | 14.16.27034.0 | x64  | 15C0D361 | 37975  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2017  | 14.16.27034.0 | x64  | ED33F6D0 | 37975  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2017  | 14.16.27034.0 | x86  | 5B305468 | 188401 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2017  | 14.16.27034.0 | x86  | A1D4FB95 | 188401 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.20.27508.1 | x64  | 315FE938 | 134063 | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.20.27508.1 | x64  | 74DAEA46 | 134063 | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.20.27508.1 | x86  | 379929A6 | 292218 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.20.27508.1 | x86  | EF0AE6B5 | 292218 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.22.27905.0 | x64  | 4BE95C7A | 139159 | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.22.27905.0 | x64  | 8325D8FF | 139159 | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.22.27905.0 | x86  | 30F46634 | 321551 | 8B, 44, 24, 1C | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.22.27905.0 | x86  | DB21C7D6 | 321551 | 8B, 44, 24, 1C | 33, C0, 90, 90 |
