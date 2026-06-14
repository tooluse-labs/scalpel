# ADR 0002: MuPDF Source and Link Policy for Milestone 1

Status: Accepted for Milestone 1
Date: 2026-06-05

## Decision

Milestone 1 uses MuPDF 1.27.2 under the AGPL-3.0 license.

The selected upstream source is the open-source MuPDF 1.27.2 release, announced
in the official release history on 2026-02-20. The source archive used for
developer builds is:

```text
https://casper.mupdf.com/downloads/archive/mupdf-1.27.2-source.tar.gz
```

As of the product-preview GUI path, `scalpel-app` enables `gui` plus `real-mupdf`
by default. MuPDF source and build outputs remain outside git; the repository
tracks only `third_party/mupdf.version` and `scripts/setup-mupdf.sh` so local
developer builds use the same pinned upstream archive. The fake shim remains
available for lower-level contract tests and no-MuPDF builds.

For M1 developer builds, the preferred source layout is an extracted pinned
MuPDF source tree under ignored `third_party/` paths or another local directory,
referenced by `SCALPEL_MUPDF_SOURCE_DIR`. A checked-in `third_party/mupdf-1.27.2`
tree may be introduced later only together with regenerated notices and
corresponding-source publication steps.

The M1 link mode is static by default. `scalpel-shim/build.rs` accepts
`SCALPEL_MUPDF_LINK_MODE` and `SCALPEL_MUPDF_LIBS` for local experiments, but the
documented product path is a pinned source release plus MuPDF's static
`libmupdf.a` and `libmupdf-third.a` libraries.

The initial supported M1 platform is macOS for developer integration. Linux is
the next required platform before any release build. Windows remains an explicit
follow-up until the MuPDF build and sanitizer story is validated there.

The binding policy does not change in M1.1: Rust declarations remain checked-in
raw ABI declarations for `pdbg_shim.h`. If generated bindings are introduced in
a later M1 patch, generation must target `pdbg_shim.h` only, never MuPDF
`fz_*` or `pdf_*` headers, and the ABI drift guard must be updated in the same
change.

## Build Contract

Prepare the pinned local MuPDF tree from the repository root:

```sh
sh scripts/setup-mupdf.sh
. third_party/mupdf.env
```

Then the app default build uses real MuPDF:

```sh
cargo run -p scalpel-app -- --gui
cargo test -p scalpel-app
```

If MuPDF was built into a non-default location, set:

```sh
SCALPEL_MUPDF_INCLUDE_DIR=/path/to/mupdf/include
SCALPEL_MUPDF_LIB_DIR=/path/to/mupdf/build/release
```

`SCALPEL_MUPDF_LIBS` defaults to `mupdf,mupdf-third`. Additional platform or
third-party libraries may be added locally while M1.1/M1.2 settles the final
static link line.

The full real gate is:

```sh
sh scripts/run_real_gate.sh
```

No-MuPDF contract checks remain available through explicit no-default-feature
builds:

```sh
sh scripts/run_m0_local_gate.sh
```

GitHub Actions jobs that run real MuPDF should use the pinned source archive,
build the static libraries plus `mutool`, and run the same
`scripts/run_real_gate.sh` contract.

This gate also requires `mutool` for runtime generation of the encrypted-PDF
fixture. By default it expects:

```text
$SCALPEL_MUPDF_SOURCE_DIR/build/release/mutool
```

Build it from the pinned MuPDF source tree with:

```sh
make build=release build/release/mutool
```

or set `SCALPEL_MUTOOL_PATH=/path/to/mutool`.

## Rationale

This keeps the M0 green floor intact while making the real integration path
concrete enough for M1.2. The project gets a reproducible upstream version and
license posture without forcing every contributor or CI job to build MuPDF.

Static linking matches the intended single-binary debugger shape and makes the
AGPL corresponding-source obligation explicit: any distributed real-MuPDF build
must publish the application source, shim source, build scripts, exact MuPDF
source archive, notices, and local patches.

## Upgrade Policy

The MuPDF upgrade owner is the project maintainer for the current milestone.
During active M1 work, check upstream MuPDF security and release notes at least
monthly and before every public binary distribution.

Every MuPDF upgrade must update:

- this ADR or a successor ADR;
- `NOTICES`;
- the real-shim smoke tests;
- malformed and repair fixture results;
- sanitizer and TSan notes for supported platforms.
