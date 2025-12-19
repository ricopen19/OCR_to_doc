"""Utilities for image preprocessing prior to OCR / presentation export."""

from __future__ import annotations

from dataclasses import dataclass, replace
from pathlib import Path
from typing import Iterable, Mapping

from PIL import Image, ImageEnhance, ImageFilter, ImageOps, ImageStat
import numpy as np
import argparse

from image_normalizer import ensure_png_image, requires_conversion, ImageConversionError


@dataclass(frozen=True)
class ImagePreprocessProfile:
    """Configuration describing how a single variant should be generated."""

    key: str
    description: str
    target_long_edge: int = 2400
    grayscale: bool = True
    keep_color: bool = False
    contrast: float = 1.0
    brightness: float = 1.0
    sharpness: float = 1.0
    denoise_size: int = 0
    binarize: bool = False
    unsharp_radius: float = 1.5
    unsharp_percent: int = 120
    unsharp_threshold: int = 3
    gamma: float | None = None  # 追加: ガンマ補正 (None なら未適用)
    clahe: bool = False         # 追加: 局所コントラスト補正 (CLAHE)


def resize_long_edge(image: Image.Image, target: int) -> Image.Image:
    if target <= 0:
        return image
    width, height = image.size
    long_edge = max(width, height)
    if long_edge == target:
        return image
    scale = target / long_edge
    new_size = (int(width * scale), int(height * scale))
    return image.resize(new_size, Image.LANCZOS)


def _ensure_mode(image: Image.Image, *, grayscale: bool, keep_color: bool) -> Image.Image:
    if grayscale and not keep_color:
        return image.convert("L")
    return image.convert("RGB")


def _apply_enhancements(image: Image.Image, profile: ImagePreprocessProfile) -> Image.Image:
    if profile.brightness != 1.0:
        image = ImageEnhance.Brightness(image).enhance(profile.brightness)
    if profile.contrast != 1.0:
        image = ImageEnhance.Contrast(image).enhance(profile.contrast)
    if profile.gamma and profile.gamma > 0:
        gamma = profile.gamma
        lut = [min(255, int((i / 255.0) ** (1.0 / gamma) * 255 + 0.5)) for i in range(256)]
        image = image.point(lut * (3 if image.mode == "RGB" else 1))
    if profile.clahe:
        # 簡易 CLAHE: OpenCV を使わず PIL+numpy でチャネルごとに適用
        image = _apply_clahe(image)
    if profile.denoise_size and profile.denoise_size >= 3:
        image = image.filter(ImageFilter.MedianFilter(size=profile.denoise_size))
    if profile.sharpness != 1.0:
        image = ImageEnhance.Sharpness(image).enhance(profile.sharpness)
    else:
        image = image.filter(
            ImageFilter.UnsharpMask(
                radius=profile.unsharp_radius,
                percent=profile.unsharp_percent,
                threshold=profile.unsharp_threshold,
            )
        )
    return image


def _binarize(image: Image.Image) -> Image.Image:
    gray = image.convert("L") if image.mode != "L" else image
    stats = ImageStat.Stat(gray)
    mid = stats.median[0]
    threshold = max(110, min(200, int(mid + 10)))
    return gray.point(lambda x: 255 if x > threshold else 0, mode="1").convert("L")


def _apply_clahe(image: Image.Image, clip_limit: float = 2.0, grid_size: int = 8) -> Image.Image:
    """Apply a simple CLAHE-like enhancement without OpenCV."""
    img = image.convert("L")
    arr = np.asarray(img, dtype=np.uint8)
    h, w = arr.shape
    tile_h = max(1, h // grid_size)
    tile_w = max(1, w // grid_size)
    out = np.zeros_like(arr)
    for y in range(0, h, tile_h):
        for x in range(0, w, tile_w):
            tile = arr[y : y + tile_h, x : x + tile_w]
            hist, _ = np.histogram(tile.flatten(), bins=256, range=(0, 255))
            cdf = hist.cumsum().astype(np.float64)
            # CLAHE 風にヒストグラムをクリップ
            clip_val = cdf[-1] * clip_limit / grid_size
            cdf = np.clip(cdf, 0, clip_val)
            # 正規化
            cdf = (cdf - cdf.min()) / max(1.0, (cdf.max() - cdf.min())) * 255.0
            lut = np.floor(cdf + 0.5).astype(np.uint8)
            out[y : y + tile_h, x : x + tile_w] = lut[tile]
    return Image.fromarray(out, mode="L")


def preprocess_image_variants(
    source: Path,
    output_dir: Path,
    *,
    profiles: Iterable[ImagePreprocessProfile],
    page_number: int = 1,
) -> Mapping[str, Path]:
    output_dir = Path(output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)

    profile_list = list(profiles)
    if not profile_list:
        raise ValueError("profiles には少なくとも 1 要素が必要です")

    max_long_edge = max((p.target_long_edge for p in profile_list if p.target_long_edge), default=0)

    with Image.open(source) as base:
        base = ImageOps.exif_transpose(base)
        if max_long_edge:
            base = resize_long_edge(base, max_long_edge)
        variants: dict[str, Path] = {}
        for profile in profile_list:
            image = base.copy()
            image = _ensure_mode(image, grayscale=profile.grayscale, keep_color=profile.keep_color)
            image = _apply_enhancements(image, profile)
            if profile.binarize:
                image = _binarize(image)
            variant_dir = output_dir / profile.key
            variant_dir.mkdir(parents=True, exist_ok=True)
            variant_path = variant_dir / f"page_{page_number:03d}.png"
            image.save(variant_path, format="PNG", optimize=True)
            variants[profile.key] = variant_path

    return variants


OCR_DEFAULT_PROFILE = ImagePreprocessProfile(
    key="ocr_default",
    description="高コントラスト・グレースケールで OCR 用に最適化",
    target_long_edge=2600,
    grayscale=True,
    contrast=1.35,
    brightness=1.05,
    sharpness=1.15,
    denoise_size=3,
    binarize=True,
)

PRESENTATION_COLOR_PROFILE = ImagePreprocessProfile(
    key="presentation_color",
    description="色を保持しつつ軽くノイズ除去（将来の PowerPoint 出力向け）",
    target_long_edge=2600,
    grayscale=False,
    keep_color=True,
    contrast=1.1,
    brightness=1.0,
    sharpness=1.05,
    denoise_size=3,
    binarize=False,
)

# 黒板・低コントラスト手書き向けプロファイル
CHALKBOARD_STRONG_PROFILE = ImagePreprocessProfile(
    key="chalkboard_strong",
    description="黒板や薄いチョーク文字向けに高コントラスト + 二値化 + 強めデノイズ",
    target_long_edge=2800,
    grayscale=True,
    contrast=2.2,
    brightness=1.0,
    sharpness=1.2,
    denoise_size=5,
    binarize=True,
    gamma=0.7,
    clahe=True,
)


PROFILE_REGISTRY: dict[str, ImagePreprocessProfile] = {
    OCR_DEFAULT_PROFILE.key: OCR_DEFAULT_PROFILE,
    PRESENTATION_COLOR_PROFILE.key: PRESENTATION_COLOR_PROFILE,
    CHALKBOARD_STRONG_PROFILE.key: CHALKBOARD_STRONG_PROFILE,
}


def get_profile(key: str) -> ImagePreprocessProfile:
    if key not in PROFILE_REGISTRY:
        raise KeyError(f"未知の前処理プロファイルです: {key}")
    return PROFILE_REGISTRY[key]


__all__ = [
    "ImagePreprocessProfile",
    "PROFILE_REGISTRY",
    "OCR_DEFAULT_PROFILE",
    "PRESENTATION_COLOR_PROFILE",
    "preprocess_image_variants",
    "get_profile",
]


def _run_cli() -> None:
    parser = argparse.ArgumentParser(description="単一画像を前処理して保存する簡易CLI")
    parser.add_argument("input", type=Path, help="入力画像（HEIC/PNG/JPGなど）")
    parser.add_argument("--output", type=Path, required=True, help="出力パス (png)")
    parser.add_argument("--profile", default="ocr_default", help="PROFILE_REGISTRY のキー")
    parser.add_argument("--target-long-edge", type=int, help="長辺のリサイズ先(px)")
    parser.add_argument("--contrast", type=float, help="コントラスト倍率")
    parser.add_argument("--brightness", type=float, help="明るさ倍率")
    parser.add_argument("--sharpness", type=float, help="シャープネス倍率")
    parser.add_argument("--binarize", action=argparse.BooleanOptionalAction, default=None, help="二値化するか")
    parser.add_argument("--denoise-size", type=int, help="メディアンフィルタサイズ(奇数推奨)")
    parser.add_argument("--denoise-strong", action=argparse.BooleanOptionalAction, default=False, help="強めのデノイズ(サイズ5)")
    parser.add_argument("--keep-color", action=argparse.BooleanOptionalAction, default=None, help="カラーを保持するか")
    parser.add_argument("--no-grayscale", action="store_true", help="グレースケールを無効化（カラー維持）")
    parser.add_argument("--page-number", type=int, default=1, help="保存時のページ番号（ファイル名用）")
    parser.add_argument("--deskew", action=argparse.BooleanOptionalAction, default=False, help="(未実装) ダミー受け口。指定されても無視します。")
    args = parser.parse_args()

    if not args.input.exists():
        raise SystemExit(f"入力が見つかりません: {args.input}")

    base_profile = get_profile(args.profile)
    overrides = {}
    if args.target_long_edge is not None:
        overrides["target_long_edge"] = args.target_long_edge
    if args.contrast is not None:
        overrides["contrast"] = args.contrast
    if args.brightness is not None:
        overrides["brightness"] = args.brightness
    if args.sharpness is not None:
        overrides["sharpness"] = args.sharpness
    if args.denoise_size is not None:
        overrides["denoise_size"] = args.denoise_size
    if args.denoise_strong:
        overrides["denoise_size"] = 5
    if args.binarize is not None:
        overrides["binarize"] = args.binarize
    if args.keep_color is not None:
        overrides["keep_color"] = args.keep_color
    if args.no_grayscale:
        overrides["grayscale"] = False

    profile = replace(base_profile, **overrides) if overrides else base_profile

    source_path = args.input
    if requires_conversion(source_path):
        try:
            converted = ensure_png_image(
                source_path,
                convert_dir=args.output.parent,
                overwrite=True,
            )
            source_path = converted.converted
        except ImageConversionError as exc:
            raise SystemExit(f"画像変換に失敗しました: {exc}") from exc

    variants = preprocess_image_variants(
        source_path,
        args.output.parent,
        profiles=[profile],
        page_number=args.page_number,
    )
    output_path = variants[profile.key]
    if output_path != args.output:
        # rename to requested path
        output_path.rename(args.output)
        output_path = args.output
    print(f"[image_preprocessor] saved: {output_path}")


if __name__ == "__main__":
    _run_cli()
