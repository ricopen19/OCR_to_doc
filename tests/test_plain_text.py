import unittest

from plain_text import to_plain_text


class PlainTextTests(unittest.TestCase):
    def test_image_is_kept_as_label(self) -> None:
        md = "before ![](./figures/a.png) after"
        self.assertEqual(to_plain_text(md), "before [画像: ./figures/a.png] after")

    def test_html_img_is_kept_as_label(self) -> None:
        md = 'x <img src="./figures/a.png" width="10"> y'
        self.assertEqual(to_plain_text(md), "x [画像: ./figures/a.png] y")

    def test_table_is_converted_to_tsv(self) -> None:
        md = "|A|B|\n|---|---|\n|1|2|\n"
        self.assertEqual(to_plain_text(md), "A\tB\n1\t2")

    def test_inline_math_delimiters_are_stripped(self) -> None:
        md = "数学$mathematics$ と $x+y$"
        self.assertEqual(to_plain_text(md), "数学mathematics と x+y")


if __name__ == "__main__":
    unittest.main()

