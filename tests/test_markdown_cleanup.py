import tempfile
import unittest
from pathlib import Path

from markdown_cleanup import clean_text, clean_file


class MarkdownCleanupTests(unittest.TestCase):
    def test_unit_wrapping(self) -> None:
        line = "10 × 10^3 回/秒 × 16ビット/回"
        cleaned = clean_text(line)
        self.assertIn("回/秒", cleaned)
        self.assertIn("ビット/回", cleaned)
        self.assertNotIn("\\text{", cleaned)
        self.assertNotIn("$", cleaned)

    def test_nested_block_dollars(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            md_path = Path(tmpdir) / "input.md"
            md_path.write_text("$$ $n+11-a$ $$\n2", encoding="utf-8")
            clean_file(md_path, inplace=True)
            cleaned = md_path.read_text(encoding="utf-8")
            self.assertIn("n+11-a", cleaned)
            self.assertNotIn("$$", cleaned)
            self.assertNotIn("$n+11-a$", cleaned)

    def test_log_base_two(self) -> None:
        line = "$log^{2} n$"
        cleaned = clean_text(line)
        self.assertIn("log_2 n", cleaned)
        self.assertNotIn("$", cleaned)

    def test_ocr_br_marker_is_normalized(self) -> None:
        line = "a<<<br>>>b"
        cleaned = clean_text(line)
        self.assertEqual(cleaned, "a<br>b")

    def test_ocr_br_marker_fullwidth_is_normalized(self) -> None:
        line = "a＜br＞b"
        cleaned = clean_text(line)
        self.assertEqual(cleaned, "a<br>b")

    def test_br_tag_is_not_duplicated(self) -> None:
        line = "a<br>b"
        cleaned = clean_text(line)
        self.assertEqual(cleaned, "a<br>b")

    def test_fraction_with_slash_is_inlined(self) -> None:
        line = "23年度<br>/88,000<br>185,300<br>×100=3.2%"
        cleaned = clean_text(line)
        self.assertIn("185,300/88,000", cleaned)
        self.assertNotIn("/88,000<br>185,300", cleaned)

    def test_fraction_with_text_labels_is_inlined(self) -> None:
        line = "後の年度の数値<br>前の年度の数値<br>-1)×100% で確認する。"
        cleaned = clean_text(line)
        self.assertIn("後の年度の数値/前の年度の数値-1)×100%", cleaned)

    def test_slash_and_bar_are_fixed_in_numbered_block(self) -> None:
        content = "\n".join(
            [
                "/",
                r"\| 東京都の燃料消費量は、平成22年及び23年はいずれも前年に比べて減少している。",
                "2 埼玉県の電力消費量の対前年増加率は、平成22年及び23年はいずれも5%を上回っている。",
                "3 平成21年から23年までのうち、神奈川県の燃料消費量が最も多いのは23年である。",
            ]
        )
        with tempfile.TemporaryDirectory() as tmpdir:
            md_path = Path(tmpdir) / "input.md"
            md_path.write_text(content, encoding="utf-8")
            clean_file(md_path, inplace=True)
            cleaned = md_path.read_text(encoding="utf-8")
            self.assertIn("1 東京都の燃料消費量は、平成22年及び23年はいずれも前年に比べて減少している。", cleaned)
            self.assertNotIn("\n/\n", cleaned)

    def test_letter_slash_prefix_formula_is_normalized(self) -> None:
        line = "A / 2x+1"
        cleaned = clean_text(line)
        self.assertEqual(cleaned, "A: 2x+1")

    def test_letter_slash_prefix_text_is_preserved(self) -> None:
        line = "A / りんご"
        cleaned = clean_text(line)
        self.assertEqual(cleaned, "A / りんご")

    def test_trailing_backslash_is_removed(self) -> None:
        line = "line\\"
        cleaned = clean_text(line)
        self.assertEqual(cleaned, "line")

    def test_escaped_punctuation_is_unescaped(self) -> None:
        line = r"note\! and wave\~"
        cleaned = clean_text(line)
        self.assertEqual(cleaned, "note! and wave~")

    def test_leading_dot_is_converted_to_bullet(self) -> None:
        line = "。いつもどおりに...頑張っていく!"
        cleaned = clean_text(line)
        self.assertEqual(cleaned, "- いつもどおりに...頑張っていく!")

    def test_repair_broken_table_rows_with_br(self) -> None:
        content = """# Page 1
## 詳細情報
|区間|道路|距離と時間|値段円:割引料金詳細|
|-|-|-|-|
|御殿場
↓
浜松|東名高速道路|146.3km
95分|通常料金:3740円
ETC料金:3740円
ETC2.0料金:3740円
- 深夜割引0-4時/30%:2620円
- 休日割引:2620円|
ルート3
"""
        expected_row = (
            "|御殿場<br>↓<br>浜松|東名高速道路|146.3km<br>95分|"
            "通常料金:3740円<br>ETC料金:3740円<br>ETC2.0料金:3740円<br>"
            "- 深夜割引0-4時/30%:2620円<br>- 休日割引:2620円|"
        )
        with tempfile.TemporaryDirectory() as tmpdir:
            md_path = Path(tmpdir) / "input.md"
            md_path.write_text(content, encoding="utf-8")
            clean_file(md_path, inplace=True)
            cleaned = md_path.read_text(encoding="utf-8")
            self.assertIn(expected_row, cleaned)
            self.assertIn("\nルート3", cleaned)


if __name__ == "__main__":
    unittest.main()
