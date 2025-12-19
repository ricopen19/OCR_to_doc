"""page_images を入力に YomiToku の JSON を <input>/yomi_formats/json に出力するユーティリティ。

使い方:
    poetry run python export_yomi_json.py --input result/応用情報技術者_p38-39

オプション:
    --pages 38,39    対象ページをカンマ区切りで指定（未指定なら page_images にある全ページ）
    --mode lite/full YomiToku モード（default: lite）
"""

from __future__ import annotations

import argparse
import subprocess
from pathlib import Path
from typing import Iterable, List


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="page_images から YomiToku JSON を書き出す")
    parser.add_argument("--input", required=True, help="result/<name> ディレクトリ")
    parser.add_argument(
        "--pages",
        help="処理ページをカンマ区切りで指定 (例: 38,39)。未指定なら page_images/*.png を全部処理",
    )
    parser.add_argument(
        "--mode",
        choices=["lite", "full"],
        default="lite",
        help="YomiToku モード (default: lite)",
    )
    return parser.parse_args()


def list_targets(page_image_dir: Path, pages: Iterable[int] | None) -> List[tuple[int, Path]]:
    targets: List[tuple[int, Path]] = []
    if pages is None:
        for path in sorted(page_image_dir.glob("page_*.png")):
            try:
                page = int(path.stem.split("_")[1])
            except (IndexError, ValueError):
                continue
            targets.append((page, path))
    else:
        for page in pages:
            path = page_image_dir / f"page_{page:03}.png"
            if path.exists():
                targets.append((page, path))
    return targets


def run_yomitoku(img_path: Path, output_dir: Path, mode: str) -> None:
    cmd = [
        "yomitoku",
        str(img_path),
        "-f",
        "json",
        "-o",
        str(output_dir),
        "-d",
        "cpu",
    ]
    if mode == "lite":
        cmd.insert(4, "--lite")
    subprocess.run(cmd, check=True)


def main() -> None:
    args = parse_args()
    input_dir = Path(args.input)
    page_image_dir = input_dir / "page_images"
    if not page_image_dir.exists():
        raise SystemExit(f"page_images が見つかりません: {page_image_dir}")

    pages = None
    if args.pages:
        pages = []
        for token in args.pages.split(","):
            token = token.strip()
            if not token:
                continue
            try:
                pages.append(int(token))
            except ValueError:
                raise SystemExit(f"ページ指定が整数ではありません: {token}")

    targets = list_targets(page_image_dir, pages)
    if not targets:
        raise SystemExit("処理対象ページがありません")

    json_dir = input_dir / "yomi_formats" / "json"
    json_dir.mkdir(parents=True, exist_ok=True)

    for page, img in targets:
        print(f"page {page}: {img.name} → {json_dir}")
        run_yomitoku(img, json_dir, args.mode)

    print(f"完了: {len(targets)} ページ分の JSON を {json_dir} に出力しました")


if __name__ == "__main__":
    main()
