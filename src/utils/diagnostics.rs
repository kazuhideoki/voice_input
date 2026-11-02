//! 常時有効な処理メタデータの診断ログ。音声や転写本文は記録しない。

use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Value, json};

/// プロセス内で重複しない診断用IDを発行する。
pub fn next_id() -> u64 {
    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

/// 時刻とプロセスIDを付けたJSONメタデータを標準エラーへ記録する。
pub fn log(event: &str, fields: Value) {
    eprintln!(
        "VOICE_INPUT_DIAG {}",
        json!({
            "at": chrono::Utc::now().to_rfc3339(),
            "pid": std::process::id(),
            "event": event,
            "fields": fields,
        })
    );
}
