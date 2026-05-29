@echo off
chcp 65001 > nul
setlocal enabledelayedexpansion
cd /d "%~dp0"

echo.
echo ====================================================
echo   OCR-to-Doc アンインストール
echo ====================================================
echo.

:: ============================================================
:: Step 1: glm-ocr モデルの削除
:: ============================================================
echo [1/4] Ollama モデル (glm-ocr) を削除中...
where ollama > nul 2>&1
if errorlevel 1 (
    echo       Ollama が見つかりません。スキップします。
    goto SKIP_OLLAMA
)
ollama rm glm-ocr
if errorlevel 1 (
    echo       [WARNING] glm-ocr の削除に失敗しました（既に存在しない可能性があります）。
) else (
    echo       完了
)
:SKIP_OLLAMA

:: ============================================================
:: Step 2: Python 仮想環境の削除
:: ============================================================
echo.
echo [2/4] Python 仮想環境 (.venv) を削除中...
if exist ".venv" (
    rmdir /s /q ".venv"
    echo       完了
) else (
    echo       .venv は存在しません。スキップします。
)

:: ============================================================
:: Step 3: Poppler の削除
:: ============================================================
echo.
echo [3/4] Poppler を削除中...
if exist "poppler" (
    rmdir /s /q "poppler"
    echo       完了
) else (
    echo       poppler\ は存在しません。スキップします。
)

:: ============================================================
:: Step 4: 処理結果フォルダの削除（確認付き）
:: ============================================================
echo.
if not exist "result" (
    echo [4/4] result\ は存在しません。スキップします。
    goto UNINSTALL_DONE
)
set /p CONFIRM="[4/4] result\ (処理結果) を削除しますか? [Y/N]: "
if /i "!CONFIRM!"=="Y" (
    rmdir /s /q "result"
    echo       完了
) else (
    echo       result\ を保持します。
)

:UNINSTALL_DONE
echo.
echo ====================================================
echo   アンインストール完了。
echo   Ollama 本体を削除する場合は Windows の
echo   「アプリと機能」から手動でアンインストールしてください。
echo ====================================================
echo.
pause
endlocal
