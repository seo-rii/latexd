import gzip
import importlib.util
import io
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location(
    "fetch_arxiv_cc0_corpus", Path(__file__).resolve().parents[1] / "fetch_arxiv_cc0_corpus.py"
)
fetch = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(fetch)


class SourceArchiveTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.archive = self.root / "source.tar"
        self.project = self.root / "project"

    def make_archive(self, entries):
        with tarfile.open(self.archive, "w") as archive:
            for name, kind, payload in entries:
                entry = tarfile.TarInfo(name)
                entry.type = kind
                if kind == tarfile.REGTYPE:
                    entry.size = len(payload)
                    archive.addfile(entry, io.BytesIO(payload))
                else:
                    entry.linkname = payload.decode()
                    archive.addfile(entry)

    def test_regular_multifile_project_preserves_bytes(self):
        self.make_archive([("main.tex", tarfile.REGTYPE, b"\\input{parts/body}"),
                           ("parts/body.tex", tarfile.REGTYPE, b"body")])
        fetch.extract_source(self.archive, self.project, "main.tex", False)
        self.assertEqual((self.project / "parts/body.tex").read_bytes(), b"body")

    def test_links_devices_traversal_and_duplicates_never_publish(self):
        for name, kind, payload in [("../escape", tarfile.REGTYPE, b"x"),
                                    ("/absolute", tarfile.REGTYPE, b"x"),
                                    ("link", tarfile.SYMTYPE, b"../outside"),
                                    ("link", tarfile.LNKTYPE, b"main.tex"),
                                    ("device", tarfile.CHRTYPE, b""),
                                    ("main.tex", tarfile.REGTYPE, b"second")]:
            with self.subTest(name=name, kind=kind):
                self.make_archive([("main.tex", tarfile.REGTYPE, b"first"), (name, kind, payload)])
                with self.assertRaises(ValueError):
                    fetch.extract_source(self.archive, self.project, "main.tex", False)
                self.assertFalse(self.project.exists())
                self.assertFalse((self.root / "escape").exists())

    def test_budgets_reject_before_replacing_existing_project(self):
        self.project.mkdir()
        (self.project / "main.tex").write_bytes(b"last good")
        self.make_archive([("main.tex", tarfile.REGTYPE, b"12345")])
        with patch.object(fetch, "MAX_SOURCE_FILE_BYTES", 4):
            with self.assertRaises(ValueError):
                fetch.extract_source(self.archive, self.project, "main.tex", True)
        self.assertEqual((self.project / "main.tex").read_bytes(), b"last good")
        with patch.object(fetch, "MAX_SOURCE_MEMBERS", 0):
            with self.assertRaises(ValueError):
                fetch.extract_source(self.archive, self.project, "main.tex", True)

    def test_gzip_source_has_bounded_decompression(self):
        self.archive.write_bytes(gzip.compress(b"x" * 32))
        with patch.object(fetch, "MAX_SOURCE_FILE_BYTES", 16):
            with self.assertRaises(ValueError):
                fetch.extract_source(self.archive, self.project, "../main.tex", False)
        self.assertFalse(self.project.exists())
        fetch.extract_source(self.archive, self.project, "../main.tex", False)
        self.assertEqual((self.project / "main.tex").read_bytes(), b"x" * 32)

    def test_hidden_pax_metadata_counts_toward_decompression_budget(self):
        with tarfile.open(self.archive, "w:gz", format=tarfile.PAX_FORMAT) as archive:
            entry = tarfile.TarInfo("main.tex")
            entry.pax_headers = {"comment": "x" * 100_000}
            entry.size = 1
            archive.addfile(entry, io.BytesIO(b"x"))
        with patch.object(fetch, "MAX_SOURCE_BYTES", 16):
            with self.assertRaisesRegex(ValueError, "decoded source archive"):
                fetch.extract_source(self.archive, self.project, "main.tex", False)
        self.assertFalse(self.project.exists())
