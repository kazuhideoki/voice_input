//! メディア再生制御サービス
//!
//! # 責任
//! - Apple Musicの再生状態管理
//! - 録音時の自動一時停止/再開

use std::sync::{Arc, Mutex};

use crate::error::{Result, VoiceInputError};
use crate::infrastructure::external::sound::{pause_apple_music, resume_apple_music};
#[cfg(test)]
use async_trait::async_trait;

/// メディア制御の抽象化（テスト用）
#[cfg(test)]
#[async_trait]
pub(crate) trait MediaController: Send + Sync {
    /// Apple Musicが再生中かチェック
    async fn is_playing(&self) -> Result<bool>;

    /// Apple Musicを一時停止
    async fn pause(&self) -> Result<()>;

    /// Apple Musicを再生再開
    async fn resume(&self) -> Result<()>;
}

/// メディア制御サービス
pub struct MediaControlService {
    /// 録音による一時停止の所有セッションを記録
    pause_owner_session: Arc<Mutex<Option<u64>>>,
    /// メディアコントローラー（テスト時のモック用）
    #[cfg(test)]
    controller: Option<Box<dyn MediaController>>,
}

impl MediaControlService {
    /// 新しいMediaControlServiceを作成
    pub fn new() -> Self {
        Self {
            pause_owner_session: Arc::new(Mutex::new(None)),
            #[cfg(test)]
            controller: None,
        }
    }

    /// カスタムコントローラーで作成（テスト用）
    #[cfg(test)]
    pub(crate) fn with_controller(controller: Box<dyn MediaController>) -> Self {
        Self {
            pause_owner_session: Arc::new(Mutex::new(None)),
            controller: Some(controller),
        }
    }

    /// 再生中の場合は一時停止し、所有セッションを記録
    pub async fn pause_if_playing_for_session(&self, session_id: u64) -> Result<bool> {
        #[cfg(test)]
        {
            if let Some(ref controller) = self.controller {
                // モックコントローラーを使用
                if controller.is_playing().await? {
                    controller.pause().await?;
                    self.set_pause_owner_session(session_id)?;
                    return Ok(true);
                }
                return Ok(false);
            }
        }

        // 実際のApple Music制御を使用
        let was_playing = pause_apple_music().await;
        if was_playing {
            self.set_pause_owner_session(session_id)?;
        }
        Ok(was_playing)
    }

    fn set_pause_owner_session(&self, session_id: u64) -> Result<()> {
        let mut owner = self
            .pause_owner_session
            .lock()
            .map_err(|e| VoiceInputError::SystemError(format!("Lock error: {}", e)))?;
        match *owner {
            Some(current_owner) if current_owner > session_id => {}
            _ => *owner = Some(session_id),
        }
        Ok(())
    }

    /// 指定セッションが所有している一時停止のみ再開
    pub async fn resume_if_paused_for_session(&self, session_id: u64) -> Result<()> {
        let should_resume = {
            let mut owner = self
                .pause_owner_session
                .lock()
                .map_err(|e| VoiceInputError::SystemError(format!("Lock error: {}", e)))?;
            if *owner == Some(session_id) {
                *owner = None;
                true
            } else {
                false
            }
        };

        if should_resume {
            #[cfg(test)]
            {
                if let Some(ref controller) = self.controller {
                    // モックコントローラーを使用
                    controller.resume().await?;
                } else {
                    // 実際のApple Music制御を使用
                    resume_apple_music();
                }
            }

            #[cfg(not(test))]
            {
                // 実際のApple Music制御を使用
                resume_apple_music();
            }
        }

        Ok(())
    }

    /// 現在録音によって一時停止中かどうかを確認
    pub fn is_paused_by_recording(&self) -> Result<bool> {
        Ok(self
            .pause_owner_session
            .lock()
            .map_err(|e| VoiceInputError::SystemError(format!("Lock error: {}", e)))?
            .is_some())
    }

    /// 状態をリセット
    pub fn reset(&self) -> Result<()> {
        *self
            .pause_owner_session
            .lock()
            .map_err(|e| VoiceInputError::SystemError(format!("Lock error: {}", e)))? = None;
        Ok(())
    }
}

impl Default for MediaControlService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// テスト用のモックメディアコントローラー
    struct MockMediaController {
        playing: Arc<AtomicBool>,
    }

    impl MockMediaController {
        fn new(initial_playing: bool) -> Self {
            Self {
                playing: Arc::new(AtomicBool::new(initial_playing)),
            }
        }
    }

    #[async_trait]
    impl MediaController for MockMediaController {
        async fn is_playing(&self) -> Result<bool> {
            Ok(self.playing.load(Ordering::SeqCst))
        }

        async fn pause(&self) -> Result<()> {
            self.playing.store(false, Ordering::SeqCst);
            Ok(())
        }

        async fn resume(&self) -> Result<()> {
            self.playing.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    struct SequencedMediaController {
        playing: Arc<AtomicBool>,
        is_playing_results: Mutex<VecDeque<bool>>,
    }

    impl SequencedMediaController {
        fn new(initial_playing: bool, is_playing_results: Vec<bool>) -> Self {
            Self {
                playing: Arc::new(AtomicBool::new(initial_playing)),
                is_playing_results: Mutex::new(is_playing_results.into()),
            }
        }
    }

    #[async_trait]
    impl MediaController for SequencedMediaController {
        async fn is_playing(&self) -> Result<bool> {
            Ok(self
                .is_playing_results
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| self.playing.load(Ordering::SeqCst)))
        }

        async fn pause(&self) -> Result<()> {
            self.playing.store(false, Ordering::SeqCst);
            Ok(())
        }

        async fn resume(&self) -> Result<()> {
            self.playing.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    /// モックコントローラーで再生中なら一時停止し記録状態にする
    #[tokio::test]
    async fn pause_if_playing_pauses_and_marks_state() {
        let controller = Box::new(MockMediaController::new(true));
        let service = MediaControlService::with_controller(controller);

        let was_playing = service.pause_if_playing_for_session(1).await.unwrap();
        assert!(was_playing);
        assert!(service.is_paused_by_recording().unwrap());
    }

    /// モックコントローラーで再生中でなければ一時停止しない
    #[tokio::test]
    async fn pause_if_playing_noop_when_not_playing() {
        let controller = Box::new(MockMediaController::new(false));
        let service = MediaControlService::with_controller(controller);

        let was_playing = service.pause_if_playing_for_session(1).await.unwrap();
        assert!(!was_playing);
        assert!(!service.is_paused_by_recording().unwrap());
    }

    /// モックコントローラーで録音による一時停止状態なら再開できる
    #[tokio::test]
    async fn resume_if_paused_restores_playback() {
        let controller = Box::new(MockMediaController::new(true));
        let playing_ref = controller.playing.clone();
        let service = MediaControlService::with_controller(controller);

        // まず一時停止
        service.pause_if_playing_for_session(1).await.unwrap();
        assert!(!playing_ref.load(Ordering::SeqCst));

        // 再開
        service.resume_if_paused_for_session(1).await.unwrap();
        assert!(playing_ref.load(Ordering::SeqCst));
        assert!(!service.is_paused_by_recording().unwrap());
    }

    /// モックコントローラーで一時停止していない場合は再開が無視される
    #[tokio::test]
    async fn resume_if_paused_noop_when_not_paused() {
        let controller = Box::new(MockMediaController::new(false));
        let playing_ref = controller.playing.clone();
        let service = MediaControlService::with_controller(controller);

        // 再開を試みる（何も起こらないはず）
        service.resume_if_paused_for_session(1).await.unwrap();
        assert!(!playing_ref.load(Ordering::SeqCst));
    }

    /// 新しいセッション所有者がいる場合は古いセッションが再開できない
    #[tokio::test]
    async fn old_session_cannot_resume_newer_pause_owner() {
        let controller = Box::new(SequencedMediaController::new(true, vec![true, true]));
        let playing_ref = controller.playing.clone();
        let service = MediaControlService::with_controller(controller);

        service.pause_if_playing_for_session(1).await.unwrap();
        service.pause_if_playing_for_session(2).await.unwrap();
        service.resume_if_paused_for_session(1).await.unwrap();

        assert!(!playing_ref.load(Ordering::SeqCst));
        assert!(service.is_paused_by_recording().unwrap());

        service.resume_if_paused_for_session(2).await.unwrap();
        assert!(playing_ref.load(Ordering::SeqCst));
        assert!(!service.is_paused_by_recording().unwrap());
    }
}
