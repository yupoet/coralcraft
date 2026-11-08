# Windows 7 — support dropped (operator decision, 2026-11-08)

CoralCraft does **not** support Windows 7. The pre-sync experimental build
(`packaging/windows/windows7.ps1`, `apps/coralcraft/src/printing/windows7.rs`) was removed after
the VectorCraft `36ef34df` sync made it unbuildable: upstream removed its Windows 7 support, and
its dependency set no longer resolves through the patched vendored `windows-link` the build
applied (`patch location vendor/windows-link does not contain packages matching windows-link`,
run 42435320094). The operator decided on 2026-11-08 to drop the platform rather than rebuild a
Windows 7-compatible dependency arrangement; CoralCraft keeps the minimum platform set covered by
CI (macOS, Linux, Windows, FreeBSD, Windows ARM64, wasm32).

No release, package, or document of CoralCraft claims Windows 7 support.
