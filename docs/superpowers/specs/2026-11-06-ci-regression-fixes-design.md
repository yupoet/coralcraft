# CI Regression Fixes Design

## Goal

Restore the previously identified CI jobs without weakening lint, test, package-install, or architecture checks.

## Approach

Use targeted root-cause fixes on current `main`:

1. Preserve the pattern-swatch boolean logic while writing it in the minimal form required by Clippy.
2. Resolve Paste in Front / Paste in Back and Draw Behind / Inside anchors through `selected_roots`, so a selected compound member refers to its compound and new art stays outside it.
3. Keep the oversized native-area test on 32-bit targets by writing explicit `u64` inputs into serialized JSON before opening it, exercising the `u64 > usize::MAX` deserialization branch.
4. Install nfpm with Go at a fixed release, build the Linux package with cargo-zigbuild for glibc 2.31, and install it with its dependencies on Debian 11.
5. Pass the current `CoralCraft` installation directory to the ARM64 package smoke test and guard the current app identity in package metadata.
6. Add the Windows 7 documentation that the portable package and workflow already reference.

The alternatives of fixing only the local Clippy failure, disabling checks, or skipping package smoke tests do not satisfy the requested scope.

## Verification

Add a deterministic regression test for pasting in front of and behind a selected compound member. Run the existing engine tests, Clippy with warnings denied, the complete `cargo xtask ci` chain, and the merged branch's platform CI. The final result must not claim success while any of these checks remains red.
