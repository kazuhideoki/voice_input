//! 辞書インフラ層まとめ
pub mod json_repo;
mod migration;
pub use json_repo::JsonFileDictRepo;

/// 診断用に辞書を検証する。ファイルの作成や移行は行わない。
pub fn validate_read_only(path: &std::path::Path) -> std::io::Result<()> {
    migration::validate_read_only(path)
}
