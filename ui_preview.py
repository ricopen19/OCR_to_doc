from __future__ import annotations

import argparse
import base64
import io
import json
import os
import sys
import tempfile
from pathlib import Path


def resolve_poppler_path(base_dir: Path) -> Path:
    system = sys.platform
    candidates: list[Path] = []

    if system.startswith("win"):
        candidates.append(base_dir / "poppler" / "win" / "bin")
        candidates.append(base_dir / "poppler" / "Library" / "bin")  # legacy 互換
    elif system == "darwin":
        candidates.append(base_dir / "poppler" / "macos" / "bin")
        candidates.append(Path("/opt/homebrew/opt/poppler/bin"))
        candidates.append(Path("/usr/local/opt/poppler/bin"))
    else:
        candidates.append(base_dir / "poppler" / system / "bin")

    for path in candidates:
        if path.exists():
            return path

    raise FileNotFoundError(
        "Poppler バイナリが見つかりません。OS ごとの bin ディレクトリを用意するか、"
        "Homebrew / Choco などでインストールして PATH を設定してください。"
    )


CropRect = tuple[float, float, float, float]


def parse_crop(value: str | None) -> CropRect | None:
    if not value:
        return None
    parts = [p.strip() for p in value.split(",")]
    if len(parts) != 4:
        raise ValueError("crop は left,top,width,height の4要素が必要です")
    left, top, width, height = (float(p) for p in parts)
    left = max(0.0, min(1.0, left))
    top = max(0.0, min(1.0, top))
    width = max(0.0, min(1.0 - left, width))
    height = max(0.0, min(1.0 - top, height))
    if width <= 0 or height <= 0:
        return None
    return (left, top, width, height)


def apply_crop(img, crop: CropRect | None):
    if not crop:
        return img
    left, top, width, height = crop
    w, h = img.size
    lpx = int(round(left * w))
    tpx = int(round(top * h))
    rpx = int(round((left + width) * w))
    bpx = int(round((top + height) * h))
    if rpx <= lpx or bpx <= tpx:
        return img
    return img.crop((lpx, tpx, rpx, bpx))


def resize_long_edge(img, max_long_edge: int):
    if max_long_edge <= 0:
        return img
    w, h = img.size
    long_edge = max(w, h)
    if long_edge <= max_long_edge:
        return img
    scale = max_long_edge / long_edge
    new_size = (max(1, int(round(w * scale))), max(1, int(round(h * scale))))
    return img.resize(new_size)


def image_to_data_url(img) -> str:
    buf = io.BytesIO()
    img.save(buf, format="PNG", optimize=True)
    b64 = base64.b64encode(buf.getvalue()).decode("ascii")
    return f"data:image/png;base64,{b64}"


def main() -> None:
    parser = argparse.ArgumentParser(description="UI 用の画像プレビュー生成")
    parser.add_argument("--input", required=True, help="入力ファイルパス（PDF/画像）")
    parser.add_argument("--page", type=int, default=1, help="PDF のページ番号（1起点）")
    parser.add_argument("--crop", help="正規化トリミング（left,top,width,height / 0〜1）")
    parser.add_argument("--max-long-edge", type=int, default=1400, help="長辺の最大 px（プレビュー用）")
    args = parser.parse_args()

    base_dir = Path(__file__).resolve().parent
    input_path = Path(args.input)
    if not input_path.exists():
        raise SystemExit(f"input not found: {input_path}")

    crop = parse_crop(args.crop)

    try:
        from PIL import Image, ImageOps
    except ImportError as exc:
        raise SystemExit(f"Pillow is required: {exc}") from exc

    page_count: int | None = None
    page = args.page

    if input_path.suffix.lower() == ".pdf":
        from pdf2image import convert_from_path, pdfinfo_from_path

        poppler_path = resolve_poppler_path(base_dir)
        os.environ["PATH"] = str(poppler_path) + os.pathsep + os.environ.get("PATH", "")

        info = pdfinfo_from_path(str(input_path), poppler_path=str(poppler_path))
        page_count = int(info["Pages"])
        page = max(1, min(page, page_count))

        images = convert_from_path(
            str(input_path),
            dpi=150,
            first_page=page,
            last_page=page,
            fmt="png",
            poppler_path=str(poppler_path),
        )
        img = images[0]
        img = ImageOps.exif_transpose(img)
        img = apply_crop(img, crop)
        img = resize_long_edge(img, args.max_long_edge)
        data_url = image_to_data_url(img)
    else:
        # HEIC/HEIF/SVG を含めて、まず PNG に正規化（tmp 配下に変換）
        from image_normalizer import ensure_png_image
        with tempfile.TemporaryDirectory(prefix="ocr_to_doc_preview_") as tmp:
            conversion = ensure_png_image(input_path, convert_dir=Path(tmp))
            with Image.open(conversion.converted) as img:
                img = ImageOps.exif_transpose(img)
                img = apply_crop(img, crop)
                img = resize_long_edge(img, args.max_long_edge)
                data_url = image_to_data_url(img)

    print(
        json.dumps(
            {
                "dataUrl": data_url,
                "pageCount": page_count,
                "page": page,
            },
            ensure_ascii=False,
        )
    )


if __name__ == "__main__":
    main()
