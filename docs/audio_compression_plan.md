# 音声データ圧縮導入の実装方針

## 現状整理
- `CpalAudioBackend::stop_recording` は 16bit PCM サンプルを `combine_wav_data` で WAV (44byte ヘッダー + PCM) に変換し、`AudioData(Vec<u8>)` として返却している。現状は**無圧縮 PCM**が API へそのまま送られている。参照: `src/infrastructure/audio/cpal_backend.rs`。
- OpenAI 転写クライアント (`OpenAiClient::transcribe_audio`) は `multipart/form-data` で `audio/wav` をアップロードしている。参照: `src/infrastructure/external/openai.rs`。
- 無圧縮 PCM/WAV は 48kHz・16bit・ステレオで約 192 kb/s (= 約 1.5 Mbps) の帯域を消費し、録音時間が長いほど送信サイズが比例的に増える。

## ゴール
- 録音品質を損なわずに（= 可逆圧縮/高品質不可逆圧縮を選択できるようにして）音声データの転送サイズを削減し、転写 API への送信時間を短縮する。
- 既存の CLI / デーモン利用者に対して非互換を生まない（互換フォーマット/フォールバックを用意する）。

## 推奨アプローチ
1. **デフォルトは FLAC (可逆圧縮) を採用**
   - 可逆圧縮のため音質劣化なし。
   - 一般的に 30〜50% 程度のサイズ削減が見込める。
   - OpenAI Whisper / audio transcription API は `audio/flac` を受け付ける。
2. **オプションで Opus (可逆ではないが高品質) も選択可能にする**
   - 音質と圧縮率のトレードオフに柔軟に対応。
   - 低遅延・高圧縮が必要な利用者向け。
3. **環境変数でフォーマットを切り替え**
   - 例: `VOICE_INPUT_AUDIO_FORMAT=wav|flac|opus`
   - 未指定時は `flac` を既定値とし、互換性のため `wav` へフォールバック可能にする。

## 実装ステップ詳細

### 1. 音声データ表現の拡張
- `AudioData` を単なる `Vec<u8>` から `(bytes, mime_type, file_name)` を持つ構造体へ拡張する。
  ```rust
  pub struct AudioData {
      pub bytes: Vec<u8>,
      pub mime_type: &'static str,
      pub file_name: String,
  }
  ```
- 既存呼び出し側（録音ドメイン、転写サービス、テスト）を更新し、型互換性を維持する。

### 2. PCM → 圧縮フォーマット変換モジュールの追加
- `src/infrastructure/audio` 配下に `encoder` モジュールを追加し、PCM バッファから各種フォーマットに変換する責務を集約する。
- FLAC については `libflac` (FFI) や `free-lossless-audio-codec` などの Rust クレートを採用。
  - 変換処理は `Vec<i16>` (チャネル interleaved) を入力にし、ストリーム API で FLAC フレームをエンコードして `Vec<u8>` に蓄積。
  - サンプルレート・チャネル数は `MemoryRecordingState` から取得。
- Opus については後続拡張として `audiopus` などのクレートを検討し、適切なビットレート (例: 48kHz/mono で 32〜48 kbps) を指定できるようにする。
- Encoder モジュールは `enum AudioFormat { Wav, Flac, Opus }` を受け取り、`Result<AudioData, AudioEncodeError>` を返す統一インターフェースを提供する。

### 3. `CpalAudioBackend::stop_recording` の書き換え
- 既存の WAV ヘッダー生成を Encoder モジュールへ移行。
- 環境変数で指定された `AudioFormat` を参照し、PCM バッファを適切にエンコードして `AudioData` を返す。
- 例外時（エンコーダ初期化失敗など）は `AudioFormat::Wav` で再エンコードし直すフォールバックを実装。

### 4. OpenAI クライアントの更新
- `AudioData` の `mime_type` と `file_name` を利用し、`multipart::Part::bytes` に適切なメタデータを付与するよう変更する。
- API 仕様に従い、`audio/flac` / `audio/ogg` (`opus`) に対応。

### 5. 設定・ドキュメント反映
- `EnvConfig` に `VOICE_INPUT_AUDIO_FORMAT` を追加し、`enum AudioFormatConfig` へマッピング。
- README / docs に設定方法とフォーマット毎の特性を追記。
- 既定値を `flac` に設定し、互換性を意識した案内を明記。

### 6. テスト戦略
- ユニットテスト
  - PCM → WAV / FLAC 変換の出力バイト長・ヘッダー検証。
  - エンコード失敗時に WAV へフォールバックするケース。
- 統合テスト
  - `OpenAiClient::transcribe_audio` にモック HTTP サーバーを用意し、`Content-Type` とファイル名が期待通りになるか確認。
  - (Optional) エンドツーエンドテスト: 小さな音声サンプルをエンコードし、OpenAI API (またはモック) に投げる。

### 7. パフォーマンス評価
- エンコード処理の CPU 使用率とレイテンシを計測し、録音停止→送信までの時間を比較。
- 圧縮後サイズの統計を計測できるログ/メトリクスを追加し、可視化。

## ロールアウト計画
1. FLAC 実装を追加し、環境変数で明示的に `flac` を指定したユーザーでベータテスト。
2. 問題がなければデフォルトを `flac` に切り替え。
3. 追加で Opus 実装を導入し、用途に応じて選べるようにする。
4. 既存ユーザー向けにドキュメント更新とリリースノートで周知。
