# Third-Party Source Setup

This directory tracks small metadata files only. Do not commit downloaded
upstream archives, extracted MuPDF source trees, or local build outputs.

Run the setup helper from the repository root:

```sh
sh scripts/setup-mupdf.sh
. third_party/mupdf.env
```

On Windows, use the PowerShell helper from a VS x64 Developer shell:

```powershell
.\scripts\setup-mupdf.ps1
. .\third_party\mupdf.env.ps1
```

`mupdf.version` pins the upstream MuPDF source archive used by the
`real-mupdf` build.
