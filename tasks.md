# Voice‑Input モジュール再設計ドキュメント

> **スコープ** > _録音→転写 CLI ツール_ をレイヤ分離しつつ **既存の UX と挙動を一切変えない**。外部クレートは増やさず (`anyhow`, `thiserror` 等は **導入しない**)。

---

## 1. ガイドライン

| 項目           | 方針                                                        |
| -------------- | ----------------------------------------------------------- |
| 依存クレート   | **追加しない**。標準ライブラリ＋既存依存のみ使用            |
| エラー扱い     | これまでどおり `Box<dyn std::error::Error>` で伝搬          |
| パブリック API | 既存 CLI 引数・戻り値・ファイル I/O を保持                  |
| OS 互換性      | まず macOS のみ。クロスプラットフォーム化は別イテレーション |
| PR 粒度        | _フェーズ×ステップ_ ごとに小さくマージ                      |

---

## 2. 新ディレクトリ構造

```
crate/
├── bin/
│   └── voice_input.rs      # エントリポイント (極小)
└── src/
    ├── cli/                # CLI ハンドラ (record / transcribe)
    ├── domain/             # ロジック (Recorder, Transcriber)
    ├── infrastructure/
    │   ├── audio/
    │   │   ├── mod.rs      # trait AudioBackend
    │   │   └── cpal_backend.rs
    │   └── external/
    │       ├── openai.rs   # REST 呼び出し
    │       ├── sound.rs    # 効果音
    │       └── clipboard.rs
    ├── utils/
    │   └── detach.rs
    └── lib.rs              # pub use のみ
```

> **命名修正**: 旧 `audio_recoder.rs` → `cpal_backend.rs` ("recorder" に綴り訂正)

---

## 3. 実装ステップ

### Phase 1 : ファイル移動 & 雛形

1. `mkdir -p` で新ディレクトリ生成。
2. `git mv` でファイルを移設し命名修正。コンパイルが壊れたままで OK。
3. `src/lib.rs` にモジュールツリーを宣言。

### Phase 2 : AudioBackend 抽出

1. `src/infrastructure/audio/mod.rs` に trait `AudioBackend` を定義。
2. 旧ロジックを `cpal_backend.rs` に移し、`impl AudioBackend` で包む。
3. thread‑local を struct フィールドへ移行（外部 API 変化なし）。

### Phase 3 : ドメイン層 Recorder／Transcriber

1. `domain/recorder.rs` で純粋ロジックを実装。
2. `domain/transcriber.rs` に OpenAI 呼び出し & WAV → Text 処理を移動。
   _OpenAI 通信自体は次ステップで external に差し替え_

### Phase 4 : Infrastructure 外部連携

1. `external/openai.rs` に HTTP 呼び出し関数を移設。ドメインからは関数呼び出しのみ。
2. `sound.rs` / `clipboard.rs` を抽象化（trait optional）。

### Phase 5 : CLI 層薄型化

1. `cli/record.rs`, `cli/transcribe.rs` を実装し、旧 `main.rs` のロジックを分解。
2. `bin/voice_input.rs` は `voice_input::cli::run()` を呼ぶだけにする。

### Phase 6 : ビルド & 動作確認

```bash
cargo build --release
./target/release/voice_input record
./target/release/voice_input transcribe --wav sample.wav
```

動作が旧バイナリと一致することを確認。

---

## 4. テスト計画 (任意)

- **単体テスト** : `MockAudioBackend` を in‑memory 実装し、`domain::recorder` 動作検証。
- **結合テスト** : `assert_cmd` は既存クレートなので利用可。CLI フローを ―wav スタブ― で確認。

> _外部クレートを増やさずに_ モックを自作する場合は、`cfg(test)` 内に簡易 struct を定義する。

---

## 5. PR 分割例

| PR  | 対応内容                                                  |
| --- | --------------------------------------------------------- |
| #1  | Phase 1 — レイアウト作成 & ファイル移動 (ビルド壊れて OK) |
| #2  | Phase 2 — AudioBackend 抽出 & ビルド復旧                  |
| #3  | Phase 3 — domain 層の導入                                 |
| #4  | Phase 4 — external 分離                                   |
| #5  | Phase 5 — CLI リファクタ & 最終確認                       |

---

## 6. 完了基準

- [ ] `cargo build` が通る（追加依存なし）
- [ ] 既存コマンドオプション・挙動が完全一致
- [ ] `git diff --stat` が PR 粒度ごとに適切
- [ ] README に新構造図 & ビルド方法を追記
