"""Pix2Text を使って Markdown 内の数式ブロックを置換するユーティリティ。"""

from __future__ import annotations

import logging
import os
import re
from dataclasses import dataclass
from pathlib import Path
from typing import List, Sequence

logger = logging.getLogger(__name__)

try:  # Pix2Text まわりの import は重いので遅延しないようにまとめて実行
    from pix2text import Pix2Text
    from pix2text.page_elements import ElementType
except Exception as exc:  # pragma: no cover - import エラー時に後続で例外化
    Pix2Text = None  # type: ignore[assignment]
    ElementType = None  # type: ignore[assignment]
    _PIX2TEXT_IMPORT_ERROR: Exception | None = exc
else:
    _PIX2TEXT_IMPORT_ERROR = None

JP_PATTERN = re.compile(r"[\u3000-\u30ff\u3400-\u9fff]")
FULLWIDTH_DIGIT_PATTERN = re.compile(r"[\uff10-\uff19]")
FORMULA_CHAR_SET = set(
    "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ"  # 英数字
    "=+-*/×÷·・.,;:!?'\"|^_`~\\"  # 演算・記号類
    "()[]{}<>«»＃％＆＠＄＋－＝～‖∥∠≒≈≡≤≥∞∫∑∏√∇°′″"  # 数式記号
    "→←↔⇒⇔±％‰℃°πΠΣΩΔΛαβγδεζηθικλμνξοπρστυφχψω"  # 矢印・ギリシャ文字
)


@dataclass
class RecognizedFormula:
    latex: str
    isolated: bool
    score: float
    y_center: float


@dataclass(frozen=True)
class BlockRange:
    start: int
    end: int


@dataclass
class MathRefineResult:
    replaced: int = 0
    unused: int = 0


class MathRefiner:
    """Pix2Text で抽出した数式を Markdown に反映するクラス。"""

    def __init__(
        self,
        *,
        cache_root: Path | None = None,
        min_score: float = 0.65,
        resized_shape: int = 960,
    ) -> None:
        if _PIX2TEXT_IMPORT_ERROR is not None:
            raise RuntimeError(
                "pix2text をインポートできませんでした。poetry install 後に再実行してください。"
            ) from _PIX2TEXT_IMPORT_ERROR

        self.min_score = min_score
        self.resized_shape = resized_shape
        self._engine: Pix2Text | None = None

        base_dir = Path(__file__).resolve().parent
        if cache_root is None:
            root = base_dir / ".pix2text_cache"
        else:
            root = Path(cache_root)
            if not root.is_absolute():
                root = base_dir / root
        self._prepare_cache_dirs(root)

    def refine_page(
        self,
        *,
        page_md_paths: Sequence[Path],
        image_path: Path,
        page_number: int,
    ) -> MathRefineResult:
        if not page_md_paths:
            return MathRefineResult()
        if not image_path.exists():
            logger.debug("%s が存在しないため数式置換をスキップ", image_path)
            return MathRefineResult()

        formulas = self._extract_formulas(image_path, page_number)
        if not formulas:
            return MathRefineResult()

        consumed = 0
        replaced_total = 0
        sorted_pages = sorted(page_md_paths)

        for md_path in sorted_pages:
            text = md_path.read_text(encoding="utf-8")
            newline = "\n" if text.endswith("\n") else ""
            lines = text.splitlines()
            lines, replaced, consumed = self._replace_in_lines(lines, formulas, consumed)
            if replaced:
                md_path.write_text("\n".join(lines) + newline, encoding="utf-8")
                replaced_total += replaced

        unused = max(0, len(formulas) - consumed)
        if unused:
            logger.debug("page %s: %s 件の数式が置換できませんでした", page_number, unused)
        return MathRefineResult(replaced=replaced_total, unused=unused)

    # ----------------------------------------------------------------------------------
    # 内部処理
    # ----------------------------------------------------------------------------------

    def _prepare_cache_dirs(self, root: Path) -> None:
        cache_map = {
            "PIX2TEXT_HOME": root / "pix2text",
            "CNOCR_HOME": root / "cnocr",
            "CNSTD_HOME": root / "cnstd",
            "MPLCONFIGDIR": root / "matplotlib",
        }
        for key, path in cache_map.items():
            if os.environ.get(key):
                continue
            path.mkdir(parents=True, exist_ok=True)
            os.environ[key] = str(path)

    def _load_engine(self) -> Pix2Text:
        if self._engine is None:
            self._engine = Pix2Text()
        return self._engine

    def _extract_formulas(self, image_path: Path, page_number: int) -> List[RecognizedFormula]:
        engine = self._load_engine()
        page = engine.recognize_page(
            str(image_path),
            page_number=page_number,
            resized_shape=self.resized_shape,
            text_contain_formula=True,
        )
        formulas: List[RecognizedFormula] = []
        for element in page.elements:
            if element.type != ElementType.FORMULA:
                continue
            latex = (element.text or "").strip()
            score = element.score
            if isinstance(element.meta, dict):
                latex = (element.meta.get("text") or latex).strip()
                score = float(element.meta.get("score", score))
            if not latex or score < self.min_score:
                continue
            box = element.box or [0, 0, 0, 0]
            y_center = (box[1] + box[3]) / 2 if len(box) >= 4 else 0.0
            formulas.append(
                RecognizedFormula(
                    latex=latex,
                    isolated=bool(element.isolated),
                    score=score,
                    y_center=y_center,
                )
            )
        formulas.sort(key=lambda item: item.y_center)
        return formulas

    def _replace_in_lines(
        self,
        lines: List[str],
        formulas: Sequence[RecognizedFormula],
        start_index: int,
    ) -> tuple[List[str], int, int]:
        blocks = self._detect_formula_blocks(lines)
        if not blocks:
            return lines, 0, start_index

        new_lines: List[str] = []
        formula_idx = start_index
        replaced = 0
        block_iter = iter(blocks)
        current = next(block_iter, None)
        i = 0

        while i < len(lines):
            if current and i == current.start:
                if formula_idx < len(formulas):
                    rendered = self._render_formula(formulas[formula_idx])
                    new_lines.append(rendered)
                    replaced += 1
                    formula_idx += 1
                else:
                    new_lines.extend(lines[current.start : current.end])
                i = current.end
                current = next(block_iter, None)
                continue
            new_lines.append(lines[i])
            i += 1

        return new_lines, replaced, formula_idx

    def _detect_formula_blocks(self, lines: Sequence[str]) -> List[BlockRange]:
        blocks: List[BlockRange] = []
        idx = 0
        while idx < len(lines):
            if self._looks_like_formula_block(lines[idx]):
                start = idx
                end = idx + 1
                while end < len(lines) and self._looks_like_formula_block(lines[end]):
                    end += 1
                blocks.append(BlockRange(start=start, end=end))
                idx = end
            else:
                idx += 1
        return blocks

    def _looks_like_formula_block(self, line: str) -> bool:
        stripped = line.strip()
        if not stripped:
            return False
        if stripped.startswith(('#', '《', '<img', '問')):
            return False
        if stripped.startswith(('- ', '* ')):
            return False
        if stripped.startswith('$$') and stripped.endswith('$$'):
            return True

        br_count = stripped.count('<br>')
        body = stripped.replace('<br>', '').replace('\u3000', '').strip()
        if not body:
            return False

        total = len(body)
        math_chars = sum(1 for ch in body if ch in FORMULA_CHAR_SET)
        digit_like = any(ch.isdigit() for ch in body) or bool(FULLWIDTH_DIGIT_PATTERN.search(body))
        jp_chars = len(JP_PATTERN.findall(body))
        has_equation = any(sym in body for sym in ('=', '＝', '≒', '≈', '≡', '∝'))

        if br_count >= 2 and math_chars / total >= 0.5:
            return True
        if digit_like and has_equation and math_chars >= (jp_chars + 2):
            return True
        if math_chars / total >= 0.75 and jp_chars <= 2 and total <= 160:
            return True
        return False

    def _render_formula(self, formula: RecognizedFormula) -> str:
        latex = formula.latex.strip()
        if latex.startswith("$$") and latex.endswith("$$"):
            latex = latex[2:-2].strip()
        elif latex.startswith("$") and latex.endswith("$"):
            latex = latex[1:-1].strip()

        if formula.isolated:
            return f"$$\n{latex}\n$$"
        return f"${latex}$"


__all__ = ["MathRefiner", "MathRefineResult"]
