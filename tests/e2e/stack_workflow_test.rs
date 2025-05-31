use voice_input::application::{StackService, UserFeedback};

/// Complete E2E workflow test for stack functionality
#[tokio::test]
async fn test_complete_stack_workflow() {
    let mut service = StackService::new();
    
    // Phase 1: Initial state
    assert!(!service.is_stack_mode_enabled());
    assert_eq!(service.list_stacks().len(), 0);
    
    // Phase 2: Enable stack mode
    assert!(service.enable_stack_mode());
    assert!(service.is_stack_mode_enabled());
    
    // Verify mode status feedback
    let status = UserFeedback::mode_status(true, 0);
    assert!(status.contains("Stack mode ON"));
    assert!(status.contains("0 stacks"));
    
    // Phase 3: Voice input simulation - add multiple stacks
    let transcribed_texts = vec![
        "This is the first voice input test content.",
        "Second voice input with different content.",
        "Third input containing some technical terms and numbers like 123.",
        "Fourth input with special characters: @#$%^&*().",
    ];
    
    let mut stack_ids = Vec::new();
    for (i, text) in transcribed_texts.iter().enumerate() {
        let id = service.save_stack(text.to_string());
        stack_ids.push(id);
        
        // Verify feedback message
        let preview = text.chars().take(30).collect::<String>();
        let feedback = UserFeedback::stack_saved(id, &preview);
        assert!(feedback.contains(&format!("Stack {} saved", id)));
        assert!(feedback.contains("📝"));
        
        // Verify stack is accessible
        assert!(service.get_stack(id).is_some());
        assert_eq!(service.list_stacks().len(), i + 1);
    }
    
    // Phase 4: Stack management operations
    
    // Test list stacks formatted
    let formatted_list = service.list_stacks_formatted();
    assert!(formatted_list.contains("4 stack(s) in memory"));
    assert!(formatted_list.contains("[1]"));
    assert!(formatted_list.contains("[4]"));
    assert!(formatted_list.contains("Use 'voice_input paste"));
    
    // Test individual stack retrieval with context
    for &id in &stack_ids {
        let result = service.get_stack_with_context(id);
        assert!(result.is_ok());
        let stack = result.unwrap();
        assert_eq!(stack.id, id);
        assert!(!stack.text.is_empty());
    }
    
    // Phase 5: Error handling tests
    
    // Test non-existent stack
    let result = service.get_stack_with_context(99);
    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("Stack 99 not found"));
    assert!(error_msg.contains("Available stacks: 1, 2, 3, 4"));
    
    // Test stack mode disabled error
    service.disable_stack_mode();
    let result = service.get_stack_with_context(1);
    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("Stack mode is not enabled"));
    
    // Re-enable for remaining tests
    service.enable_stack_mode();
    
    // Phase 6: Performance validation
    
    // Add many stacks quickly
    let start = std::time::Instant::now();
    for i in 0..20 {
        service.save_stack(format!("Performance test stack {}", i));
    }
    let elapsed = start.elapsed();
    assert!(elapsed < std::time::Duration::from_millis(20));
    
    // List all stacks quickly
    let start = std::time::Instant::now();
    let _list = service.list_stacks_formatted();
    let elapsed = start.elapsed();
    assert!(elapsed < std::time::Duration::from_millis(10));
    
    // Phase 7: Stack clearing
    
    let stack_count = service.list_stacks().len();
    assert!(stack_count > 0);
    
    let (cleared_count, message) = service.clear_stacks_with_confirmation();
    assert_eq!(cleared_count, stack_count);
    assert!(message.contains(&format!("Cleared {} stack", cleared_count)));
    assert!(message.contains("✅"));
    
    // Verify all stacks are gone
    assert_eq!(service.list_stacks().len(), 0);
    
    // Phase 8: Post-clear operations
    
    // Test clear on empty
    let (cleared_count, message) = service.clear_stacks_with_confirmation();
    assert_eq!(cleared_count, 0);
    assert!(message.contains("No stacks to clear"));
    
    // Test list on empty
    let formatted = service.list_stacks_formatted();
    assert!(formatted.contains("No stacks saved"));
    assert!(formatted.contains("Use 'voice_input start'"));
    
    // Phase 9: Disable stack mode
    assert!(service.disable_stack_mode());
    assert!(!service.is_stack_mode_enabled());
    
    let status = UserFeedback::mode_status(false, 0);
    assert!(status.contains("Stack mode OFF"));
    assert!(status.contains("🔴"));
}

/// Test workflow with memory limits
#[test]
fn test_workflow_with_memory_limits() {
    let mut service = StackService::new();
    service.enable_stack_mode();
    
    // Phase 1: Test text size limit
    let large_text = "a".repeat(StackService::MAX_STACK_SIZE + 100);
    let result = service.save_stack_optimized(large_text);
    assert!(result.is_err());
    
    // Phase 2: Test at boundary
    let max_text = "b".repeat(StackService::MAX_STACK_SIZE);
    let result = service.save_stack_optimized(max_text);
    assert!(result.is_ok());
    
    // Phase 3: Test stack count limit (simulate)
    let initial_stacks = std::cmp::min(10, StackService::MAX_STACKS);
    for i in 0..initial_stacks {
        let result = service.save_stack_optimized(format!("Limit test {}", i));
        assert!(result.is_ok());
    }
    
    assert_eq!(service.list_stacks().len(), initial_stacks + 1); // +1 from max_text
}

/// Test workflow with special content
#[test]
fn test_workflow_with_special_content() {
    let mut service = StackService::new();
    service.enable_stack_mode();
    
    let special_contents = vec![
        "", // Empty
        "   ", // Whitespace only
        "🎯📝✅❌🟢🔴", // Unicode emojis
        "Line 1\nLine 2\nLine 3", // Multi-line
        "Special chars: !@#$%^&*()[]{}|;:,.<>?", // Special characters
        "Mixed: ASCII + 日本語 + 🌟 + Ñoñó", // Mixed languages/scripts
    ];
    
    for (i, content) in special_contents.iter().enumerate() {
        let id = service.save_stack(content.to_string());
        let retrieved = service.get_stack(id).unwrap();
        assert_eq!(retrieved.text, *content);
        
        // Test that it appears in formatted list
        let formatted = service.list_stacks_formatted();
        assert!(formatted.contains(&format!("[{}]", id)));
    }
    
    // Verify all special content stacks exist
    assert_eq!(service.list_stacks().len(), special_contents.len());
}

/// Test error recovery workflow
#[test]
fn test_error_recovery_workflow() {
    let mut service = StackService::new();
    
    // Try operations when disabled
    assert!(!service.is_stack_mode_enabled());
    
    let result = service.get_stack_with_context(1);
    assert!(result.is_err());
    
    // Enable and create a stack
    service.enable_stack_mode();
    let id = service.save_stack("Test recovery".to_string());
    
    // Disable and try again
    service.disable_stack_mode();
    let result = service.get_stack_with_context(id);
    assert!(result.is_err());
    
    // Re-enable and verify data is gone (as per design)
    service.enable_stack_mode();
    assert_eq!(service.list_stacks().len(), 0);
    
    // Should be able to create new stacks normally
    let new_id = service.save_stack("After recovery".to_string());
    assert!(service.get_stack(new_id).is_some());
}