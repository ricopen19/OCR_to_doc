from __future__ import annotations

import tempfile
import unittest
from pathlib import Path
from unittest import mock

import image_normalizer as normalizer

try:  # pragma: no cover - only for optional dependency detection
    import cairosvg  # noqa: F401
except ImportError:  # pragma: no cover - handled via skip decorator
    CAIRO_AVAILABLE = False
else:  # pragma: no cover - marker for readability only
    CAIRO_AVAILABLE = True


class ImageNormalizerTests(unittest.TestCase):
    def test_requires_conversion_flags_known_extensions(self) -> None:
        self.assertTrue(normalizer.requires_conversion(Path("photo.heic")))
        self.assertTrue(normalizer.requires_conversion(Path("vector.svg")))
        self.assertFalse(normalizer.requires_conversion(Path("note.png")))

    def test_ensure_png_image_skips_native_png(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            png_path = Path(tmpdir) / "sample.png"
            png_path.write_bytes(b"data")

            result = normalizer.ensure_png_image(png_path)

            self.assertEqual(result.converted, png_path)
            self.assertFalse(result.performed)

    @unittest.skipUnless(CAIRO_AVAILABLE, "cairosvg がインストールされていません")
    def test_ensure_png_image_converts_svg(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            svg_path = Path(tmpdir) / "diagram.svg"
            svg_path.write_text(
                """<svg width='10' height='10' xmlns='http://www.w3.org/2000/svg'>
                <rect width='10' height='10' fill='black'/>
                </svg>""",
                encoding="utf-8",
            )

            convert_dir = Path(tmpdir) / "converted"
            result = normalizer.ensure_png_image(svg_path, convert_dir=convert_dir, svg_dpi=72, overwrite=True)

            self.assertTrue(result.performed)
            self.assertEqual(result.converted.suffix, ".png")
            self.assertTrue(result.converted.exists())
            self.assertGreater(result.converted.stat().st_size, 0)

    def test_ensure_png_image_uses_heic_converter(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            heic_path = Path(tmpdir) / "shot.heic"
            heic_path.write_bytes(b"binary")

            called: dict[str, tuple[Path, Path]] = {}

            def fake_converter(source: Path, target: Path) -> None:
                called["args"] = (source, target)
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(b"png")

            with mock.patch.object(normalizer, "_convert_heic_to_png", side_effect=fake_converter):
                result = normalizer.ensure_png_image(
                    heic_path,
                    convert_dir=Path(tmpdir) / "converted",
                    overwrite=True,
                )

            self.assertIn("args", called)
            self.assertEqual(result.converted.read_bytes(), b"png")
            self.assertTrue(result.performed)


if __name__ == "__main__":  # pragma: no cover
    unittest.main()
