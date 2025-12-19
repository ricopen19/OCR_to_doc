import unittest
from pathlib import Path

from docx import Document

from export_docx import convert_markdown, split_inline_math_segments


class InlineMathSplitTests(unittest.TestCase):
    def test_basic_inline_detection(self) -> None:
        text = "A $x+y$ B"
        self.assertEqual(
            split_inline_math_segments(text),
            [
                ("text", "A "),
                ("math", "x+y"),
                ("text", " B"),
            ],
        )

    def test_escaped_dollars_inside_and_outside_math(self) -> None:
        text = r"Cost $\text{Cost \$5}$ more and \$ outside"
        self.assertEqual(
            split_inline_math_segments(text),
            [
                ("text", "Cost "),
                ("math", r"\text{Cost \$5}"),
                ("text", r" more and \$ outside"),
            ],
        )

    def test_double_dollars_remain_literal(self) -> None:
        text = "prefix $$ block $$ suffix"
        self.assertEqual(
            split_inline_math_segments(text),
            [
                ("text", "prefix $$ block $$ suffix"),
            ],
        )

    def test_unclosed_inline_math_is_kept_as_text(self) -> None:
        text = "Value $unfinished"
        self.assertEqual(
            split_inline_math_segments(text),
            [
                ("text", "Value $unfinished"),
            ],
        )

    def test_list_item_inline_math_rendered(self) -> None:
        doc = Document()
        lines = ["- 生徒数の比 $A:B=3:2$ より"]
        convert_markdown(doc, lines, base_dir=Path("."))
        full_xml = "\n".join(p._p.xml for p in doc.paragraphs)
        self.assertIn("m:oMath", full_xml)


if __name__ == "__main__":
    unittest.main()
