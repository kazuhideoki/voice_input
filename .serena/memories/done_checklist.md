# タスク完了時のチェックリスト

- [ ] すべてのテストがローカルで成功（`cargo test`）
- [ ] CI 用テストが成功（`cargo test --features ci-test`）
- [ ] Clippy 警告ゼロ（`cargo clippy -- -D warnings` または `--all-targets --features ci-test`）
- [ ] コードフォーマット済み（`cargo fmt`）
- [ ] 公開 API のドキュメント更新
- [ ] 変更点に対するテストを追加/更新
- [ ] 破壊的変更があれば README/ドキュメント更新
