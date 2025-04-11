#!/bin/bash

# スクリプトを一時ファイルに保存
cat <<'EOF' >/tmp/get_selected_text.scpt
tell application "System Events"
    # フォアグラウンドアプリケーションを取得
    set frontApp to name of first application process whose frontmost is true

    # フォアグラウンドアプリケーションに切り替える
    tell application frontApp to activate

    # 少し待機してアプリケーション切り替えを安定させる
    delay 0.5

    # 現在の選択テキストをクリップボードにコピー
    keystroke "c" using {command down}

    # コピー操作が完了するまで十分待機
    delay 0.5

    # クリップボードから取得
    set theText to (do shell script "pbpaste")
    return theText
end tell
EOF

# ユーザーに指示
echo "テキストを選択した状態でこのスクリプトを実行してください"
echo "選択テキストを取得しています..."

# AppleScriptを実行
result=$(osascript /tmp/get_selected_text.scpt)

# 結果を表示
echo "======== 取得した選択テキスト ========"
echo "$result"
echo "======================================"

# 一時ファイルを削除
rm /tmp/get_selected_text.scpt
