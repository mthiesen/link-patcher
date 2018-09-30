This folder should contain a copy of all linker versions that should be tested by the integration test. I cannot include them in the public repository because they are the intellectual property of Microsoft.

Create a folder for each version of the linker you want to test. The build script scans all folders and creates test cases from them that test that the linker is correctly patched and still works. The name of the generated test case is the directory name with the prefix `patched_`.

Be aware that the individual directories have to contain the `link.exe` and all the dependencies that are needed to link executables. In my experience, all linkers depend on a DLL starting with the prefix `mspdb`. There is only one of those in the same directory as the `link.exe`.

For example, this is the folder structure on my machine:
```
├───11.0.60610.1_x86
│       link.exe
│       mspdb110.dll
│
├───12.0.31101.0_x64
│       link.exe
│       mspdbst.dll
│
├───12.0.31101.0_x86
│       link.exe
│       mspdb120.dll
│
├───14.0.23506.0_x64
│       link.exe
│       mspdbcore.dll
│
├───14.0.23506.0_x64_arm
│       link.exe
│       mspdbcore.dll
│
├───14.0.23506.0_x64_x86
│       link.exe
│       mspdbcore.dll
│
├───14.0.23506.0_x86
│       link.exe
│       mspdb140.dll
│
├───14.0.23506.0_x86_arm
│       link.exe
│       mspdb140.dll
│
├───14.0.23506.0_x86_x64
│       link.exe
│       mspdb140.dll
│
├───14.14.26433.0_x64
│       link.exe
│       mspdbcore.dll
│
├───14.14.26433.0_x64_x86
│       link.exe
│       mspdbcore.dll
│
├───14.14.26433.0_x86
│       link.exe
│       mspdb140.dll
│
├───14.14.26433.0_x86_x64
│       link.exe
│       mspdb140.dll
│
├───14.15.26726.0_x64
│       link.exe
│       mspdbcore.dll
│
├───14.15.26726.0_x64_x86
│       link.exe
│       mspdbcore.dll
│
├───14.15.26726.0_x86
│       link.exe
│       mspdb140.dll
│
└───14.15.26726.0_x86_x64
        link.exe
        mspdb140.dll
```
