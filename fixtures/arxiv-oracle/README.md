# arXiv Oracle Corpus

This directory contains small manifests for opt-in arXiv oracle comparisons.
It intentionally does not contain downloaded arXiv source packages or PDFs.

Fetch the local corpus outside the repository:

```bash
python3 scripts/fetch_arxiv_cc0_corpus.py \
  --output /home/seorii/dev/_local/latexd-arxiv-cc0
```

Run the ignored comparison test:

```bash
LATEXD_ARXIV_CC0_CORPUS=/home/seorii/dev/_local/latexd-arxiv-cc0 \
  cargo test -p latexd --test arxiv_oracle -- --ignored
```

The test compares text extracted from the official arXiv PDF against text
extracted from the internal `latexd` PDF and writes a JSON report into the
local corpus report directory. Set `LATEXD_ARXIV_ORACLE_STRICT=1` to make
internal build failures or low overlap fail the test.
