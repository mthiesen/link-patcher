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
| Visual Studio® 2017  | 14.15.26727.0 | x86  | 6CCE014E | 360041 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2017  | 14.15.26727.0 | x86  | 8FAB3853 | 360041 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2017  | 14.15.26727.0 | x86  | BD22AE77 | 360041 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2017  | 14.15.26727.0 | x86  | DD41030E | 360041 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2017  | 14.16.27023.1 | x64  | 4A2BBDF9 | 43507  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2017  | 14.16.27023.1 | x64  | D0C167D8 | 43507  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2017  | 14.16.27023.1 | x86  | CDCFB8B4 | 194755 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2017  | 14.16.27023.1 | x86  | F1B9D588 | 194755 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2017  | 14.16.27034.0 | x64  | 15C0D361 | 37975  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2017  | 14.16.27034.0 | x64  | ED33F6D0 | 37975  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2017  | 14.16.27034.0 | x86  | 5B305468 | 188401 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2017  | 14.16.27034.0 | x86  | A1D4FB95 | 188401 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2017  | 14.16.27035.0 | x64  | 5CE70DC6 | 37975  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2017  | 14.16.27035.0 | x64  | F7A7DF38 | 37975  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2017  | 14.16.27035.0 | x86  | 3681CF5D | 188401 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2017  | 14.16.27035.0 | x86  | 7AD3C395 | 188401 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2017  | 14.16.27038.0 | x64  | 6C49EDF1 | 37975  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2017  | 14.16.27038.0 | x64  | B346EF94 | 37975  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2017  | 14.16.27038.0 | x86  | 04D69CA2 | 188401 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2017  | 14.16.27038.0 | x86  | 59D034C0 | 188401 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2017  | 14.16.27039.0 | x64  | 79CE70FF | 37975  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2017  | 14.16.27039.0 | x64  | FE81C00D | 37975  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2017  | 14.16.27039.0 | x86  | 235F63DA | 188385 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2017  | 14.16.27039.0 | x86  | B4289992 | 188385 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2017  | 14.16.27040.0 | x64  | 60FB51F0 | 37975  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2017  | 14.16.27040.0 | x64  | 743997F5 | 37975  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2017  | 14.16.27040.0 | x86  | 2D1A5224 | 188401 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2017  | 14.16.27040.0 | x86  | 75D10C92 | 188401 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2017  | 14.16.27041.0 | x64  | 5406D94A | 194455 | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2017  | 14.16.27041.0 | x64  | 6A9BD821 | 194455 | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2017  | 14.16.27041.0 | x86  | 43F13B7B | 188497 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2017  | 14.16.27041.0 | x86  | 73036C56 | 188497 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2017  | 14.16.27042.0 | x64  | 1EB36B8D | 194455 | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2017  | 14.16.27042.0 | x64  | ED0F74B7 | 194455 | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2017  | 14.16.27042.0 | x86  | 05848E15 | 188497 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2017  | 14.16.27042.0 | x86  | 7D2EEC64 | 188497 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2017  | 14.16.27043.0 | x64  | 4C86EB0D | 194455 | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2017  | 14.16.27043.0 | x64  | FF084808 | 194455 | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2017  | 14.16.27043.0 | x86  | 0293291C | 188497 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2017  | 14.16.27043.0 | x86  | 26CFB00A | 188497 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2017  | 14.16.27044.0 | x64  | 274F7BAA | 194455 | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2017  | 14.16.27044.0 | x64  | 3F83C278 | 194455 | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2017  | 14.16.27044.0 | x86  | 59CA7A71 | 188481 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2017  | 14.16.27044.0 | x86  | D5F6DC59 | 188481 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2017  | 14.16.27045.0 | x64  | 18DCE766 | 194455 | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2017  | 14.16.27045.0 | x64  | 6B2C4A75 | 194455 | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2017  | 14.16.27045.0 | x86  | 1906A13F | 188497 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2017  | 14.16.27045.0 | x86  | 3FFFD65E | 188497 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.20.27508.1 | x64  | 315FE938 | 134063 | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.20.27508.1 | x64  | 74DAEA46 | 134063 | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.20.27508.1 | x86  | 379929A6 | 292218 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.20.27508.1 | x86  | EF0AE6B5 | 292218 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.22.27905.0 | x64  | 4BE95C7A | 139159 | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.22.27905.0 | x64  | 8325D8FF | 139159 | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.22.27905.0 | x86  | 30F46634 | 321551 | 8B, 44, 24, 1C | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.22.27905.0 | x86  | DB21C7D6 | 321551 | 8B, 44, 24, 1C | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.23.28105.4 | x64  | 7628FAA4 | 94789  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.23.28105.4 | x64  | 78CAD64A | 94789  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.23.28105.4 | x86  | D0AD5DB1 | 269217 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.23.28105.4 | x86  | EEF7C503 | 269217 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.23.28107.0 | x64  | 376F8A4E | 94789  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.23.28107.0 | x64  | A109AC67 | 94789  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.23.28107.0 | x86  | B30C7736 | 269377 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.23.28107.0 | x86  | F61E80D7 | 269377 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.24.28315.0 | x64  | 92F6010A | 94549  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.24.28315.0 | x64  | D4AB0D58 | 94549  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.24.28315.0 | x86  | 3712FC0F | 254912 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.24.28315.0 | x86  | F3B83CE9 | 254912 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.24.28316.0 | x64  | 8FC73ED9 | 94549  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.24.28316.0 | x64  | E2A84610 | 94549  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.24.28316.0 | x86  | 09F1F6C2 | 254912 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.24.28316.0 | x86  | 89DF94B9 | 254912 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.24.28319.0 | x64  | 6CAFD741 | 94549  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.24.28319.0 | x64  | 78D7B4B3 | 94549  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.24.28319.0 | x86  | 961E30F7 | 254912 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.24.28319.0 | x86  | F76EA00B | 254912 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.25.28610.4 | x64  | 3A05DBED | 22547  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.25.28610.4 | x64  | 8A5DE40C | 22547  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.25.28610.4 | x86  | 4574406B | 244435 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.25.28610.4 | x86  | 63E66162 | 244435 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.25.28611.0 | x64  | 42D73CA0 | 22547  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.25.28611.0 | x64  | 7EBD440A | 22547  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.25.28611.0 | x86  | 456341BB | 244435 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.25.28611.0 | x86  | 4C3B866E | 244435 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.25.28612.0 | x64  | 881ABF59 | 22547  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.25.28612.0 | x64  | A185BA9D | 22547  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.25.28612.0 | x86  | 60AF4DA2 | 244435 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.25.28612.0 | x86  | 9BC4CC19 | 244435 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.25.28614.0 | x64  | 23337682 | 22547  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.25.28614.0 | x64  | 48029BB9 | 22547  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.25.28614.0 | x86  | 534C508E | 244435 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.25.28614.0 | x86  | B7FC7257 | 244435 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.26.28805.0 | x64  | 053B5150 | 99282  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.26.28805.0 | x64  | 3D8B07F7 | 99282  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.26.28805.0 | x86  | 4B37DC1C | 305043 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.26.28805.0 | x86  | 813D439F | 305043 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.26.28806.0 | x64  | 1B6D015F | 99282  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.26.28806.0 | x64  | E3B6BE73 | 99282  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.26.28806.0 | x86  | 4B62ED11 | 305043 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.26.28806.0 | x86  | 5DE84840 | 305043 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.27.29110.0 | x64  | 508B432F | 38734  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.27.29110.0 | x64  | CFBA966A | 38734  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.27.29110.0 | x86  | 4C381CAF | 356436 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.27.29110.0 | x86  | C25415AF | 356436 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.27.29111.0 | x64  | 4444ED9C | 38734  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.27.29111.0 | x64  | A20D0810 | 38734  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.27.29111.0 | x86  | A42C8765 | 356436 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.27.29111.0 | x86  | CEF0D6A9 | 356436 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.27.29112.0 | x64  | BBF2327F | 38734  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.27.29112.0 | x64  | D0172C6C | 38734  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.27.29112.0 | x86  | 497C27A5 | 356436 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.27.29112.0 | x86  | FB40F3A5 | 356436 | 8B, 44, 24, 10 | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.28.29333.0 | x64  | E3565E4D | 88466  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.28.29333.0 | x64  | E6A383DD | 88466  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.28.29333.0 | x86  | 29EE647E | 260723 | 8B, 44, 24, 14 | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.28.29333.0 | x86  | D6932A6F | 260723 | 8B, 44, 24, 14 | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.28.29334.0 | x64  | 93B6399B | 88466  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.28.29334.0 | x64  | A1D25FED | 88466  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.28.29334.0 | x86  | 4560993E | 260723 | 8B, 44, 24, 14 | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.28.29334.0 | x86  | 7DCFEE96 | 260723 | 8B, 44, 24, 14 | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.28.29335.0 | x64  | 1349EA09 | 88466  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.28.29335.0 | x64  | 3DB4F09F | 88466  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.28.29335.0 | x86  | 456F1CED | 260723 | 8B, 44, 24, 14 | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.28.29335.0 | x86  | AB1F9A98 | 260723 | 8B, 44, 24, 14 | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.28.29336.0 | x64  | 24B1E7C1 | 88466  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.28.29336.0 | x64  | BF6DA168 | 88466  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.28.29336.0 | x86  | 1E40FB13 | 260723 | 8B, 44, 24, 14 | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.28.29336.0 | x86  | 8538880A | 260723 | 8B, 44, 24, 14 | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.28.29337.0 | x64  | A6EDB6E9 | 88466  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.28.29337.0 | x64  | C904BB2B | 88466  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.28.29337.0 | x86  | 99FE5162 | 260723 | 8B, 44, 24, 14 | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.28.29337.0 | x86  | D4C3AD21 | 260723 | 8B, 44, 24, 14 | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.28.29910.0 | x64  | 6586F2D9 | 18662  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.28.29910.0 | x64  | 9D01B243 | 18662  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.28.29910.0 | x86  | 6247CCD0 | 321947 | 8B, 44, 24, 14 | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.28.29910.0 | x86  | EF632F13 | 321947 | 8B, 44, 24, 14 | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.28.29912.0 | x64  | D0A9B970 | 18662  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.28.29912.0 | x64  | FEB6BE70 | 18662  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.28.29912.0 | x86  | 31EF221D | 321947 | 8B, 44, 24, 14 | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.28.29912.0 | x86  | FF1D75A8 | 321947 | 8B, 44, 24, 14 | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.28.29913.0 | x64  | 2A5300AE | 18662  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.28.29913.0 | x64  | D703BE8B | 18662  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.28.29913.0 | x86  | B6A7E6B5 | 321947 | 8B, 44, 24, 14 | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.28.29913.0 | x86  | F2E2B519 | 321947 | 8B, 44, 24, 14 | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.28.29914.0 | x64  | 2B18BEA1 | 18662  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.28.29914.0 | x64  | 4ACBDEB3 | 18662  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.28.29914.0 | x86  | 132085E1 | 321947 | 8B, 44, 24, 14 | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.28.29914.0 | x86  | 205A4ECE | 321947 | 8B, 44, 24, 14 | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.28.29915.0 | x64  | B5A8F379 | 18662  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.28.29915.0 | x64  | EE9B4219 | 18662  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.28.29915.0 | x86  | 0B51478A | 322171 | 8B, 44, 24, 14 | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.28.29915.0 | x86  | 1AB4A05D | 322171 | 8B, 44, 24, 14 | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.29.30037.0 | x64  | 56BA3E8B | 26370  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.29.30037.0 | x64  | 9928DC2C | 26370  | 41, 8B, C7     | 33, C0, 90     |
| Visual Studio® 2019  | 14.29.30037.0 | x86  | 3C71F56D | 258994 | 8B, 44, 24, 14 | 33, C0, 90, 90 |
| Visual Studio® 2019  | 14.29.30037.0 | x86  | 56FDF20E | 258994 | 8B, 44, 24, 14 | 33, C0, 90, 90 |
