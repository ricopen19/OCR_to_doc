from __future__ import annotations

from pathlib import Path
import unittest

try:
    from PIL import Image
except ImportError:  # pragma: no cover - Pillow not installed in minimal env
    Image = None

try:  # pragma: no cover - guard for environments without Pillow
    from image_preprocessor import PROFILE_REGISTRY, preprocess_image_variants
    PREPROCESS_IMPORT_ERROR = None
except ModuleNotFoundError as exc:  # pragma: no cover
    PROFILE_REGISTRY = {}
    preprocess_image_variants = None  # type: ignore
    PREPROCESS_IMPORT_ERROR = exc


@unittest.skipIf(Image is None or PREPROCESS_IMPORT_ERROR is not None, "Pillow が必要です")
class ImagePreprocessorTests(unittest.TestCase):
    def test_preprocess_image_variants_outputs_grayscale_and_color(self) -> None:
        from tempfile import TemporaryDirectory
        with TemporaryDirectory() as tmpdir:
            tmp_path = Path(tmpdir)
            source = tmp_path / "input.png"
            Image.new("RGB", (1200, 900), color=(240, 240, 240)).save(source)

            profiles = [
                PROFILE_REGISTRY["ocr_default"],
                PROFILE_REGISTRY["presentation_color"],
            ]

            variants = preprocess_image_variants(
                source, tmp_path / "out", profiles=profiles, page_number=5
            )

            ocr_path = variants["ocr_default"]
            pres_path = variants["presentation_color"]

            self.assertTrue(ocr_path.exists())
            self.assertTrue(pres_path.exists())

            with Image.open(ocr_path) as img:
                self.assertEqual(img.mode, "L")
                self.assertIn("page_005", ocr_path.name)

            with Image.open(pres_path) as img:
                self.assertEqual(img.mode, "RGB")
                self.assertIn("page_005", pres_path.name)
