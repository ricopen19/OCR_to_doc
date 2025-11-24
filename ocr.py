"""YomiToku OCR ラッパー。

- full/lite の切り替え
- --figure オプションの有効/無効
- 出力 Markdown のファイル名正規化（`page_000.md` 形式）

既存の `ocr_chanked.py` などからインポートして使えるように、
最小限の関数とデータクラスを提供する。
"""

from __future__ import annotations

import re
import subprocess
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable, Sequence, Dict

from PIL import Image, ImageStat

from markdown_cleanup import clean_file

RAW_MD_PATTERN = re.compile(r"page_(\d+)_p(\d+)\.md")
ALT_MD_PATTERN = re.compile(r"(?:.*_)?page_?(\d+)(?:_p(\d+))?\.md")
TARGET_MD_PATTERN = re.compile(r"page_(\d+)(?:_p(\d+))?\.md")
RAW_FIG_PATTERN = re.compile(r"(?:.*_)?page_(\d+)(?:_p(\d+))?_figure_(\d+)(\.[A-Za-z0-9]+)$")
MATH_PATTERN = re.compile(r"(\\\(|\\\[)(.*?)(\\\)|\\\])", re.DOTALL)


@dataclass
class OcrOptions:
    mode: str = "lite"  # "lite" or "full"
    device: str = "cpu"
    enable_figure: bool = True
    extra_args: Sequence[str] | None = None

    def to_cli_args(self) -> list[str]:
        args: list[str] = []
        if self.mode == "lite":
            args.extend(["--lite", "-d", self.device])
        else:
            # full の場合でも device を明示しておく
            args.extend(["-d", self.device])
        if self.enable_figure:
            args.append("--figure")
        if self.extra_args:
            args.extend(self.extra_args)
        return args


def build_command(image_path: Path, output_dir: Path, options: OcrOptions) -> list[str]:
    cmd = [
        "yomitoku",
        str(image_path),
        "-f",
        "md",
        "-o",
        str(output_dir),
    ]
    cmd.extend(options.to_cli_args())
    return cmd


def normalize_markdown_files(output_dir: Path, target_page: int | None = None) -> None:
    for md_path in output_dir.glob("*.md"):
        name = md_path.name
        if TARGET_MD_PATTERN.fullmatch(name):
            continue
        match = RAW_MD_PATTERN.fullmatch(name)
        if not match:
            match = ALT_MD_PATTERN.fullmatch(name)
        if not match:
            continue

        page_num = int(match.group(1))
        part = int(match.group(2) or "1")

        if target_page is not None and page_num != target_page:
            continue

        suffix = "" if part <= 1 else f"_p{part:02}"
        new_path = output_dir / f"page_{page_num:03}{suffix}.md"
        if new_path.exists():
            new_path.unlink()
        md_path.rename(new_path)


def rename_figure_assets(output_dir: Path, page_number: int) -> None:
    figure_dir = output_dir / "figures"
    if not figure_dir.exists():
        return

    entries = []
    for fig_path in figure_dir.glob("*"):
        match = RAW_FIG_PATTERN.match(fig_path.name)
        if not match:
            continue
        page = int(match.group(1))
        if page != page_number:
            continue
        part = int(match.group(2) or 0)
        idx = int(match.group(3))
        entries.append((part, idx, fig_path))

    if not entries:
        return

    entries.sort()
    mapping: Dict[str, str] = {}
    for new_idx, (_, _, fig_path) in enumerate(entries, start=1):
        new_name = f"fig_page{page_number:03d}_{new_idx:02d}{fig_path.suffix.lower()}"
        new_path = figure_dir / new_name
        fig_path.rename(new_path)
        mapping[fig_path.name] = new_name

    _update_markdown_figure_links(output_dir, page_number, mapping)
    remove_icon_figures(output_dir, page_number)


def _update_markdown_figure_links(output_dir: Path, page_number: int, mapping: Dict[str, str]) -> None:
    if not mapping:
        return

    candidates = list(output_dir.glob(f"page_{page_number:03}*.md"))
    for md_path in candidates:
        text = md_path.read_text(encoding="utf-8")
        replaced = False
        for old, new in mapping.items():
            new_path = f"./figures/{new}"
            replacements = [
                (f'src="figures/{old}"', f'src="{new_path}"'),
                (f'src="./figures/{old}"', f'src="{new_path}"'),
                (f'src="{old}"', f'src="{new_path}"'),
                (f'](figures/{old})', f']({new_path})'),
                (f'](./figures/{old})', f']({new_path})'),
                (f']({old})', f']({new_path})'),
            ]
            for old_token, new_token in replacements:
                if old_token in text:
                    text = text.replace(old_token, new_token)
                    replaced = True
            # フォールバック（素のパスだけが残っているケース）
            raw_variants = [f"figures/{old}", f"./figures/{old}", old]
            for variant in raw_variants:
                if variant in text:
                    text = text.replace(variant, new_path)
                    replaced = True
        sanitized = _sanitize_math(text)
        if sanitized != text:
            text = sanitized
            replaced = True
        if replaced:
            md_path.write_text(text, encoding="utf-8")

    cleanup_markdown_files(output_dir, page_number)


def cleanup_markdown_files(output_dir: Path, page_number: int) -> None:
    for md_path in output_dir.glob(f"page_{page_number:03}*.md"):
        clean_file(md_path, inplace=True)


def remove_icon_figures(output_dir: Path, page_number: int) -> None:
    figure_dir = output_dir / "figures"
    if not figure_dir.exists():
        return

    icon_files: list[Path] = []
    for fig_path in figure_dir.glob(f"fig_page{page_number:03d}_*"):
        if fig_path.suffix.lower() not in {".png", ".jpg", ".jpeg"}:
            continue
        try:
            with Image.open(fig_path) as img:
                if is_icon_image(img):
                    icon_files.append(fig_path)
        except Exception:
            continue

    if not icon_files:
        return

    for icon in icon_files:
        remove_figure_references(output_dir, page_number, icon.name)
        try:
            icon.unlink()
        except FileNotFoundError:
            pass


def is_icon_image(image: Image.Image) -> bool:
    width, height = image.size
    area = width * height
    if area == 0:
        return False
    if width > 220 or height > 220 or area > 35000:
        return False

    sample = image.convert("RGB")
    colors = sample.getcolors(maxcolors=4096) or []
    unique = len(colors)
    stat = ImageStat.Stat(sample)
    avg_std = sum(stat.stddev) / max(1, len(stat.stddev))

    if unique <= 40:
        return True
    if avg_std < 18:
        return True
    dominant_ratio = 0
    if colors:
        dominant = max(count for count, _ in colors)
        dominant_ratio = dominant / area
    return dominant_ratio >= 0.85


def remove_figure_references(output_dir: Path, page_number: int, figure_name: str) -> None:
    candidates = list(output_dir.glob(f"page_{page_number:03}*.md"))
    patterns = [
        rf"!\[[^\]]*\]\((?:\./)?figures/{re.escape(figure_name)}\)",
        rf"<img[^>]+src=\"(?:\./)?figures/{re.escape(figure_name)}\"[^>]*>"
    ]
    combined = re.compile("|".join(patterns))
    for md_path in candidates:
        text = md_path.read_text(encoding="utf-8")
        new_text = combined.sub("", text)
        if new_text != text:
            md_path.write_text(new_text, encoding="utf-8")


def _sanitize_math(text: str) -> str:
    def repl(match: re.Match[str]) -> str:
        opener, body, closer = match.groups()
        body = body.replace(r"\-", "-")
        body = body.replace(r"\+", "+")
        body = body.replace(r"\×", r"\\times ")
        body = body.replace(r"\÷", r"\\div ")
        body = body.replace(r"\=", "=")
        new_opener = "$$" if opener == r"\[" else "$"
        new_closer = new_opener
        return f"{new_opener}{body}{new_closer}"

    return MATH_PATTERN.sub(repl, text)


def run_ocr(image_path: Path, output_dir: Path, page_number: int, options: OcrOptions) -> None:
    output_dir.mkdir(parents=True, exist_ok=True)
    cmd = build_command(image_path, output_dir, options)
    subprocess.run(cmd, check=True)
    normalize_markdown_files(output_dir, target_page=page_number)
    rename_figure_assets(output_dir, page_number)


def run_batch(image_paths: Iterable[Path], output_dir: Path, start_page: int = 1, options: OcrOptions | None = None) -> None:
    options = options or OcrOptions()
    for idx, img in enumerate(image_paths, start=start_page):
        run_ocr(img, output_dir, idx, options)


__all__ = [
    "OcrOptions",
    "build_command",
    "normalize_markdown_files",
    "rename_figure_assets",
    "run_ocr",
    "run_batch",
]
