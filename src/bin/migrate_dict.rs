use voice_input::{domain::dict::DictRepository, infrastructure::dict::JsonFileDictRepo};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let repo = JsonFileDictRepo::new();
    let list = repo.load()?;
    repo.save(&list)?; // save again to write new status field
    println!("✅ dictionary migrated ({} entries)", list.len());
    Ok(())
}
