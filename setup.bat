@echo off
chcp 65001 > nul
setlocal enabledelayedexpansion
cd /d "%~dp0"

echo.
echo ====================================================
echo   OCR-to-Doc セットアップ
echo ====================================================
echo.

:: ============================================================
:: Step 1: uv の確認 / インストール
:: ============================================================
echo [1/5] uv を確認中...
where uv > nul 2>&1
if not errorlevel 1 (
    echo       uv が見つかりました。
    goto UV_READY
)
echo       uv が見つかりません。インストールします...
powershell -NoProfile -ExecutionPolicy Bypass -Command "irm https://astral.sh/uv/install.ps1 | iex"
if errorlevel 1 (
    echo [ERROR] uv のインストールに失敗しました。
    echo         手動でインストール後に再実行してください:
    echo         https://docs.astral.sh/uv/getting-started/installation/
    pause & exit /b 1
)
set "PATH=%USERPROFILE%\.local\bin;%PATH%"
where uv > nul 2>&1
if errorlevel 1 (
    echo [ERROR] インストール後も uv が見つかりません。
    echo         新しいコマンドプロンプトを開いて再実行してください。
    pause & exit /b 1
)
:UV_READY
echo       OK

:: ============================================================
:: Step 2: Python 依存インストール
:: ============================================================
echo.
echo [2/5] Python 依存パッケージをインストール中...
uv sync --no-install-project
if errorlevel 1 (
    echo [ERROR] uv sync に失敗しました。
    pause & exit /b 1
)
echo       完了

:: ============================================================
:: Step 3: Ollama の確認 / インストール
:: ============================================================
echo.
echo [3/5] Ollama を確認中...
where ollama > nul 2>&1
if not errorlevel 1 (
    echo       Ollama が見つかりました。
    goto OLLAMA_INSTALLED
)
echo       Ollama が見つかりません。winget でインストールします...
winget install Ollama.Ollama --silent --accept-source-agreements --accept-package-agreements
if not errorlevel 1 goto OLLAMA_WINGET_OK
echo       winget でのインストールに失敗しました。公式インストーラを試みます...
powershell -NoProfile -ExecutionPolicy Bypass -Command "Invoke-WebRequest -Uri 'https://ollama.com/download/OllamaSetup.exe' -OutFile ([IO.Path]::Combine($env:TEMP, 'OllamaSetup.exe')) -UseBasicParsing"
if errorlevel 1 (
    echo [ERROR] Ollama のダウンロードに失敗しました。
    echo         手動でインストールしてください: https://ollama.com/download/windows
    pause & exit /b 1
)
start /wait "" "%TEMP%\OllamaSetup.exe"
del "%TEMP%\OllamaSetup.exe" > nul 2>&1
:OLLAMA_WINGET_OK
set "PATH=%LOCALAPPDATA%\Programs\Ollama;%PATH%"
:OLLAMA_INSTALLED
echo       OK

:: ============================================================
:: Step 4: Ollama サービス起動 & glm-ocr モデル取得
:: ============================================================
echo.
echo [4/5] glm-ocr モデルをダウンロード中...
echo       ※ モデルのダウンロードには数 GB の通信が発生します（数分〜十数分）。
echo.
start /b "" ollama serve > nul 2>&1
set RETRY=0
:WAIT_OLLAMA
ollama list > nul 2>&1
if not errorlevel 1 goto OLLAMA_READY
set /a RETRY+=1
if %RETRY% geq 15 (
    echo [ERROR] Ollama サービスが起動しませんでした。
    echo         手動で「ollama serve」を実行してから再実行してください。
    pause & exit /b 1
)
timeout /t 2 /nobreak > nul
goto WAIT_OLLAMA
:OLLAMA_READY
ollama pull glm-ocr
if errorlevel 1 (
    echo [ERROR] glm-ocr のダウンロードに失敗しました。
    echo         ネットワーク接続を確認して再実行してください。
    pause & exit /b 1
)
echo       完了

:: ============================================================
:: Step 5: Poppler のダウンロード / 展開
:: ============================================================
echo.
echo [5/5] Poppler を確認中...
if exist "poppler\Library\bin\pdfinfo.exe" (
    echo       Poppler は既にインストール済みです。スキップします。
    goto SETUP_DONE
)
if exist "poppler\win\bin\pdfinfo.exe" (
    echo       Poppler は既にインストール済みです。スキップします。
    goto SETUP_DONE
)

set POPPLER_VER=24.02.0-0
set POPPLER_ZIP=%TEMP%\poppler-windows.zip
set POPPLER_EXTRACT=%TEMP%\poppler-extract
set POPPLER_URL=https://github.com/oschwartz10612/poppler-Windows/releases/download/v%POPPLER_VER%/Release-%POPPLER_VER%.zip

echo       ダウンロード中...
powershell -NoProfile -ExecutionPolicy Bypass -Command "Invoke-WebRequest -Uri '%POPPLER_URL%' -OutFile '%POPPLER_ZIP%' -UseBasicParsing"
if errorlevel 1 (
    echo [ERROR] Poppler のダウンロードに失敗しました。
    echo         ネットワーク接続を確認して再実行してください。
    pause & exit /b 1
)

echo       展開中...
powershell -NoProfile -ExecutionPolicy Bypass -Command "Expand-Archive -Path '%POPPLER_ZIP%' -DestinationPath '%POPPLER_EXTRACT%' -Force"
if errorlevel 1 (
    echo [ERROR] Poppler の展開に失敗しました。
    del "%POPPLER_ZIP%" > nul 2>&1
    pause & exit /b 1
)

set "POPPLER_SRC="
for /f "tokens=*" %%d in ('dir /b /ad "%POPPLER_EXTRACT%"') do set "POPPLER_SRC=%POPPLER_EXTRACT%\%%d"
if not defined POPPLER_SRC (
    echo [ERROR] Poppler の展開先フォルダが見つかりません。
    del "%POPPLER_ZIP%" > nul 2>&1
    rmdir /s /q "%POPPLER_EXTRACT%" > nul 2>&1
    pause & exit /b 1
)

if not exist "poppler" mkdir poppler
robocopy "%POPPLER_SRC%" "poppler" /e /is /it > nul
set ROBO=%ERRORLEVEL%
del "%POPPLER_ZIP%" > nul 2>&1
rmdir /s /q "%POPPLER_EXTRACT%" > nul 2>&1
if %ROBO% gtr 7 (
    echo [ERROR] Poppler のコピーに失敗しました（robocopy exit: %ROBO%）
    pause & exit /b 1
)
echo       完了

:SETUP_DONE
echo.
echo ====================================================
echo   セットアップ完了！
echo   ocr-to-doc.exe を起動してご利用ください。
echo ====================================================
echo.
pause
endlocal
