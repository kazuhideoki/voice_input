# Voice Input アプリケーション リファクタリング計画

## 📊 現状分析と提案の妥当性評価

### 現在のアーキテクチャ
- **レイヤードアーキテクチャ**: Application, Domain, Infrastructure の3層構造を採用
- **主要な問題点**: `voice_inputd.rs`（1098行）にビジネスロジックとオーケストレーションが集中
- **既存の良い点**: 一部のモジュール（StackService, Recorder）は適切に分離されている

### 提案された改善案の妥当性評価

| 提案 | 妥当性 | 理由 |
|------|--------|------|
| 責任の分離 | ⭐⭐⭐⭐⭐ | 必須。現状の1000行超えファイルは明らかに責任過多 |
| 統一的エラーハンドリング | ⭐⭐⭐⭐⭐ | 現状の混在は保守性を損なう。早急に対応すべき |
| 依存性注入 | ⭐⭐⭐⭐ | テスタビリティ向上に必須。特に外部APIのモック化に有効 |
| イベント駆動アーキテクチャ | ⭐⭐⭐ | 非同期処理が多く、疎結合化に有効。ただし複雑性が増す可能性あり |
| Actorパターンでの状態管理 | ⭐⭐⭐ | RefCell/Arcの混在解消に有効だが、学習コストを考慮する必要あり |
| 設定管理の一元化 | ⭐⭐⭐⭐ | 既にEnvConfigがあるが、より体系的な管理が必要 |
| テスト戦略の改善 | ⭐⭐⭐⭐⭐ | CI/CDの課題からも明らか。必須改善項目 |
| 型安全なRPC | ⭐⭐ | 現状のJSON-RPCでも十分機能している。優先度は低い |

## 🎯 コード管理性を重視した優先順位付け実装計画

### 基本方針
- **外部仕様は一切変更しない** - すべてのPhaseは純粋なリファクタリング
- **段階的な改善** - 各Phaseは独立してマージ可能
- **既存のレイヤー構造を活用** - Application層に新しいサービスを追加

### 依存関係に基づく実装順序
1. **Phase 1**: エラーハンドリング統一（基盤）
2. **Phase 2&3**: コア機能分離 + 依存性注入（同時実施）

### Phase 1: 統一的エラーハンドリング（2-3日）【最初に実施】
**目的**: 後続のすべてのPhaseで使用する基盤を整備

#### なぜ最初に実施するか
- 他のすべてのPhaseでResult型を使用するため
- リファクタリング中のエラー処理を一貫させるため
- 既存コードへの影響が最小限で、独立して実施可能

#### 意図
- **エラーの追跡性**: エラーの発生源と伝播経路を明確化
- **一貫したAPI**: すべてのモジュールで同じResult型を使用
- **適切なエラー変換**: 下位層のエラーを上位層で適切に変換

#### 実装方法
1. **thiserrorクレートの導入理由**
   - `#[from]`属性で自動的なエラー変換を生成
   - Display traitの自動実装でエラーメッセージを統一
   - エラーチェーンの自然な表現
   - Rust標準のError traitを適切に実装

2. **統一エラー型の定義**
   ```rust
   // src/error.rs
   use thiserror::Error;
   
   #[derive(Debug, Error)]
   pub enum VoiceInputError {
       #[error("Audio recording error: {0}")]
       Recording(String),
       
       #[error("Transcription failed: {0}")]
       Transcription(String),
       
       #[error("Stack operation failed")]
       Stack(#[from] StackServiceError),
       
       #[error("IPC communication error")]
       Ipc(#[from] std::io::Error),
       
       #[error("Configuration error: {0}")]
       Config(String),
       
       #[error("Permission denied: {reason}")]
       Permission { reason: String },
   }
   
   pub type Result<T> = std::result::Result<T, VoiceInputError>;
   ```

3. **既存コードの段階的移行**
   ```rust
   // Before: 文字列エラー
   if recorder.is_recording() {
       return Err("Already recording".to_string());
   }
   
   // After: 型付きエラー
   if recorder.is_recording() {
       return Err(VoiceInputError::Recording("Already recording".to_string()));
   }
   ```

### Phase 2&3: コア機能の分離 + 依存性注入（7-10日）【同時実施】
**目的**: 1098行の`voice_inputd.rs`を分割しつつ、テスト可能な構造に

#### なぜ同時実施するか
- 機能を分離する際に、最初から依存性注入を考慮した設計にする方が効率的
- 二度手間を避け、一度で適切な抽象化を実現
- テスト可能な構造を最初から組み込める

#### 意図
- **単一責任の原則**: 各モジュールが1つの明確な責任を持つ
- **テスト容易性**: 外部依存を抽象化し、モックでテスト可能に
- **理解容易性**: 新規開発者がコードを理解しやすく

#### 実装方法
1. **抽象化トレイトの配置（Application層）**
   ```rust
   // src/application/traits.rs
   // 意図：Application層で外部依存のインターフェースを定義
   // Domain層は純粋なビジネスロジックのみを持つため、外部依存のtraitはApplication層に配置
   
   #[async_trait]
   pub trait AudioRecorder: Send + Sync {
       async fn start(&mut self) -> Result<()>;
       async fn stop(&mut self) -> Result<Vec<u8>>;
       fn is_recording(&self) -> bool;
   }
   
   #[async_trait]
   pub trait TranscriptionClient: Send + Sync {
       async fn transcribe(&self, audio: &[u8], lang: &str) -> Result<String>;
   }
   
   #[async_trait]
   pub trait TextInputClient: Send + Sync {
       async fn input_text(&self, text: &str) -> Result<()>;
   }
   ```

2. **既存のapplication層への配置**
   ```
   src/application/
   ├── mod.rs                      # 既存
   ├── stack_service.rs            # 既存
   ├── traits.rs                   # 新規：外部依存の抽象化
   ├── recording_service.rs        # 新規：録音管理
   ├── transcription_service.rs    # 新規：音声認識
   └── command_handler.rs          # 新規：コマンド処理の統合
   ```

3. **録音管理サービスの抽出（RefCell/Rc維持）**
   ```rust
   // src/application/recording_service.rs
   use std::rc::Rc;
   use std::cell::RefCell;
   
   pub struct RecordingService {
       recorder: Rc<RefCell<Recorder>>,  // 既存の構造を維持
       state: Rc<RefCell<RecordingState>>,
       config: RecordingConfig,
   }
   
   impl RecordingService {
       pub fn new(recorder: Rc<RefCell<Recorder>>, config: RecordingConfig) -> Self {
           Self {
               recorder,
               state: Rc::new(RefCell::new(RecordingState::Idle)),
               config,
           }
       }
       
       pub async fn start_recording(&self, options: RecordingOptions) -> Result<SessionId> {
           // voice_inputd.rsから録音関連のロジックを移動
           let session_id = SessionId::new();
           self.recorder.borrow_mut().start().await?;
           *self.state.borrow_mut() = RecordingState::Recording(session_id);
           Ok(session_id)
       }
   }
   ```

4. **転写サービスの抽出（依存性注入対応）**
   ```rust
   // src/application/transcription_service.rs
   use super::traits::TranscriptionClient;
   
   pub struct TranscriptionService {
       client: Box<dyn TranscriptionClient>,  // 抽象化されたインターフェース
       dict_service: DictionaryService,
       semaphore: Arc<Semaphore>,
   }
   
   impl TranscriptionService {
       pub fn new(client: Box<dyn TranscriptionClient>, dict_service: DictionaryService) -> Self {
           Self {
               client,
               dict_service,
               semaphore: Arc::new(Semaphore::new(3)), // 並行数制限
           }
       }
       
       pub async fn transcribe(&self, audio: Vec<u8>, options: TranscriptionOptions) -> Result<String> {
           let _permit = self.semaphore.acquire().await?;
           let text = self.client.transcribe(&audio, &options.language).await?;
           let processed = self.dict_service.process(text);
           Ok(processed)
       }
   }
   ```

5. **コマンドハンドラーの統合（RefCell/Rc版）**
   ```rust
   // src/application/command_handler.rs
   pub struct CommandHandler {
       recording: Rc<RefCell<RecordingService>>,
       transcription: Rc<RefCell<TranscriptionService>>,
       stack: Rc<RefCell<StackService>>,
       media_control: Rc<RefCell<MediaControlService>>,
       ui_manager: Rc<RefCell<UiProcessManager>>,
   }
   
   impl CommandHandler {
       pub fn new(
           recording: Rc<RefCell<RecordingService>>,
           transcription: Rc<RefCell<TranscriptionService>>,
           stack: Rc<RefCell<StackService>>,
           media_control: Rc<RefCell<MediaControlService>>,
           ui_manager: Rc<RefCell<UiProcessManager>>,
       ) -> Self {
           Self { recording, transcription, stack, media_control, ui_manager }
       }
       
       pub async fn handle(&self, cmd: Command) -> Result<Response> {
           match cmd {
               Command::StartRecording(opts) => {
                   // Apple Music一時停止
                   self.media_control.borrow().pause_if_playing().await?;
                   
                   let session_id = self.recording.borrow().start_recording(opts).await?;
                   Ok(Response::RecordingStarted(session_id))
               }
               // 各コマンドを適切なサービスに委譲
           }
       }
   }
   ```

6. **サービスコンテナによる依存関係管理（RefCell/Rc版）**
   ```rust
   // src/application/service_container.rs
   // 意図：すべての依存関係を一箇所で組み立て、main関数から各所へ配布
   
   pub struct ServiceContainer {
       pub command_handler: Rc<RefCell<CommandHandler>>,
       pub shortcut_service: Rc<RefCell<ShortcutKeyService>>, // 独立ワーカー用
   }
   
   impl ServiceContainer {
       pub fn new(config: AppConfig) -> Result<Self> {
           // 本番用の依存関係を構築
           let recorder = Rc::new(RefCell::new(Recorder::new(config.recording.clone())?));
           let transcription_client = Box::new(OpenAiClient::new(config.env.openai_api_key.clone())?);
           
           Self::with_dependencies(config, recorder, transcription_client)
       }
       
       // テストや特殊な設定用に依存関係を注入可能
       pub fn with_dependencies(
           config: AppConfig,
           recorder: Rc<RefCell<Recorder>>,
           transcription_client: Box<dyn TranscriptionClient>,
       ) -> Result<Self> {
           // サービスを組み立て
           let recording = Rc::new(RefCell::new(RecordingService::new(recorder, config.recording)));
           let transcription = Rc::new(RefCell::new(TranscriptionService::new(
               transcription_client,
               DictionaryService::new()
           )));
           let stack = Rc::new(RefCell::new(StackService::new()));
           let media_control = Rc::new(RefCell::new(MediaControlService::new()));
           let ui_manager = Rc::new(RefCell::new(UiProcessManager::new()));
           
           let command_handler = Rc::new(RefCell::new(CommandHandler::new(
               recording,
               transcription,
               stack,
               media_control,
               ui_manager,
           )));
           
           let shortcut_service = Rc::new(RefCell::new(ShortcutKeyService::new()));
           
           Ok(ServiceContainer { command_handler, shortcut_service })
       }
   }
   ```

7. **main関数での初期化と配布（RefCell/Rc版）**
   ```rust
   // src/bin/voice_inputd.rs
   #[tokio::main(flavor = "current_thread")]
   async fn main() -> Result<()> {
       // 設定を一度だけ読み込み
       let config = AppConfig::load()?;
       
       // サービスコンテナの初期化（ここで全依存関係を構築）
       let container = ServiceContainer::new(config)?;
       
       // ショートカットワーカーの起動（独立したまま）
       let shortcut_service = container.shortcut_service.clone();
       tokio::task::spawn_local(async move {
           shortcut_worker(shortcut_service).await
       });
       
       // Unix Domain Socketの設定
       let listener = UnixListener::bind("/tmp/voice_input.sock")?;
       
       // メインループ
       loop {
           let (stream, _) = listener.accept().await?;
           
           // Rc経由でCommandHandlerを各接続に渡す
           let handler = container.command_handler.clone();
           
           tokio::task::spawn_local(async move {
               handle_client(stream, handler).await
           });
       }
   }
   
   async fn handle_client(stream: UnixStream, handler: Rc<RefCell<CommandHandler>>) -> Result<()> {
       // IPCコマンドを読み取り、CommandHandlerに委譲
       let cmd = read_command(&stream).await?;
       let response = handler.borrow().handle(cmd).await?;
       write_response(&stream, response).await?;
       Ok(())
   }
   ```

## 📝 追加のコード管理性向上施策

### 1. **モジュール構造の明確化**
```rust
// src/lib.rs でpublicインターフェースを明示
pub mod application {
    pub use self::command_handler::CommandHandler;
    pub use self::recording_service::RecordingService;
    pub use self::transcription_service::TranscriptionService;
    pub use self::traits::{AudioRecorder, TranscriptionClient};
}

pub mod domain {
    pub use self::stack::Stack;
    pub use self::recorder::Recorder;
}
```

### 2. **ドキュメントコメントの充実**
```rust
/// 音声録音を管理するサービス
/// 
/// # 責任
/// - 録音の開始・停止
/// - 録音状態の管理
/// - Apple Music の一時停止/再開
/// 
/// # Example
/// ```
/// let service = RecordingService::new(recorder);
/// let session_id = service.start_recording(options).await?;
/// let audio_data = service.stop_recording(session_id).await?;
/// ```
pub struct RecordingService { /* ... */ }
```

### 3. **型エイリアスによる意図の明確化**
```rust
// src/types.rs
pub type SessionId = Uuid;
pub type StackId = u32;
pub type AudioData = Vec<u8>;
pub type Milliseconds = u64;
```

## 🔧 実装上の方針決定

### 1. **RefCell/Rc → Arc/Mutex移行**
- **方針**: 現状のRefCell/Rcを維持
- **理由**: single-threaded runtimeと整合性が取れており、変更不要

### 2. **既存エラー型の統合**
- **方針**: 既存型はそのまま、VoiceInputErrorに`#[from]`で自動変換
- **理由**: 段階的移行で十分、既存コードへの影響最小限

### 3. **録音状態管理（RecCtx）**
- **方針**: RecCtxはそのままRecordingServiceに移動
- **理由**: 内部構造の変更は必要時に実施

### 4. **ショートカットキーサービス**
- **方針**: 独立したワーカーのまま維持
- **理由**: 現在正常動作しているため変更不要

### 5. **Apple Music制御**
- **方針**: 別サービス（MediaControlService）として分離
- **理由**: 録音と音楽再生制御は責任が異なる
```rust
// src/application/media_control_service.rs
pub struct MediaControlService {
    paused_by_recording: Arc<Mutex<bool>>,
}

impl MediaControlService {
    pub async fn pause_if_playing(&self) -> Result<()> {
        // Apple Music制御ロジック
    }
    
    pub async fn resume_if_paused(&self) -> Result<()> {
        // 再開ロジック
    }
}
```

### 6. **テスト環境の切り替え**
- **方針**: TEST_MODE環境変数で継続
- **理由**: 既存方式でシンプルかつ十分機能する

### 7. **モック配置場所**
- **方針**: `tests/common/mocks/`に配置
- **理由**: テスト間で共有でき、本番バイナリに含まれない

## 📅 実装スケジュール（コード管理性重視版）

| Phase | 期間 | 効果 | 依存関係 |
|-------|------|------|----------|
| Phase 1: エラーハンドリング | 2-3日 | ⭐⭐⭐⭐ | なし（最初に実施） |
| Phase 2&3: コア機能分離＋依存性注入 | 7-10日 | ⭐⭐⭐⭐⭐ | Phase 1完了後 |

**合計**: 9-13日（約2週間）

## 🎯 成功指標

1. **コード可読性**
   - 各ファイル500行以下
   - 各関数50行以下
   - 認知的複雑度10以下

2. **保守性**
   - 新機能追加時の変更ファイル数が3以下
   - バグ修正の平均時間50%削減
   - コードレビュー時間30%削減

3. **テスト**
   - ユニットテストカバレッジ80%以上
   - 統合テストの実行時間5分以内
   - CI失敗率10%以下

## 📝 追加のコード管理性向上施策

### 1. **モジュール構造の明確化**
```rust
// src/lib.rs でpublicインターフェースを明示
pub mod application {
    pub use self::command_handler::CommandHandler;
    pub use self::recording_service::RecordingService;
    pub use self::transcription_service::TranscriptionService;
}

pub mod domain {
    pub use self::events::DomainEvent;
    pub use self::traits::{AudioRecorder, TranscriptionClient};
}
```

### 2. **ドキュメントコメントの充実**
```rust
/// 音声録音を管理するサービス
/// 
/// # 責任
/// - 録音の開始・停止
/// - 録音状態の管理
/// - Apple Music の一時停止/再開
/// 
/// # Example
/// ```
/// let service = RecordingService::new(recorder);
/// let session_id = service.start_recording(options).await?;
/// let audio_data = service.stop_recording(session_id).await?;
/// ```
pub struct RecordingService { /* ... */ }
```

### 3. **型エイリアスによる意図の明確化**
```rust
// src/types.rs
pub type SessionId = Uuid;
pub type StackId = u32;
pub type AudioData = Vec<u8>;
pub type Milliseconds = u64;
```


## 🎯 成功指標

1. **コード可読性**
   - 各ファイル500行以下
   - 各関数50行以下
   - 認知的複雑度10以下

2. **保守性**
   - 新機能追加時の変更ファイル数が3以下
   - バグ修正の平均時間50%削減
   - コードレビュー時間30%削減

3. **テスト**
   - ユニットテストカバレッジ80%以上
   - 統合テストの実行時間5分以内
   - CI失敗率10%以下

