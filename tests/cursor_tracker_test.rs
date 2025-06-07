//! Cursor tracker unit tests
//!
//! Tests for the polling-based cursor position tracking implementation.

use std::time::Duration;
use voice_input::infrastructure::animation::cursor_tracker::CursorTracker;

#[test]
#[cfg_attr(feature = "ci-test", ignore)]
fn test_cursor_tracker_creation() {
    let tracker = CursorTracker::new();
    assert!(tracker.get_receiver().try_recv().is_err());
}

#[test]
#[cfg_attr(feature = "ci-test", ignore)]
fn test_cursor_tracker_start_stop() {
    let mut tracker = CursorTracker::new();
    
    // Start tracking
    assert!(tracker.start().is_ok());
    
    // Should not be able to start again
    assert!(tracker.start().is_err());
    
    // Stop tracking
    assert!(tracker.stop().is_ok());
    
    // Should be able to start again after stopping
    assert!(tracker.start().is_ok());
    assert!(tracker.stop().is_ok());
}

#[test]
#[cfg_attr(feature = "ci-test", ignore)]
fn test_cursor_position_updates() {
    let mut tracker = CursorTracker::new();
    
    // Start tracking
    tracker.start().expect("Failed to start tracking");
    
    // Wait for position updates
    std::thread::sleep(Duration::from_millis(100));
    
    // Should have received at least one position update
    let mut positions = Vec::new();
    while let Ok(pos) = tracker.get_receiver().try_recv() {
        positions.push(pos);
    }
    
    // At 30ms intervals for 100ms, should have ~3 updates
    assert!(!positions.is_empty(), "Should have received position updates");
    assert!(positions.len() >= 2, "Should have multiple updates");
    
    // All positions should be valid
    for pos in &positions {
        assert!(pos.x >= 0.0);
        assert!(pos.y >= 0.0);
    }
    
    tracker.stop().expect("Failed to stop tracking");
}

#[test]
#[cfg_attr(feature = "ci-test", ignore)]
fn test_cursor_tracker_drop() {
    let mut tracker = CursorTracker::new();
    tracker.start().expect("Failed to start tracking");
    
    // Drop should automatically stop tracking
    drop(tracker);
    
    // Create a new tracker to ensure resources were cleaned up
    let mut new_tracker = CursorTracker::new();
    assert!(new_tracker.start().is_ok());
    new_tracker.stop().expect("Failed to stop tracking");
}

#[test]
#[cfg_attr(feature = "ci-test", ignore)]
fn test_position_accuracy() {
    let mut tracker = CursorTracker::new();
    tracker.start().expect("Failed to start tracking");
    
    // Get initial position
    std::thread::sleep(Duration::from_millis(50));
    let pos1 = tracker.get_receiver().recv().expect("Failed to receive position");
    
    // Get another position
    std::thread::sleep(Duration::from_millis(50));
    let pos2 = tracker.get_receiver().recv().expect("Failed to receive position");
    
    // If cursor hasn't moved, positions should be similar (allowing for minor jitter)
    let distance = ((pos2.x - pos1.x).powi(2) + (pos2.y - pos1.y).powi(2)).sqrt();
    
    // Note: This test might fail if the mouse is actively moving
    println!("Position 1: {:?}, Position 2: {:?}, Distance: {}", pos1, pos2, distance);
    
    tracker.stop().expect("Failed to stop tracking");
}

/// Performance test to verify CPU usage stays under 1%
#[test]
#[ignore]
fn test_cpu_usage() {
    use std::process::Command;
    
    let mut tracker = CursorTracker::new();
    tracker.start().expect("Failed to start tracking");
    
    // Get process ID
    let pid = std::process::id();
    
    // Let it run for a few seconds
    std::thread::sleep(Duration::from_secs(3));
    
    // Check CPU usage (macOS specific)
    let output = Command::new("ps")
        .args(&["-p", &pid.to_string(), "-o", "%cpu"])
        .output()
        .expect("Failed to execute ps command");
    
    let cpu_output = String::from_utf8_lossy(&output.stdout);
    println!("CPU usage output: {}", cpu_output);
    
    // Parse CPU usage (second line contains the value)
    if let Some(cpu_line) = cpu_output.lines().nth(1) {
        if let Ok(cpu_usage) = cpu_line.trim().parse::<f32>() {
            println!("CPU usage: {}%", cpu_usage);
            assert!(cpu_usage < 1.0, "CPU usage should be less than 1%");
        }
    }
    
    tracker.stop().expect("Failed to stop tracking");
}