"""入力ファイル種別に応じて処理モジュールを切り替える。"""

from __future__ import annotations

from pathlib import Path

from ingest import inspect, InputKind


def run(path: Path) -> None:
    meta = inspect(path)
    if meta.is_image or meta.is_pdf:
        print(f"[dispatcher] OCR パスへ委譲: {path}")
        # TODO: ocr_chanked.py を呼び出す/モジュール化する
    else:
        print(f"[dispatcher] 未サポート: {path}")


if __name__ == "__main__":
    run(Path("sample.pdf"))
