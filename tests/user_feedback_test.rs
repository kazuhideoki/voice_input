use voice_input::application::UserFeedback;

#[test]
fn test_stack_saved_message() {
    let message = UserFeedback::stack_saved(1, "This is a test preview...");
    assert_eq!(message, "📝 Stack 1 saved: This is a test preview...");
}

#[test]
fn test_paste_success_message() {
    let message = UserFeedback::paste_success(1, 150);
    assert_eq!(message, "✅ Pasted stack 1 (150 characters)");
}

#[test]
fn test_stack_not_found_message() {
    let available = vec![1, 3, 5];
    let message = UserFeedback::stack_not_found(99, &available);
    assert_eq!(message, "❌ Stack 99 not found. Available: [1, 3, 5]");
}

#[test]
fn test_stack_not_found_empty_available() {
    let available = vec![];
    let message = UserFeedback::stack_not_found(1, &available);
    assert_eq!(message, "❌ Stack 1 not found. Available: []");
}

#[test]
fn test_mode_status_enabled() {
    let message = UserFeedback::mode_status(true, 5);
    assert_eq!(message, "🟢 Stack mode ON (5 stacks in memory)");
}

#[test]
fn test_mode_status_disabled() {
    let message = UserFeedback::mode_status(false, 0);
    assert_eq!(message, "🔴 Stack mode OFF");
}

#[test]
fn test_feedback_message_emojis() {
    // Test that all feedback messages include appropriate emojis
    assert!(UserFeedback::stack_saved(1, "test").contains("📝"));
    assert!(UserFeedback::paste_success(1, 100).contains("✅"));
    assert!(UserFeedback::stack_not_found(1, &[]).contains("❌"));
    assert!(UserFeedback::mode_status(true, 1).contains("🟢"));
    assert!(UserFeedback::mode_status(false, 0).contains("🔴"));
}

#[test]
fn test_feedback_message_consistency() {
    // Test that all messages follow consistent formatting
    let saved = UserFeedback::stack_saved(10, "preview");
    let pasted = UserFeedback::paste_success(10, 250);
    let not_found = UserFeedback::stack_not_found(10, &[1, 2, 3]);

    // All should mention stack number
    assert!(saved.contains("10"));
    assert!(pasted.contains("10"));
    assert!(not_found.contains("10"));

    // Success messages should be positive
    assert!(saved.starts_with("📝") || saved.starts_with("✅"));
    assert!(pasted.starts_with("✅"));

    // Error messages should be clear
    assert!(not_found.starts_with("❌"));
}
