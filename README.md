# link-patcher

It is a poorly kept secret that the Microsoft Linker emits an undocumented data structure between the DOS stub executable and the actual Windows executable data. This data structure contains (poorly) encrypted information about the environment in which the executable was produced. The information can be used in computer forensics to identify authors of executables.

This excellent [article](http://bytepointer.com/articles/the_microsoft_rich_header.htm) article explains the so-called 'Rich' header in great detail.

This tool can automatically patch any version of the Microsoft Linker so that it does not produce the `Rich` header.

The images below compare an executable produced by an unpatched linker and one produced by a linker patched with this tool:

| Before | After |
| :-----:|:-----:|
![before](https://raw.githubusercontent.com/mthiesen/link-patcher/documentation/images/before.png) | ![after](https://raw.githubusercontent.com/mthiesen/link-patcher/documentation/images/after.png)

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

![usage_example](https://raw.githubusercontent.com/mthiesen/link-patcher/documentation/images/usage_example.png)

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

| Version | Toolchain | MD5 | Offset | Original Bytes | Patch Bytes |
| ------- | --------- | ---- | ------ | -------------- | ----------- |
| 11.0.60610.1 | x86 | 589BC749F4A848DB2E3619FD4B7E123C | 131156 | 8B, 45, F0 | 33, C0, 90 |
| 12.0.31101.0 | x64 | 7E95F4DEB6C1FFF1CB46AA64F74A841E | 71872 | 41, 8B, C7 | 33, C0, 90 |
| 12.0.31101.0 | x86 | F8A40802C6B4AE3C89296DCA4694033E | 196317 | 8B, 45, F4 | 33, C0, 90 |
| 14.0.23506.0 | x64 | 9F892E5EFFD39FF007255E7D5E09B556 | 191599 | 41, 8B, C7 | 33, C0, 90 |
| 14.0.23506.0 | x64_arm | E55B931EE6AAA6EA24848B4090611251 | 191599 | 41, 8B, C7 | 33, C0, 90 |
| 14.0.23506.0 | x86_x86 | DBC81147C21CF51B732CC9815C9B79CD | 191599 | 41, 8B, C7 | 33, C0, 90 |
| 14.0.23506.0 | x86 | 318758BE70268F06CBEB7CC643473B24 | 275951 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| 14.0.23506.0 | x86_arm | 8436D1BCCE9721A786BA10530DC070C8 | 275951 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| 14.0.23506.0 | x86_x64 | 26A5EBDFE66C55F08EC891650819C589 |  275951 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| 14.14.26433.0 | x64 | 4859D04AC75C585DDDF48C134139ABEC | 190598 | 41, 8B, C7 | 33, C0, 90 |
| 14.14.26433.0 | x64_x86 | 46CE0FA20A7102FD886CD694D27805E2 | 190598 | 41, 8B, C7 | 33, C0, 90 |
| 14.14.26433.0 | x86 | 490ACCC54B2C8D454D884DA871509C12 | 213144 | 8B, 44, 24, 1C | 33, C0, 90, 90 |
| 14.14.26433.0 | x86_x64 | 44D14D0C5867E276FBB0B652718DF7BF | 213144 | 8B, 44, 24, 1C | 33, C0, 90, 90 |
| 14.15.26726.0 | x64 | 6757C9A07E56A0B65700B194C2E6A091 | 193435 | 41, 8B, C7 | 33, C0, 90 |
| 14.15.26726.0 | x64_x86 | 60260EC0718D6A333BD2EFF4ABADC32A | 193435 | 41, 8B, C7 | 33, C0, 90 |
| 14.15.26726.0 | x86 | DA1802133605543B2189956F428305A5 | 360041 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| 14.15.26726.0 | x86_x64 | 93987C84C70AB5EB22651C270007A30E | 360041 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
