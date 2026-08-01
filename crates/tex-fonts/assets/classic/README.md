# Classic TeX Font Bundle

This directory contains an unmodified, minimal Computer Modern runtime bundle
for hermetic native and WASI rendering. It is separate third-party font
software and is **not** relicensed under latexd's MIT license.

## Sources

- Type 1 outlines: AMSFonts 3.04 from
  <https://mirrors.ctan.org/fonts/amsfonts.zip>.
- TFM metrics: CTAN `cm-tfm`, release noted by CTAN as 2022-12-23, from
  <https://mirrors.ctan.org/fonts/cm/tfm.zip>.

The archive and per-file SHA-256 values are recorded in `manifest.json`. The
vendored files were also checked byte-for-byte against the corresponding TeX
Live files on the development host before inclusion.

## Licenses

- The AMS Type 1 files are Copyright 1997, 2009 American Mathematical Society
  and licensed under SIL Open Font License 1.1. Their embedded notices and
  Reserved Font Names are unchanged. See `licenses/OFL.txt` and the upstream
  `licenses/README`.
- The official Computer Modern TFM files use the Knuth License. They are
  unmodified and keep their standard names. See `licenses/KNUTH.txt`.

Do not edit the binary files in place. A modified font must follow its source
license's renaming requirements and use a new bundle identity and manifest.
