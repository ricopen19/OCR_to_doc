"""Ingest pipeline utilities.

Phase 1 では入力ファイルの種別判定を担い、必要なディレクトリの準備のみを行う。
PDF→画像変換など重い処理は今後の拡張ポイントとして stub を残している。
"""

from __future__ import annotations

from dataclasses import dataclass
from enum import Enum
from pathlib import Path
from typing import Optional

from image_normalizer import HEIC_EXTENSIONS, SVG_EXTENSIONS

IMAGE_EXTS = {
    ".png",
    ".jpg",
    ".jpeg",
    ".webp",
    ".tif",
    ".tiff",
    ".bmp",
}
IMAGE_EXTS |= HEIC_EXTENSIONS | SVG_EXTENSIONS
PDF_EXTS = {".pdf"}
DEFAULT_PAGE_DIR = Path("result")
DEFAULT_FIGURE_DIR = DEFAULT_PAGE_DIR / "figures"


class InputKind(str, Enum):
    IMAGE = "image"
    PDF = "pdf"
    UNSUPPORTED = "unsupported"


@dataclass
class InputMeta:
    path: Path
    kind: InputKind
    pages: Optional[int] = None
    note: str = ""

    @property
    def is_image(self) -> bool:
        return self.kind == InputKind.IMAGE

    @property
    def is_pdf(self) -> bool:
        return self.kind == InputKind.PDF


class IngestError(Exception):
    pass


def detect_kind(path: Path) -> InputKind:
    suffix = path.suffix.lower()
    if suffix in IMAGE_EXTS:
        return InputKind.IMAGE
    if suffix in PDF_EXTS:
        return InputKind.PDF
    return InputKind.UNSUPPORTED


def inspect(path: Path) -> InputMeta:
    if not path.exists():
        raise IngestError(f"入力ファイルが見つかりません: {path}")

    kind = detect_kind(path)
    if kind == InputKind.UNSUPPORTED:
        raise IngestError(f"未対応の拡張子です: {path.suffix}")

    note = ""
    pages = None

    if kind == InputKind.PDF:
        # Phase 1 では PDF→画像変換を後回しにするため、ページ数のみ lazily 取得する。
        try:
            from pdf2image import pdfinfo_from_path

            info = pdfinfo_from_path(str(path))
            pages = int(info.get("Pages", 0)) or None
        except Exception as exc:  # pragma: no cover - best effort
            note = f"PDF情報取得に失敗: {exc}" if not note else note

    return InputMeta(path=path, kind=kind, pages=pages, note=note)


def prepare_workdirs(page_dir: Path = DEFAULT_PAGE_DIR, figure_dir: Path = DEFAULT_FIGURE_DIR) -> None:
    page_dir.mkdir(parents=True, exist_ok=True)
    figure_dir.mkdir(parents=True, exist_ok=True)


__all__ = [
    "InputKind",
    "InputMeta",
    "inspect",
    "prepare_workdirs",
    "IngestError",
]
