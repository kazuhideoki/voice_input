# カーソル位置への直接入力についてのアイデア

現在 `voice_input` は、転写後のテキストをクリップボードへコピーし、`System Events` 経由で ⌘V を送信することにより貼り付けています。Paste を経由せずに直接カーソル位置へ文字列を入力する方法として、以下の案を検討できます。

## Quartz Event Services を使ったキーボードイベント送信

macOS の Quartz Event Services (`CGEventCreateKeyboardEvent` など) を利用すると、任意のキー入力イベントを合成してポストできます。この API を呼び出すことで、テキストを 1 文字ずつ "タイプ" するように送信できます。

- `core-graphics` クレートや C バインディングを使用して `CGEvent` を作成
- フォアグラウンドアプリケーションに対してイベントを `CGEventPost` で送信
- アクセシビリティアクセスを許可する必要がある

## Accessibility API 経由でフォーカス要素に文字列を設定

`AXUIElement` などの Accessibility API を利用すると、現在フォーカスされている UI 要素 (テキストフィールド等) を取得し、その `AXValue` を直接書き換えることが可能です。対象アプリがアクセシビリティ操作を受け付ける必要があります。

1. `AXUIElementCreateSystemWide()` でシステム全体のアクセシビリティオブジェクトを取得
2. `kAXFocusedUIElementAttribute` から現在フォーカスされている要素を得る
3. `AXUIElementSetAttributeValue` で `kAXValueAttribute` を設定

## IME / Input Method Kit への統合

より高度な方法として、独自の Input Method (IME) を実装し、転写結果をその IME の変換結果として出力する案もあります。実装コストは高いものの、テキスト入力システムとしてシームレスに連携できます。

## まとめ

上記いずれかの API を利用すれば、クリップボードを介さずにテキストをカーソル位置へ送信できます。実装難易度や対象アプリとの互換性を考慮し、まずは Quartz Event Services を用いたキーボードイベントの生成から試すのが現実的と考えられます。

