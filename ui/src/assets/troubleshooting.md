大容量PDFでPCが落ちる（Windows）
・PDF DPI を下げる（100〜150）
・処理モードは lite、図表抽出は OFF
・ページ範囲を分割して実行（例: 1〜50 / 51〜100）
・チャンクサイズを小さくする（3〜5ページ）
・実行中は他アプリを閉じる
・Windows の仮想メモリ（ページファイル）を増やす

実行中に黒いウィンドウが表示される
内部処理のために一時的に表示されるものです。正常な動作なので閉じないでください（処理完了後に自動で消えます）。

安定して実行するための目安
・長いPDFはページ範囲を分割して処理
・安定性優先のときは lite + 図表抽出OFF
・負荷が高いPCはチャンクサイズを小さくする

macOS版の出力先
・既定: ~/Library/Application Support/ocr-to-doc/result
・設定 > 出力ルートディレクトリ で変更できます

macOS DMG（ランタイム同梱）
・ビルド前に ui/src-tauri/resources に配置
・M1 は arm64 の配布物を使用
・python-build-standalone を ui/src-tauri/resources/python に展開（bin/python または bin/python3 が必要）
・Pythonスクリプトは ui/src-tauri/resources/py に配置（dispatcher.py など）
・poppler は ui/src-tauri/resources/py/poppler/macos/bin に配置
・アイコン設定は ui/src-tauri/resources/py/configs/icon_profiles に配置
・アプリ内で resources が Contents/Resources/_up_/resources 配下になることがある

Excelの記号セル（○/□/× など）が空になる
・設定 >「Excelの記号補完を有効化」を ON
・tesseract 本体は OS 側に別途インストールが必要（Poetry では pytesseract のみ管理）
  - macOS: `brew install tesseract`
  - Windows: 公式インストーラで導入し PATH を通す
  - Linux: `sudo apt install tesseract-ocr` など

問い合わせ先
不具合報告・要望は メールアドレス へお願いします。
ricopen.continue@gmail.com
