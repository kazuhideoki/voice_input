# カーソル位置へのテキスト挿入機能：詳細設計と実装手順

## 1. Why (目的)

現在のシステムは音声認識により得られたテキストをクリップボードにコピーし、AppleScript を使用して Command+V のキーボードショートカットを送信することでテキストを挿入しています。この方法には以下の課題があります：

1. **クリップボード履歴の汚染** - 音声入力のたびにユーザーのクリップボード履歴が上書きされる
2. **ペースト操作の認識問題** - アプリケーションによってはペーストコマンドの処理方法が異なる場合がある
3. **コンテキスト切り替え** - クリップボード経由の操作は、特定のコンテキストで問題を引き起こす可能性がある
4. **パフォーマンスオーバーヘッド** - AppleScript 経由のキーボードショートカット送信にはわずかな遅延が発生する

カーソル位置へテキストを直接挿入する機能を実装することで、これらの問題を解決し、よりシームレスで効率的なユーザー体験を提供します。

## 2. What (要件)

カーソル位置へのテキスト挿入機能は以下の要件を満たす必要があります：

1. **クリップボード非依存** - ユーザーのクリップボード履歴を変更せずにテキスト挿入が可能であること
2. **アプリケーション互換性** - 主要な macOS アプリケーション（テキストエディタ、ブラウザ、オフィスアプリケーションなど）で動作すること
3. **パフォーマンス** - テキスト挿入の遅延を最小限に抑えること
4. **信頼性** - さまざまな状況下でも確実にテキストが挿入されること
5. **アクセシビリティ** - macOS のアクセシビリティ権限内で動作すること
6. **設定オプション** - ユーザーが従来のクリップボード方式と新しい直接挿入方式を選択できること
7. **エラーハンドリング** - テキスト挿入に失敗した場合の適切なフォールバック処理を提供すること

## 3. How (実装アプローチ)

実装可能な複数のアプローチを検討し、それぞれの長所と短所を評価します。

### 3.1 実装オプション

#### A. Quartz Event Services（キーボードイベント送信）

Quartz Event Services は macOS の低レベルイベント処理システムであり、キーボードイベントをプログラム的に生成し送信することができます。

```rust
use core_graphics::event::{CGEvent, CGEventFlags, CGEventSource, CGEventSourceStateID, CGKeyCode};
use core_graphics::event_source::CGEventSource;

pub fn insert_text_with_keyboard_events(text: &str) -> Result<(), String> {
    // イベントソースの作成
    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|_| "Failed to create event source".to_string())?;
    
    // 各文字を個別のキーイベントとして送信
    for c in text.chars() {
        // 文字をキーコードに変換（実際の実装ではより複雑になる）
        let key_code = char_to_key_code(c)?;
        
        // キーダウンイベントの作成と送信
        let key_down = CGEvent::new_keyboard_event(source.clone(), key_code, true)
            .map_err(|_| format!("Failed to create key down event for '{}')", c))?;
        key_down.post(CGEventTapLocation::HID);
        
        // キーアップイベントの作成と送信
        let key_up = CGEvent::new_keyboard_event(source.clone(), key_code, false)
            .map_err(|_| format!("Failed to create key up event for '{}')", c))?;
        key_up.post(CGEventTapLocation::HID);
        
        // 短い遅延を入れる（オプション）
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    
    Ok(())
}

// 文字をCGKeyCodeに変換するヘルパー関数
fn char_to_key_code(c: char) -> Result<CGKeyCode, String> {
    // 実装には詳細なキーマッピングが必要
    // ...
}
```

**長所:**
- 多くのアプリケーションで動作する汎用的なアプローチ
- アプリケーション固有のAPIに依存しない
- 実際のキーボード入力と同等の動作

**短所:**
- 非ASCII文字のマッピングが複雑
- キーボードレイアウトに依存する可能性がある
- 文字ごとの送信による遅延
- アクセシビリティ権限が必要

#### B. Accessibility API（AXUIElement）

AppleのAccessibility APIを使用して、現在フォーカスされているテキスト入力要素を直接操作します。

```rust
use cocoa_foundation::base::{id, nil};
use core_foundation::base::TCFType;
use core_foundation::string::CFString;
use core_foundation::dictionary::CFDictionary;
use objc::{msg_send, sel, sel_impl};

pub fn insert_text_with_accessibility_api(text: &str) -> Result<(), String> {
    unsafe {
        // システム全体のアクセシビリティオブジェクトを取得
        let system_wide = AXUIElementCreateSystemWide();
        
        // フォーカスされている要素を取得
        let mut focused_element: CFTypeRef = std::ptr::null();
        let result = AXUIElementCopyAttributeValue(
            system_wide,
            kAXFocusedUIElementAttribute as CFStringRef,
            &mut focused_element,
        );
        
        if result != 0 || focused_element.is_null() {
            return Err("Failed to get focused element".to_string());
        }
        
        // 要素が編集可能かチェック
        let mut is_editable: DarwinBoolean = 0;
        AXUIElementCopyAttributeValue(
            focused_element as AXUIElementRef,
            kAXEditableAttribute as CFStringRef,
            &mut is_editable as *mut _ as *mut CFTypeRef,
        );
        
        if is_editable == 0 {
            return Err("Focused element is not editable".to_string());
        }
        
        // 現在の値を取得
        let mut current_value: CFTypeRef = std::ptr::null();
        AXUIElementCopyAttributeValue(
            focused_element as AXUIElementRef,
            kAXValueAttribute as CFStringRef,
            &mut current_value,
        );
        
        // 選択範囲を取得
        let mut selection_range: CFTypeRef = std::ptr::null();
        AXUIElementCopyAttributeValue(
            focused_element as AXUIElementRef,
            kAXSelectedTextRangeAttribute as CFStringRef,
            &mut selection_range,
        );
        
        // テキスト挿入位置および選択範囲情報を使用して新しいテキストを挿入
        // （実際の実装ではさらに詳細な処理が必要）
        
        // 新しい値を設定
        let cf_text = CFString::new(text);
        AXUIElementSetAttributeValue(
            focused_element as AXUIElementRef,
            kAXValueAttribute as CFStringRef,
            cf_text.as_concrete_TypeRef() as CFTypeRef,
        );
        
        // リソース解放
        CFRelease(system_wide as CFTypeRef);
        if !focused_element.is_null() {
            CFRelease(focused_element);
        }
        if !current_value.is_null() {
            CFRelease(current_value);
        }
        if !selection_range.is_null() {
            CFRelease(selection_range);
        }
        
        Ok(())
    }
}
```

**長所:**
- 一度に全テキストを設定するため効率的
- 選択範囲の置換も可能
- 国際文字やマークアップテキストも挿入可能

**短所:**
- すべてのアプリケーションがアクセシビリティAPIをサポートしているわけではない
- アクセシビリティ権限が必要
- 実装が複雑

#### C. IMEインテグレーション（Input Method Kit）

macOSの入力メソッドフレームワークを利用して、カスタム入力メソッドとして実装する高度なアプローチ。

**長所:**
- ネイティブなテキスト入力と同等の機能
- 最高レベルの互換性

**短所:**
- 実装が非常に複雑
- 既存アプリとは異なるアーキテクチャが必要
- 開発および保守コストが高い

### 3.2 推奨アプローチ

実現可能性、互換性、開発コストを考慮すると、**Quartz Event Services** を使用したキーボードイベント送信が最も適切な初期実装と考えられます。以下の理由からこのアプローチを推奨します：

1. より広範なアプリケーション互換性
2. 実装の複雑さが比較的低い
3. 既存コードベースとの親和性が高い
4. フォールバックメカニズムとして既存のクリップボード方式を維持可能

ただし、Quartz Event Servicesの制限（特に非ASCII文字の処理）が明らかになった場合は、Accessibility APIへの切り替えも検討する価値があります。

## 4. 実装計画

### 4.1 新しいモジュール構造

```
src/
  infrastructure/
    external/
      text_input/
        mod.rs            # モジュール定義
        keyboard_events.rs # Quartz Event Services実装
        accessibility.rs  # Accessibility API実装（将来的に実装予定）
        clipboard.rs      # 既存のクリップボード実装（フォールバック用）
```

### 4.2 インターフェース設計

```rust
// src/infrastructure/external/text_input/mod.rs
pub enum TextInsertionMethod {
    KeyboardEvents,
    Accessibility,
    Clipboard, // 既存の方法
}

pub trait TextInserter {
    fn insert_text(&self, text: &str) -> Result<(), String>;
}

pub fn create_text_inserter(method: TextInsertionMethod) -> Box<dyn TextInserter> {
    match method {
        TextInsertionMethod::KeyboardEvents => Box::new(KeyboardEventsInserter::new()),
        TextInsertionMethod::Accessibility => Box::new(AccessibilityInserter::new()),
        TextInsertionMethod::Clipboard => Box::new(ClipboardInserter::new()),
    }
}
```

### 4.3 実装ロードマップ

1. **準備フェーズ**
   - 必要なクレート追加: `core-graphics`, `core-foundation`, `cocoa-foundation`
   - テキスト挿入のコア抽象化レイヤー実装
   - 設定オプション拡張

2. **Quartz Event Services実装**
   - キーボードイベント送信機能の実装
   - ASCII文字サポート
   - 基本的なテスト

3. **拡張フェーズ**
   - 非ASCII文字サポート
   - パフォーマンス最適化
   - エラーハンドリングとフォールバック機能

4. **評価フェーズ**
   - 様々なアプリケーションでのテスト
   - パフォーマンス測定
   - 必要に応じてAccessibility API実装への拡張

### 4.4 設定オプション拡張

`AppConfig`構造体に以下の設定を追加：

```rust
pub enum TextInsertionMethod {
    KeyboardEvents,
    Accessibility,
    Clipboard,
}

pub struct AppConfig {
    // 既存の設定...
    
    /// テキスト挿入方法
    pub text_insertion_method: TextInsertionMethod,
    
    /// キーボードイベント挿入時の遅延（ミリ秒）
    pub key_event_delay_ms: u64,
    
    /// 挿入失敗時にクリップボード方式へフォールバックするか
    pub fallback_to_clipboard: bool,
}
```

### 4.5 エラーハンドリングとフォールバック戦略

```rust
pub fn insert_text_with_fallback(text: &str, config: &AppConfig) -> Result<(), String> {
    // 設定された方法でテキスト挿入を試みる
    let inserter = create_text_inserter(config.text_insertion_method);
    let result = inserter.insert_text(text);
    
    // 挿入に失敗し、フォールバックが有効な場合
    if result.is_err() && config.fallback_to_clipboard {
        log::warn!("Primary insertion method failed: {}, falling back to clipboard", result.unwrap_err());
        
        // クリップボード方式にフォールバック
        let clipboard_inserter = create_text_inserter(TextInsertionMethod::Clipboard);
        return clipboard_inserter.insert_text(text);
    }
    
    result
}
```

## 5. テスト計画

### 5.1 単体テスト

各実装アプローチに対する単体テストを作成：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_keyboard_events_ascii() {
        // ASCIIテキストのキーボードイベント送信テスト
    }
    
    #[test]
    fn test_keyboard_events_non_ascii() {
        // 非ASCIIテキストのキーボードイベント送信テスト
    }
    
    #[test]
    fn test_accessibility_text_insertion() {
        // アクセシビリティAPIによるテキスト挿入テスト
    }
    
    #[test]
    fn test_fallback_mechanism() {
        // フォールバック機能のテスト
    }
}
```

### 5.2 統合テスト

様々なアプリケーションでのテキスト挿入テストを手動で実施：

1. テキストエディタ（VS Code, TextEdit, etc.）
2. ブラウザ（Chrome, Safari, etc.）
3. オフィスアプリケーション（Pages, Word, etc.）
4. ターミナル
5. フォーム入力（ウェブフォーム等）

## 6. 将来の拡張可能性

1. **非ASCII文字の最適化** - より効率的な国際文字入力サポート
2. **アプリケーション固有の最適化** - 特定のアプリケーションに対する挙動調整
3. **リッチテキスト対応** - マークアップや書式付きテキスト挿入のサポート
4. **IMEインテグレーション** - 将来的により高度な統合が必要になった場合
5. **クロスプラットフォーム** - Windows, Linuxでの同様の機能実装