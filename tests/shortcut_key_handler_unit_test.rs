//! KeyHandlerの基本メソッドのユニットテスト
//! rdev実動作以外の純粋な関数をテストする

#[cfg(test)]
mod tests {
    use rdev::Key;

    // KeyHandlerの実装前にテスト関数を定義
    // 実装時にこれらの関数をKeyHandlerに移植する

    fn is_cmd_key(key: &Key) -> bool {
        matches!(key, Key::MetaLeft | Key::MetaRight)
    }

    fn key_to_number(key: &Key) -> u32 {
        match key {
            Key::Num1 => 1,
            Key::Num2 => 2,
            Key::Num3 => 3,
            Key::Num4 => 4,
            Key::Num5 => 5,
            Key::Num6 => 6,
            Key::Num7 => 7,
            Key::Num8 => 8,
            Key::Num9 => 9,
            _ => 0,
        }
    }

    #[test]
    fn test_is_cmd_key() {
        // Cmdキーの判定
        assert!(is_cmd_key(&Key::MetaLeft));
        assert!(is_cmd_key(&Key::MetaRight));

        // 非Cmdキーの判定
        assert!(!is_cmd_key(&Key::KeyR));
        assert!(!is_cmd_key(&Key::Num1));
        assert!(!is_cmd_key(&Key::ControlLeft));
        assert!(!is_cmd_key(&Key::ShiftLeft));
    }

    #[test]
    fn test_key_to_number() {
        // 数字キーの変換
        assert_eq!(key_to_number(&Key::Num1), 1);
        assert_eq!(key_to_number(&Key::Num2), 2);
        assert_eq!(key_to_number(&Key::Num3), 3);
        assert_eq!(key_to_number(&Key::Num4), 4);
        assert_eq!(key_to_number(&Key::Num5), 5);
        assert_eq!(key_to_number(&Key::Num6), 6);
        assert_eq!(key_to_number(&Key::Num7), 7);
        assert_eq!(key_to_number(&Key::Num8), 8);
        assert_eq!(key_to_number(&Key::Num9), 9);

        // 非数字キーは0を返す
        assert_eq!(key_to_number(&Key::KeyR), 0);
        assert_eq!(key_to_number(&Key::MetaLeft), 0);
        assert_eq!(key_to_number(&Key::Space), 0);
        assert_eq!(key_to_number(&Key::Num0), 0); // 0キーも0を返す（Phase 1では未使用）
    }

    #[test]
    fn test_shortcut_key_combinations() {
        // ショートカットキーで使用される組み合わせの確認
        let target_keys = vec![
            Key::KeyR, // Cmd+R用
            Key::Num1,
            Key::Num2,
            Key::Num3, // Cmd+1-3用
            Key::Num4,
            Key::Num5,
            Key::Num6, // Cmd+4-6用
            Key::Num7,
            Key::Num8,
            Key::Num9, // Cmd+7-9用
        ];

        for key in target_keys {
            match key {
                Key::KeyR => {
                    // Rキーは非Cmdキー、数字変換は0
                    assert!(!is_cmd_key(&key));
                    assert_eq!(key_to_number(&key), 0);
                }
                Key::Num1
                | Key::Num2
                | Key::Num3
                | Key::Num4
                | Key::Num5
                | Key::Num6
                | Key::Num7
                | Key::Num8
                | Key::Num9 => {
                    // 数字キーは非Cmdキー、適切な数字に変換
                    assert!(!is_cmd_key(&key));
                    assert!(key_to_number(&key) >= 1 && key_to_number(&key) <= 9);
                }
                _ => {}
            }
        }
    }

    #[test]
    fn test_edge_cases() {
        // エッジケースのテスト

        // ファンクションキーも0を返す
        assert_eq!(key_to_number(&Key::F1), 0);
        assert_eq!(key_to_number(&Key::F9), 0);

        // その他の修飾キーはCmdキーではない
        assert!(!is_cmd_key(&Key::ControlLeft));
        assert!(!is_cmd_key(&Key::ControlRight));
        assert!(!is_cmd_key(&Key::ShiftLeft));
        assert!(!is_cmd_key(&Key::ShiftRight));
    }
}
