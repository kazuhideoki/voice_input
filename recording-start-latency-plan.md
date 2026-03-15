# Recording Start Latency Plan

## 目的

キーボードで録音開始をトリガーしてから、実際に録音開始されるまでの遅延を短縮する。

## 調査結果

- `start` の全体遅延はおおむね `391ms` から `485ms`
- クライアント側の IPC 自体は支配的ではない
  - UDS 接続、JSON 送信、Tokio runtime 構築はほぼ `1ms` 未満
  - `voice_input status` の外側実測は平均 `4.5ms`
- 遅延の主因は daemon の開始処理
  - Apple Music 状態確認: 約 `113ms` から `124ms`
  - 録音開始処理: 約 `275ms` から `366ms`
- 録音開始処理の内訳
  - 入力デバイス選択: 約 `132ms` から `221ms`
  - `default_input_config()`: 約 `22ms` から `23ms`
  - `build_input_stream()`: 約 `119ms` から `122ms`

## 現状の問題

- Apple Music が再生されていない場合でも、録音開始前に毎回 `osascript` を待っている
- 録音開始のたびに CPAL のデバイス選択と stream 構築をやり直している
- 開始音は録音開始より前に鳴るため、体感と実際の開始タイミングが一致していない

## 実装方針

### Phase 1

Apple Music の pause 判定を録音開始のクリティカルパスから外す。

- `handle_start()` で録音開始を先に行う
- Apple Music の pause は並行実行にするか、開始後に遅延実行する
- `music_was_playing` の扱いが壊れないように状態管理を見直す

期待効果:

- 約 `110ms` から `120ms` の短縮見込み

進捗:

- 実装済み
- `handle_start()` で録音開始を先に行い、その後で Apple Music pause を非同期化した
- `music_was_playing` の反映は録音セッション ID と照合してから行うようにした
- `MediaControlService` に pause 所有セッションを持たせ、古い pause タスクが新しい pause を打ち消さないようにした
- Apple Music 制御失敗時は「録音開始は成功、Music 制御状態は汚さない」仕様としてテストで固定した

追加した確認:

- 遅い Apple Music 確認があっても start が待たない
- `start -> stop -> 即 start` でも前セッションの遅延 pause が次セッションへ混入しない
- `start1` の pause が遅く `start2` の pause が先に成功しても、古い pause が新しい pause を打ち消さない
- Apple Music 制御失敗でも録音開始自体は成功し、pause 状態が壊れない

### Phase 2

CPAL stream 初期化のコストを下げる。

- 入力デバイスと stream の使い回しを検討する
- 少なくとも毎回の `select_input_device()` と `build_input_stream()` を減らせないか確認する
- 単一スレッド runtime と既存の `AudioBackend` 抽象を崩さない設計にする

期待効果:

- 約 `275ms` から `366ms` の主要部分を削減できる可能性がある

進捗:

- 一部実装済み
- `CpalAudioBackend` に入力デバイスと `default_input_config()` のキャッシュを追加した
- これにより、2回目以降の録音開始では `default_input_config()` の再取得を避ける
- 入力デバイス選択自体は毎回再評価し、現在の `INPUT_DEVICE_PRIORITY` と選択デバイスを照合して、優先マイクや利用可能デバイスの変化があれば再解決する
- `build_input_stream()` はまだ毎回行うため、Phase 2 の削減余地は残っている
- `build_input_stream()` または `stream.play()` に失敗した場合は stale な設定を握り続けないよう、入力設定キャッシュと録音状態を巻き戻す

追加した確認:

- 入力設定キャッシュは明示的に破棄されるまで再利用される
- キャッシュ破棄後は入力設定を再解決する
- キャッシュ済み設定が現在の選択条件と不一致なら再解決する
- 入力設定利用処理が失敗したらキャッシュと録音状態を巻き戻す
- backend の start ワークフローで cache hit / cache miss / build failure / play failure を直接検証する

### Phase 3

開始音のタイミングを再調整する。

- 方針: 録音開始タイミングと開始音をできるだけ一致させる

進捗:

- 実装済み
- 開始音は録音開始成功後に鳴らすよう変更した
- テストで録音開始イベントの後に開始音が鳴ることを確認した

## 変更時のチェック項目

- Apple Music 再生中のみ pause されること
- Apple Music 非再生時に録音開始が速くなること
- `toggle` の start/stop 動作が壊れないこと
- 録音停止後の resume 動作が壊れないこと
- `cargo check`
- `cargo clippy -- -D warnings`
- `cargo fmt -- --check`
- `cargo test`

## 次にやること

1. Phase 2 の残件として `build_input_stream()` 再利用の可否を設計する
2. `Stream` 再利用が安全にできるなら、callback の書き込み先切り替えを導入する
3. 実装前後で start latency を再計測して短縮幅を確認する
