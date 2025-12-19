"""数式候補を検出してページ画像からトリミングし、Markdown に埋め込みリンクを付与する PoC。

前提:
- YomiToku 実行済みで `result/<name>/page_images/page_###.png` があること。
- YomiToku の JSON 出力が `result/<name>/yomi_formats/json/*.json` または指定パスにあること。
- 既存の結合 Markdown `<base>_merged.md` を読み、`# Page <n>` 見出し単位で挿入する。

出力:
- 数式スニペット画像: `result/<name>/figures/eq_pageNNN_MM.png`
- 画像を差し込んだ Markdown: `<base>_merged_with_eq_img.md`（デフォルト）

使い方（例）:
```bash
poetry run python math_snippet_extractor.py --input result/sample --base-name sample
```
"""

from __future__ import annotations

import argparse
import json
import re
from collections import defaultdict
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable, Iterator, List, Sequence

from PIL import Image


MATH_TOKENS = (
    "$",
    "=",
    "≒",
    "≠",
    "±",
    "+",
    "-",
    "×",
    "÷",
    "∑",
    "∫",
    "√",
    "π",
    "λ",
    "^",
    "log",
    "sin",
    "cos",
    "tan",
    "\\frac",
    "%",
    "％",
    "/",
)

MATH_KEYWORDS = ("比率", "割合", "分数", "率", "比")

URL_PATTERN = re.compile(r"https?://", re.IGNORECASE)
BASE_PATTERN = re.compile(r"\([0-9]{1,3}\)\s*[0-9]{0,3}")  # (10), (12) 等
SUB_SUP_PATTERN = re.compile(r"[_^][0-9]+")

JSON_PATTERN = re.compile(r"page(?:_images)?_page_(\d{3})")
PAGE_HEADING_PATTERN = re.compile(r"^#\s+Page\s+(\d+)$")


@dataclass
class MathRegion:
    page: int
    box: tuple[int, int, int, int]  # left, top, right, bottom
    score: float
    text: str


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="数式領域を抽出して画像を埋め込む PoC")
    parser.add_argument("--input", required=True, help="result/<name> ディレクトリ")
    parser.add_argument("--json-dir", help="YomiToku JSON ディレクトリ (省略時は <input>/yomi_formats/json)")
    parser.add_argument("--page-images", help="ページ画像ディレクトリ (省略時は <input>/page_images)")
    parser.add_argument(
        "--output-md",
        help="出力 Markdown パス。省略時は <input>/<base-name>_merged_with_eq_img.md",
    )
    parser.add_argument(
        "--base-name",
        help="結合済み Markdown のベース名 (default: input ディレクトリ名)",
    )
    parser.add_argument(
        "--padding",
        type=int,
        default=6,
        help="クロップ時に上下左右へ付与する余白(px)",
    )
    parser.add_argument(
        "--max-per-page",
        type=int,
        default=20,
        help="1ページあたり埋め込む数式スニペットの上限",
    )
    parser.add_argument(
        "--min-score",
        type=float,
        default=0.6,
        help="det/rec スコアの下限 (default: 0.6)",
    )
    parser.add_argument(
        "--min-ops",
        type=int,
        default=1,
        help="演算子や記号（+-×÷=/^%）の最小個数 (default: 1)",
    )
    parser.add_argument(
        "--max-chars",
        type=int,
        default=120,
        help="これを超える長文は除外 (default: 120)",
    )
    parser.add_argument(
        "--max-aspect",
        type=float,
        default=6.0,
        help="縦横比の上限（長すぎる帯を除外） default: 6.0",
    )
    return parser.parse_args()


def iter_json_files(json_dir: Path) -> Iterator[tuple[int, Path]]:
    for path in sorted(json_dir.glob("*.json")):
        match = JSON_PATTERN.search(path.stem)
        if not match:
            continue
        page = int(match.group(1))
        yield page, path


def math_features(text: str) -> tuple[int, int, float, bool]:
    """Return (ops, digits, digit_ratio, has_base_marker)."""
    ops = sum(text.count(ch) for ch in "+-×÷=/%^·")
    digits = sum(ch.isdigit() for ch in text)
    length = max(1, len(text))
    digit_ratio = digits / length
    has_base = bool(BASE_PATTERN.search(text) or SUB_SUP_PATTERN.search(text))
    return ops, digits, digit_ratio, has_base


def looks_math(text: str) -> bool:
    if URL_PATTERN.search(text):
        return False
    if any(kw in text for kw in MATH_KEYWORDS):
        return True
    ops, digits, digit_ratio, has_base = math_features(text)
    # 数字だけの行でも基数表記や下付きがあれば許容
    if has_base and digits >= 4:
        return True
    if ops >= 1 and digits >= 2:
        return True
    # 二進数・多桁の羅列を許容（数字率が高い）
    if digit_ratio >= 0.4 and digits >= 6:
        return True
    return False


def load_regions(json_path: Path, *, min_score: float, min_ops: int, max_chars: int, max_aspect: float) -> List[MathRegion]:
    data = json.loads(json_path.read_text(encoding="utf-8"))
    regions: List[MathRegion] = []

    def add_region(page: int, box: Sequence[int], score: float, text: str) -> None:
        if len(box) != 4:
            return
        left, top, right, bottom = box
        # 位置が逆転している場合の簡易補正
        if right < left:
            left, right = right, left
        if bottom < top:
            top, bottom = bottom, top
        width = max(1, right - left)
        height = max(1, bottom - top)
        aspect = max(width / height, height / width)
        if aspect > max_aspect:
            return
        if len(text) > max_chars:
            return
        ops, digits, digit_ratio, has_base = math_features(text)
        if ops < min_ops and not (has_base or digit_ratio >= 0.4):
            return
        if score < min_score:
            return
        regions.append(MathRegion(page=page, box=(left, top, right, bottom), score=score, text=text))

    page = int(JSON_PATTERN.search(json_path.stem).group(1))

    for para in data.get("paragraphs", []):
        text = para.get("contents", "")
        if looks_math(text):
            box = para.get("box")
            if box:
                add_region(page, box, para.get("score", 0.5), text)

    for det in data.get("detections", []):
        text = det.get("content", "")
        if not looks_math(text):
            continue
        points = det.get("points")
        if points and isinstance(points, list) and len(points) >= 4:
            xs = [p[0] for p in points]
            ys = [p[1] for p in points]
            box = [min(xs), min(ys), max(xs), max(ys)]
            add_region(page, box, det.get("rec_score", det.get("det_score", 0.5)), text)

    regions.sort(key=lambda r: (-r.score, r.box[1], r.box[0]))
    return regions


def crop_region(img_path: Path, region: MathRegion, padding: int) -> Image.Image:
    with Image.open(img_path) as img:
        left, top, right, bottom = region.box
        left = max(0, left - padding)
        top = max(0, top - padding)
        right = min(img.width, right + padding)
        bottom = min(img.height, bottom + padding)
        return img.crop((left, top, right, bottom)).copy()


def save_regions(
    regions: Iterable[MathRegion],
    page_image_dir: Path,
    figure_dir: Path,
    padding: int,
    max_per_page: int,
) -> dict[int, list[tuple[str, str]]]:
    figure_dir.mkdir(parents=True, exist_ok=True)
    saved: dict[int, list[tuple[str, str]]] = defaultdict(list)

    for region in regions:
        if len(saved[region.page]) >= max_per_page:
            continue
        img_path = page_image_dir / f"page_{region.page:03}.png"
        if not img_path.exists():
            continue
        cropped = crop_region(img_path, region, padding)
        name = f"eq_page{region.page:03}_{len(saved[region.page]) + 1:02}.png"
        out_path = figure_dir / name
        cropped.save(out_path)
        saved[region.page].append((name, region.text))
    return saved


def insert_links(
    merged_md: Path,
    output_md: Path,
    page_to_images: dict[int, list[tuple[str, str]]],
) -> None:
    raw_lines = merged_md.read_text(encoding="utf-8").splitlines()
    eq_md = re.compile(r"!\[[^\]]*\]\((?:\./)?figures/eq_page\d+_\d+\.png\)")
    eq_html = re.compile(r"<img[^>]+eq_page\d+_\d+\.png[^>]*>", re.IGNORECASE)
    lines = [ln for ln in raw_lines if not (eq_md.search(ln) or eq_html.search(ln) or "eq_page" in ln)]

    def normalize_for_match(text: str) -> str:
        t = text.lower()
        t = re.sub(r"\\text\{([^}]*)\}", r"\1", t)
        t = re.sub(r"[\$`~^\\{}_*\[\]()<>|]", "", t)
        t = re.sub(r"\s+", "", t)
        return t

    output: list[str] = []
    current_page: int | None = None
    page_lines: list[str] = []

    def flush_page() -> None:
        nonlocal page_lines, current_page
        if current_page is None:
            return
        pending = page_to_images.get(current_page, []).copy()
        pending_norm = [(img, normalize_for_match(txt)) for img, txt in pending]

        out_page: list[str] = []
        for line in page_lines:
            out_page.append(line)
            norm_line = normalize_for_match(line)
            if not norm_line:
                continue
            match_idx = None
            for i, (_, txt_norm) in enumerate(pending_norm):
                if txt_norm and txt_norm in norm_line:
                    match_idx = i
                    break
            if match_idx is not None:
                img_name = pending[match_idx][0]
                out_page.extend([
                    "",
                    f'![eq]({Path("figures") / img_name})',
                    "",
                ])
                pending.pop(match_idx)
                pending_norm.pop(match_idx)

        for img_name, _ in pending:
            out_page.extend([
                "",
                f'![eq]({Path("figures") / img_name})',
                "",
            ])

        output.extend(out_page)
        page_lines = []

    for line in lines:
        stripped_line = line.strip()
        heading_match = PAGE_HEADING_PATTERN.match(stripped_line)
        if heading_match:
            flush_page()
            current_page = int(heading_match.group(1))
            output.append(line)
            continue

        if current_page is None:
            output.append(line)
            continue

        page_lines.append(line)

    flush_page()

    # 連続する空行を 2 行までに抑える（挿入時に空行を追加しているため）
    cleaned: list[str] = []
    blank_run = 0
    for line in output:
        if line.strip() == "":
            blank_run += 1
        else:
            blank_run = 0
        if blank_run <= 2:
            cleaned.append(line)

    output_md.write_text("\n".join(cleaned) + "\n", encoding="utf-8")


def main() -> None:
    args = parse_args()

    input_dir = Path(args.input)
    if not input_dir.exists():
        raise SystemExit(f"input ディレクトリがありません: {input_dir}")

    json_dir = Path(args.json_dir) if args.json_dir else input_dir / "yomi_formats" / "json"
    page_image_dir = Path(args.page_images) if args.page_images else input_dir / "page_images"
    figure_dir = input_dir / "figures"

    base_name = args.base_name or input_dir.name
    merged_md = input_dir / f"{base_name}_merged.md"
    if not merged_md.exists():
        raise SystemExit(f"結合済み Markdown が見つかりません: {merged_md}")

    if not json_dir.exists():
        raise SystemExit(f"YomiToku JSON ディレクトリが見つかりません: {json_dir}")
    if not page_image_dir.exists():
        raise SystemExit(f"ページ画像ディレクトリが見つかりません: {page_image_dir}")

    output_md = Path(args.output_md) if args.output_md else input_dir / f"{base_name}_merged_with_eq_img.md"

    regions: list[MathRegion] = []
    for page, path in iter_json_files(json_dir):
        regions.extend(
            load_regions(
                path,
                min_score=args.min_score,
                min_ops=args.min_ops,
                max_chars=args.max_chars,
                max_aspect=args.max_aspect,
            )
        )

    if not regions:
        raise SystemExit("数式候補が見つかりませんでした。ヒューリスティックを調整してください。")

    saved = save_regions(
        regions=regions,
        page_image_dir=page_image_dir,
        figure_dir=figure_dir,
        padding=args.padding,
        max_per_page=args.max_per_page,
    )

    insert_links(merged_md=merged_md, output_md=output_md, page_to_images=saved)

    print(f"数式候補 {sum(len(v) for v in saved.values())} 件を保存しました: {figure_dir}")
    print(f"Markdown 出力: {output_md}")


if __name__ == "__main__":
    main()
