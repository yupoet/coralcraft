# Windows 7 compatibility build

> **Status after the VectorCraft 36ef34d sync (2026-11-08):** upstream removed its Windows 7
> support, and its dependency set no longer resolves through the vendored `windows-link` the
> Windows 7 build patches — the CI job fails at `patch location vendor/windows-link does not
> contain packages matching windows-link` (run 42435320094). The Windows 7 code
> (`apps/coralcraft/src/printing/windows7.rs`, `packaging/windows/windows7.ps1`) is retained in
> the tree pending an operator decision: either a Windows 7 compatible dependency arrangement is
> rebuilt, or the support is dropped explicitly. Until one of those happens, no CoralCraft
> release or document claims Windows 7 support, and the `windows7` workflow is not run.


CoralCraft can build an **experimental, unsupported** portable package for Windows 7 SP1 x64. The package is not a supported release and does not promise that every feature works on Windows 7.

The build requires an OpenGL 3.3-capable driver on the Windows 7 machine. It uses the patched `vendor/windows-link` crate and rebuilds the Rust standard library for the `x86_64-win7-windows-msvc` target. Other CoralCraft builds continue to use the normal crates.io dependency.

## Build and host checks

Run on a Windows 10/11 build host with the VS 2022 x64 C++ tools and Windows SDK:

```powershell
pwsh packaging/windows/windows7.ps1 -Test
```

The script builds `dist/release/coralcraft-windows7-x64-portable.zip`, checks the PE architecture and subsystem versions, rejects known post-Windows-7 imports, and runs the application and CLI tests on the build host. These host checks do not prove Windows 7 runtime compatibility.

## Windows 7 runtime checklist

Before making any compatibility claim, copy the portable package to an actual Windows 7 SP1 x64 machine and verify all of the following:

1. `coralcraft-cli.exe --version` and `coralcraft.exe --version` run without missing-DLL or missing-API errors.
2. The desktop window opens with the OpenGL 3.3 driver, and the canvas redraws while creating and editing a document.
3. A synthetic document can be saved, closed, reopened, and exported without losing its objects.
4. Portable mode keeps settings and recovery data beside the application as described by `portable.txt`.
5. The application exits cleanly and does not leave either executable running.

Record the exact package commit, Windows 7 build, graphics driver, and any failure. Do not mark the build supported based only on the modern-host CI job.
