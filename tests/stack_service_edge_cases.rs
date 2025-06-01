use voice_input::application::{StackService, StackServiceError};

#[test]
fn test_paste_nonexistent_stack_helpful_message() {
    let mut service = StackService::new();
    service.enable_stack_mode();
    service.save_stack("test".to_string());

    let result = service.get_stack_with_context(99);
    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("Stack 99 not found"));
    assert!(error_msg.contains("Available stacks: 1"));
}

#[test]
fn test_paste_when_stack_mode_disabled() {
    let service = StackService::new(); // mode disabled by default

    let result = service.get_stack_with_context(1);
    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("Stack mode is not enabled"));
}

#[test]
fn test_save_text_too_large() {
    let mut service = StackService::new();
    service.enable_stack_mode();

    let large_text = "a".repeat(20_000); // Exceeds MAX_STACK_SIZE
    let result = service.save_stack_optimized(large_text);

    assert!(result.is_err());
    match result.unwrap_err() {
        StackServiceError::TextTooLarge(size) => assert_eq!(size, 20_000),
        _ => panic!("Expected TextTooLarge error"),
    }
}

#[test]
fn test_stack_service_error_display() {
    let available = vec![1, 3, 5];
    let error = StackServiceError::StackNotFound(99, available.clone());
    let error_msg = error.to_string();

    assert!(error_msg.contains("Stack 99 not found"));
    assert!(error_msg.contains("Available stacks: 1, 3, 5"));
}

#[test]
fn test_multiple_stacks_available_list() {
    let mut service = StackService::new();
    service.enable_stack_mode();
    service.save_stack("first".to_string());
    service.save_stack("second".to_string());
    service.save_stack("third".to_string());

    let result = service.get_stack_with_context(10);
    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    // Check that all stack numbers are mentioned (order may vary due to HashMap)
    assert!(error_msg.contains("1"));
    assert!(error_msg.contains("2"));
    assert!(error_msg.contains("3"));
    assert!(error_msg.contains("Available stacks:"));
}

#[test]
fn test_empty_stacks_error_message() {
    let mut service = StackService::new();
    service.enable_stack_mode();

    let result = service.get_stack_with_context(1);
    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("No stacks saved"));
}

#[test]
fn test_clear_stacks_with_confirmation() {
    let mut service = StackService::new();
    service.save_stack("test1".to_string());
    service.save_stack("test2".to_string());

    let (count, message) = service.clear_stacks_with_confirmation();
    assert_eq!(count, 2);
    assert!(message.contains("Cleared 2 stack(s)"));
    assert_eq!(service.list_stacks().len(), 0);
}

#[test]
fn test_clear_empty_stacks_with_confirmation() {
    let mut service = StackService::new();

    let (count, message) = service.clear_stacks_with_confirmation();
    assert_eq!(count, 0);
    assert!(message.contains("No stacks to clear"));
}

#[test]
fn test_list_stacks_formatted_empty() {
    let service = StackService::new();
    let formatted = service.list_stacks_formatted();

    assert!(formatted.contains("No stacks saved"));
    assert!(formatted.contains("Use 'voice_input start' to create stacks"));
}

#[test]
fn test_list_stacks_formatted_with_content() {
    let mut service = StackService::new();
    service.save_stack("First test content".to_string());
    service.save_stack("Second test content".to_string());

    let formatted = service.list_stacks_formatted();
    assert!(formatted.contains("2 stack(s) in memory"));
    assert!(formatted.contains("[1]"));
    assert!(formatted.contains("[2]"));
    assert!(formatted.contains("Use 'voice_input paste <number>' to paste"));
}
