use voice_input::application::{StackService, StackServiceError};

#[test]
fn test_max_stacks_limit() {
    let mut service = StackService::new();
    service.enable_stack_mode();

    // Fill up to MAX_STACKS
    for i in 0..StackService::MAX_STACKS {
        let result = service.save_stack_optimized(format!("Stack {}", i));
        assert!(result.is_ok());
    }

    assert_eq!(service.list_stacks().len(), StackService::MAX_STACKS);

    // Add one more - should remove oldest and add new
    let result = service.save_stack_optimized("New stack".to_string());
    assert!(result.is_ok());

    // Should still be at MAX_STACKS
    assert_eq!(service.list_stacks().len(), StackService::MAX_STACKS);

    // The first stack (id=1) should be gone
    assert!(service.get_stack(1).is_none());

    // The new stack should exist
    let stacks = service.list_stacks();
    assert!(stacks.iter().any(|s| s.preview.contains("New stack")));
}

#[test]
fn test_text_size_limit() {
    let mut service = StackService::new();
    service.enable_stack_mode();

    let large_text = "a".repeat(StackService::MAX_STACK_SIZE + 1);
    let result = service.save_stack_optimized(large_text);

    assert!(result.is_err());
    match result.unwrap_err() {
        StackServiceError::TextTooLarge(size) => {
            assert_eq!(size, StackService::MAX_STACK_SIZE + 1);
        }
        _ => panic!("Expected TextTooLarge error"),
    }
}

#[test]
fn test_max_size_boundary() {
    let mut service = StackService::new();
    service.enable_stack_mode();

    // Exactly at the limit should work
    let max_text = "a".repeat(StackService::MAX_STACK_SIZE);
    let result = service.save_stack_optimized(max_text);
    assert!(result.is_ok());

    // One character over the limit should fail
    let over_limit_text = "a".repeat(StackService::MAX_STACK_SIZE + 1);
    let result = service.save_stack_optimized(over_limit_text);
    assert!(result.is_err());
}

#[test]
fn test_remove_oldest_stack_order() {
    let mut service = StackService::new();
    service.enable_stack_mode();

    // Create MAX_STACKS + 5 stacks to test removal order
    let num_stacks = std::cmp::min(10, StackService::MAX_STACKS + 5);

    for i in 0..num_stacks {
        let result = service.save_stack_optimized(format!("Stack content {}", i));
        assert!(result.is_ok());
    }

    let stacks = service.list_stacks();
    assert_eq!(
        stacks.len(),
        std::cmp::min(num_stacks, StackService::MAX_STACKS)
    );

    if num_stacks > StackService::MAX_STACKS {
        // Should start from the stack that wasn't removed
        let first_remaining = num_stacks - StackService::MAX_STACKS + 1;
        assert!(service.get_stack(first_remaining as u32).is_some());

        // Earlier stacks should be removed
        assert!(service.get_stack(1).is_none());
    }
}

#[test]
fn test_empty_text_handling() {
    let mut service = StackService::new();
    service.enable_stack_mode();

    // Empty text should be allowed
    let result = service.save_stack_optimized("".to_string());
    assert!(result.is_ok());

    let id = result.unwrap();
    let stack = service.get_stack(id).unwrap();
    assert_eq!(stack.text, "");
}

#[test]
fn test_whitespace_only_text() {
    let mut service = StackService::new();
    service.enable_stack_mode();

    // Whitespace-only text should be allowed
    let whitespace_text = "   \t\n   ".to_string();
    let result = service.save_stack_optimized(whitespace_text.clone());
    assert!(result.is_ok());

    let id = result.unwrap();
    let stack = service.get_stack(id).unwrap();
    assert_eq!(stack.text, whitespace_text);
}

#[test]
fn test_unicode_text_size_calculation() {
    let mut service = StackService::new();
    service.enable_stack_mode();

    // Unicode characters should be counted properly
    let unicode_text = "🎯".repeat(1000); // Each emoji is multiple bytes
    let result = service.save_stack_optimized(unicode_text.clone());

    // Should succeed as we're counting characters, not bytes
    assert!(result.is_ok());

    let id = result.unwrap();
    let stack = service.get_stack(id).unwrap();
    assert_eq!(stack.text, unicode_text);
}
