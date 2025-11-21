"""互換性のために残している旧 CLI から postprocess を呼び出す。"""

from pathlib import Path
import sys

CURRENT_DIR = Path(__file__).resolve().parent
ROOT = CURRENT_DIR.parent
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from postprocess import main  # noqa: E402  pylint: disable=wrong-import-position


if __name__ == "__main__":  # pragma: no cover - CLI entry
    main()
