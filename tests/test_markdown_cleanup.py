import tempfile
import unittest
from pathlib import Path

from markdown_cleanup import clean_text, clean_file


class MarkdownCleanupTests(unittest.TestCase):
    def test_unit_wrapping(self) -> None:
        line = "10 × 10^3 回/秒 × 16ビット/回"
        cleaned = clean_text(line)
        self.assertIn("\\text{回/秒}", cleaned)
        self.assertIn("\\text{ビット/回}", cleaned)

    def test_nested_block_dollars(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            md_path = Path(tmpdir) / "input.md"
            md_path.write_text("$$ $n+11-a$ $$\n2", encoding="utf-8")
            clean_file(md_path, inplace=True)
            cleaned = md_path.read_text(encoding="utf-8")
            self.assertIn("$$ n+11-a $$", cleaned)
            self.assertNotIn("$$ $n+11-a$ $$", cleaned)
            self.assertNotIn("\n2", cleaned)

    def test_log_base_two(self) -> None:
        line = "$log^{2} n$"
        cleaned = clean_text(line)
        self.assertIn("\\log_2 n", cleaned)


if __name__ == "__main__":
    unittest.main()
