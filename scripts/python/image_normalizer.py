"""Utility helpers for normalizing user-provided images.

Supports HEIC/HEIF → PNG and SVG → PNG conversions so that downstream
OCR steps can treat every asset as a regular raster image.
"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path


HEIC_EXTENSIONS = {".heic", ".heif"}
SVG_EXTENSIONS = {".svg"}
CONVERTIBLE_EXTENSIONS = HEIC_EXTENSIONS | SVG_EXTENSIONS


class ImageConversionError(RuntimeError):
    """Raised when we fail to convert an input image to PNG."""


@dataclass(frozen=True)
class ImageConversionResult:
    source: Path
    converted: Path
    performed: bool


def requires_conversion(path: Path) -> bool:
    return path.suffix.lower() in CONVERTIBLE_EXTENSIONS


def ensure_png_image(
    source: Path,
    *,
    convert_dir: Path | None = None,
    svg_dpi: int = 300,
    overwrite: bool = False,
) -> ImageConversionResult:
    """Convert HEIC/HEIF/SVG images into PNG.

    Returns an ``ImageConversionResult`` that tells the caller where the
    rasterized image lives and whether a conversion was actually performed.
    """

    source = Path(source)
    suffix = source.suffix.lower()
    if suffix not in CONVERTIBLE_EXTENSIONS:
        return ImageConversionResult(source=source, converted=source, performed=False)

    if convert_dir is None:
        convert_dir = source.parent
    convert_dir = Path(convert_dir)
    convert_dir.mkdir(parents=True, exist_ok=True)

    target = convert_dir / f"{source.stem}.png"
    if target.exists() and not overwrite:
        return ImageConversionResult(source=source, converted=target, performed=False)

    if suffix in HEIC_EXTENSIONS:
        _convert_heic_to_png(source, target)
    elif suffix in SVG_EXTENSIONS:
        _convert_svg_to_png(source, target, dpi=svg_dpi)
    else:  # pragma: no cover - safety net for future extensions
        raise ImageConversionError(f"未対応の変換形式です: {suffix}")

    return ImageConversionResult(source=source, converted=target, performed=True)


def _convert_heic_to_png(source: Path, target: Path) -> None:
    try:
        from pillow_heif import register_heif_opener
    except ImportError as exc:  # pragma: no cover - dependency missing is fatal
        raise ImageConversionError("pillow-heif がインストールされていません") from exc

    from PIL import Image

    register_heif_opener()

    try:
        with Image.open(source) as image:
            image.load()
            image.save(target, format="PNG")
    except Exception as exc:  # pragma: no cover - pillow_heif errors depend on file contents
        raise ImageConversionError(f"HEIC 変換に失敗しました: {exc}") from exc


def _convert_svg_to_png(source: Path, target: Path, *, dpi: int = 300) -> None:
    try:
        import cairosvg
    except ImportError as exc:  # pragma: no cover - dependency missing is fatal
        raise ImageConversionError("cairosvg がインストールされていません") from exc

    try:
        cairosvg.svg2png(url=str(source), write_to=str(target), dpi=dpi)
    except Exception as exc:  # pragma: no cover - cairosvg errors depend on SVG content
        raise ImageConversionError(f"SVG 変換に失敗しました: {exc}") from exc


__all__ = [
    "HEIC_EXTENSIONS",
    "SVG_EXTENSIONS",
    "CONVERTIBLE_EXTENSIONS",
    "ImageConversionError",
    "ImageConversionResult",
    "requires_conversion",
    "ensure_png_image",
]
