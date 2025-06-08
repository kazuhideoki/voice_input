use voice_input::{
    domain::dict::DictRepository, infrastructure::dict::JsonFileDictRepo, utils::config::EnvConfig,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 環境変数設定を初期化
    EnvConfig::init()?;
    let repo = JsonFileDictRepo::new();
    let list = repo.load()?;
    repo.save(&list)?; // save again to write new status field
    println!("✅ dictionary migrated ({} entries)", list.len());
    Ok(())
}
