use std::sync::{Arc, Mutex};
use voice_input::application::StackService;

#[tokio::test]
async fn test_daemon_stack_service_integration() {
    // Test that StackService can be properly integrated into daemon state
    let stack_service = Arc::new(Mutex::new(StackService::new()));

    // Test basic operations through Arc<Mutex<>> wrapper
    {
        let mut service = stack_service.lock().unwrap();
        assert!(!service.is_stack_mode_enabled());
        assert!(service.enable_stack_mode());
        assert!(service.is_stack_mode_enabled());
    }

    // Test that multiple threads can access the service
    let stack_service_clone = stack_service.clone();
    tokio::spawn(async move {
        let mut service = stack_service_clone.lock().unwrap();
        let stack_id = service.save_stack("Test from async task".to_string());
        assert_eq!(stack_id, 1);
    })
    .await
    .unwrap();

    // Verify the stack was saved
    {
        let service = stack_service.lock().unwrap();
        let stacks = service.list_stacks();
        assert_eq!(stacks.len(), 1);
        assert_eq!(stacks[0].number, 1);
        assert!(stacks[0].preview.contains("Test from async task"));
    }
}

#[test]
fn test_stack_service_error_handling() {
    let mut service = StackService::new();

    // Test that operations work when stack mode is disabled
    let id = service.save_stack("Test stack".to_string());
    assert_eq!(id, 1);

    // Stack should still be retrievable even when mode is disabled
    let stack = service.get_stack(1);
    assert!(stack.is_some());

    // Test non-existent stack
    let non_existent = service.get_stack(999);
    assert!(non_existent.is_none());
}
