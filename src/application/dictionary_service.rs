use crate::domain::dict::{
    DictTerm, DictVariant, DictionaryConflictError, DictionaryDocument, WordEntry,
};
use std::io;

/// 辞書永続化 port
pub trait DictRepository: Send + Sync {
    fn load(&self) -> io::Result<Vec<WordEntry>>;
    fn save(&self, all: &[WordEntry]) -> io::Result<()>;

    /// v2 辞書ドキュメントを読み込む。
    fn load_dictionary(&self) -> io::Result<DictionaryDocument> {
        DictionaryDocument::from_word_entries(self.load()?)
            .map_err(dictionary_conflict_to_invalid_data)
    }

    /// v2 辞書ドキュメントを保存する。
    fn save_dictionary(&self, document: &DictionaryDocument) -> io::Result<()> {
        self.save(&document.to_word_entries())
    }
}

/// 辞書更新ユースケース
pub struct DictionaryService {
    repo: Box<dyn DictRepository>,
}

/// 候補追加時に対象語句が新規作成されたかを表す結果。
pub struct AddVariantResult {
    pub term_created: bool,
}

impl DictionaryService {
    /// リポジトリを注入して新しいサービスを作成。
    pub fn new(repo: Box<dyn DictRepository>) -> Self {
        Self { repo }
    }

    /// 辞書一覧を取得。
    pub fn list(&self) -> io::Result<DictionaryDocument> {
        self.repo.load_dictionary()
    }

    /// 対象語句を追加する。
    pub fn add_term(&self, term: &str) -> io::Result<()> {
        let mut document = self.repo.load_dictionary()?;
        ensure_term(&mut document, term);
        self.repo.save_dictionary(&document)
    }

    /// 対象語句へ変換する候補を追加する。
    pub fn add_variant(&self, term: &str, surface: &str) -> io::Result<AddVariantResult> {
        let mut document = self.repo.load_dictionary()?;
        ensure_variant_is_available(&document, term, surface)?;
        let term_created = !document.terms.iter().any(|entry| entry.term == term);
        let term_entry = ensure_term(&mut document, term);
        if !term_entry
            .variants
            .iter()
            .any(|variant| variant.surface == surface)
        {
            term_entry.variants.push(DictVariant {
                surface: surface.to_string(),
                hit: 0,
            });
        }
        self.repo.save_dictionary(&document)?;
        Ok(AddVariantResult { term_created })
    }

    /// 対象語句を削除する。戻り値 true=削除した / false=見つからず。
    pub fn delete_term(&self, term: &str) -> io::Result<bool> {
        let mut document = self.repo.load_dictionary()?;
        let len_before = document.terms.len();
        document.terms.retain(|entry| entry.term != term);
        let deleted = len_before != document.terms.len();
        if deleted {
            self.repo.save_dictionary(&document)?;
        }
        Ok(deleted)
    }

    /// 対象語句から候補を削除する。戻り値 true=削除した / false=見つからず。
    pub fn delete_variant(&self, term: &str, surface: &str) -> io::Result<bool> {
        let mut document = self.repo.load_dictionary()?;
        let Some(term_entry) = document.terms.iter_mut().find(|entry| entry.term == term) else {
            return Ok(false);
        };
        let len_before = term_entry.variants.len();
        term_entry
            .variants
            .retain(|variant| variant.surface != surface);
        let deleted = len_before != term_entry.variants.len();
        if deleted {
            self.repo.save_dictionary(&document)?;
        }
        Ok(deleted)
    }
}

fn ensure_term<'a>(document: &'a mut DictionaryDocument, term: &str) -> &'a mut DictTerm {
    if let Some(index) = document.terms.iter().position(|entry| entry.term == term) {
        return &mut document.terms[index];
    }
    document.terms.push(DictTerm {
        term: term.to_string(),
        variants: Vec::new(),
    });
    document.terms.last_mut().expect("term was just pushed")
}

fn ensure_variant_is_available(
    document: &DictionaryDocument,
    term: &str,
    surface: &str,
) -> io::Result<()> {
    if let Some(existing_term) = document.terms.iter().find(|entry| {
        entry.term != term
            && entry
                .variants
                .iter()
                .any(|variant| variant.surface == surface)
    }) {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!(
                "dictionary variant {:?} is already assigned to {:?}",
                surface, existing_term.term
            ),
        ));
    }
    Ok(())
}

fn dictionary_conflict_to_invalid_data(error: DictionaryConflictError) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct InMemoryDictRepo {
        document: Mutex<DictionaryDocument>,
    }

    impl InMemoryDictRepo {
        fn new(document: DictionaryDocument) -> Self {
            Self {
                document: Mutex::new(document),
            }
        }
    }

    impl DictRepository for InMemoryDictRepo {
        fn load(&self) -> io::Result<Vec<WordEntry>> {
            Ok(self.document.lock().unwrap().to_word_entries())
        }

        fn save(&self, all: &[WordEntry]) -> io::Result<()> {
            *self.document.lock().unwrap() =
                DictionaryDocument::from_word_entries(all.to_vec()).unwrap();
            Ok(())
        }

        fn load_dictionary(&self) -> io::Result<DictionaryDocument> {
            Ok(self.document.lock().unwrap().clone())
        }

        fn save_dictionary(&self, document: &DictionaryDocument) -> io::Result<()> {
            *self.document.lock().unwrap() = document.clone();
            Ok(())
        }
    }

    /// 対象語句と候補を追加できる
    #[test]
    fn add_term_and_variant_registers_dictionary_term() {
        let service =
            DictionaryService::new(Box::new(InMemoryDictRepo::new(DictionaryDocument::empty())));

        service.add_term("bar").expect("add term");
        let result = service.add_variant("bar", "foo").expect("add variant");

        let loaded = service.list().expect("load");
        assert!(!result.term_created);
        assert_eq!(loaded.terms.len(), 1);
        assert_eq!(loaded.terms[0].term, "bar");
        assert_eq!(loaded.terms[0].variants[0].surface, "foo");
    }

    /// 対象語句が未登録でも候補追加と同時に対象語句を作成できる
    #[test]
    fn add_variant_creates_missing_dictionary_term() {
        let service =
            DictionaryService::new(Box::new(InMemoryDictRepo::new(DictionaryDocument::empty())));

        let result = service.add_variant("bar", "foo").expect("add variant");

        let loaded = service.list().expect("load");
        assert!(result.term_created);
        assert_eq!(loaded.terms.len(), 1);
        assert_eq!(loaded.terms[0].term, "bar");
        assert_eq!(loaded.terms[0].variants[0].surface, "foo");
    }

    /// 対象語句と候補を削除できる
    #[test]
    fn delete_term_and_variant_removes_dictionary_items() {
        let service = DictionaryService::new(Box::new(InMemoryDictRepo::new(DictionaryDocument {
            version: crate::domain::dict::CURRENT_DICTIONARY_VERSION,
            terms: vec![DictTerm {
                term: "bar".into(),
                variants: vec![DictVariant {
                    surface: "foo".into(),
                    hit: 0,
                }],
            }],
        })));

        assert!(
            service
                .delete_variant("bar", "foo")
                .expect("delete variant")
        );
        assert!(
            service
                .list()
                .expect("load")
                .terms
                .first()
                .expect("term")
                .variants
                .is_empty()
        );
        assert!(service.delete_term("bar").expect("delete term"));
        assert!(service.list().expect("load").terms.is_empty());
    }
}
