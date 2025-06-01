use std::time::{Duration, Instant};
use voice_input::application::StackService;

#[test]
fn test_stack_save_performance() {
    let mut service = StackService::new();
    let start = Instant::now();

    for i in 0..100 {
        service.save_stack(format!("Test text number {}", i));
    }

    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_millis(100),
        "Stack save performance too slow: took {:?}",
        elapsed
    );
}

#[test]
fn test_stack_list_performance() {
    let mut service = StackService::new();

    // Create 50 stacks
    for i in 0..50 {
        service.save_stack(format!(
            "Test stack content number {} with some longer text to simulate real usage",
            i
        ));
    }

    let start = Instant::now();
    let _list = service.list_stacks();
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_millis(10),
        "Stack list performance too slow: took {:?}",
        elapsed
    );
}

#[test]
fn test_stack_formatted_list_performance() {
    let mut service = StackService::new();

    // Create 50 stacks
    for i in 0..50 {
        service.save_stack(format!(
            "Test stack content number {} with some longer text to simulate real usage",
            i
        ));
    }

    let start = Instant::now();
    let _formatted = service.list_stacks_formatted();
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_millis(10),
        "Stack formatted list performance too slow: took {:?}",
        elapsed
    );
}

#[test]
fn test_stack_get_performance() {
    let mut service = StackService::new();

    // Create 50 stacks
    for i in 0..50 {
        service.save_stack(format!("Test stack content number {}", i));
    }

    let start = Instant::now();

    // Get 100 stacks (some won't exist)
    for i in 1..=100 {
        let _stack = service.get_stack(i);
    }

    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_millis(10),
        "Stack get performance too slow: took {:?}",
        elapsed
    );
}

#[test]
fn test_stack_clear_performance() {
    let mut service = StackService::new();

    // Create 50 stacks
    for i in 0..50 {
        service.save_stack(format!("Test stack content number {}", i));
    }

    let start = Instant::now();
    service.clear_stacks();
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_millis(5),
        "Stack clear performance too slow: took {:?}",
        elapsed
    );
}

#[test]
fn test_large_text_stack_performance() {
    let mut service = StackService::new();
    let large_text = "a".repeat(5000); // 5KB text

    let start = Instant::now();
    for _i in 0..10 {
        service.save_stack(large_text.clone());
    }
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_millis(50),
        "Large text stack save performance too slow: took {:?}",
        elapsed
    );
}

#[test]
fn test_memory_efficiency_estimate() {
    let mut service = StackService::new();

    // Create 50 stacks with realistic content
    for i in 0..50 {
        let content = format!(
            "This is test stack number {} with some realistic content that might be typical \
            for voice input transcription results. It includes multiple sentences and \
            various types of content that users might dictate.",
            i
        );
        service.save_stack(content);
    }

    // This is a simple check that we can create 50 stacks without issues
    assert_eq!(service.list_stacks().len(), 50);

    // Check that all stacks are accessible
    for i in 1..=50 {
        assert!(service.get_stack(i).is_some());
    }
}

#[test]
#[cfg_attr(feature = "ci-test", ignore)]
fn test_concurrent_operations_simulation() {
    let mut service = StackService::new();
    service.enable_stack_mode();

    let start = Instant::now();

    // Simulate rapid operations like a user might perform
    for i in 0..20 {
        service.save_stack(format!("Rapid input {}", i));

        if i % 5 == 0 {
            let _list = service.list_stacks_formatted();
        }

        if i > 10 {
            let _stack = service.get_stack(i - 10);
        }
    }

    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_millis(50),
        "Concurrent operations simulation too slow: took {:?}",
        elapsed
    );
}

#[test]
fn test_optimized_save_performance() {
    let mut service = StackService::new();
    service.enable_stack_mode();

    let start = Instant::now();

    for i in 0..50 {
        let text = format!("Optimized test content {}", i);
        let result = service.save_stack_optimized(text);
        assert!(result.is_ok());
    }

    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_millis(50),
        "Optimized stack save performance too slow: took {:?}",
        elapsed
    );
}
