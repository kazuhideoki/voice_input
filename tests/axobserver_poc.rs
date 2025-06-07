//! AXObserver technical proof of concept
//!
//! This test verifies the feasibility of using AXObserver for event-driven
//! cursor tracking instead of polling.

use core_foundation_sys::base::{CFRelease, CFTypeRef, kCFAllocatorDefault};
use core_foundation_sys::runloop::{
    CFRunLoopAddSource, CFRunLoopGetCurrent, CFRunLoopRunInMode, kCFRunLoopDefaultMode,
};
use core_foundation_sys::string::{CFStringCreateWithCString, CFStringRef, kCFStringEncodingUTF8};
use std::ffi::{CString, c_void};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, Instant};
use voice_input::infrastructure::external::accessibility_sys::*;

static CALLBACK_COUNT: AtomicU32 = AtomicU32::new(0);
static CALLBACK_RECEIVED: AtomicBool = AtomicBool::new(false);

/// AXObserver callback function
unsafe extern "C" fn observer_callback(
    _observer: AXObserverRef,
    element: AXUIElementRef,
    notification: CFStringRef,
    _user_info: *mut c_void,
) {
    CALLBACK_COUNT.fetch_add(1, Ordering::SeqCst);
    CALLBACK_RECEIVED.store(true, Ordering::SeqCst);

    println!("🔔 Callback invoked!");
    println!("   Element: {:p}", element);
    println!("   Notification: {:p}", notification);
    println!("   Count: {}", CALLBACK_COUNT.load(Ordering::SeqCst));

    // Try to get cursor position from the element
    let position_str = CString::new("AXPosition").unwrap();
    let position_attr = CFStringCreateWithCString(
        std::ptr::null(),
        position_str.as_ptr(),
        kCFStringEncodingUTF8,
    );

    let mut position_value: CFTypeRef = std::ptr::null_mut();
    let result = AXUIElementCopyAttributeValue(
        element,
        position_attr,
        &mut position_value as *mut CFTypeRef,
    );

    CFRelease(position_attr as _);

    if ax_error_is_success(result) && !position_value.is_null() {
        println!("   ✅ Successfully retrieved position from element");
        CFRelease(position_value);
    } else {
        println!(
            "   ❌ Could not get position from element (error: {})",
            result
        );
    }
}

#[test]
#[ignore]
fn test_axobserver_feasibility() {
    println!("\n=== AXObserver Technical Verification ===\n");

    unsafe {
        // Reset counters
        CALLBACK_COUNT.store(0, Ordering::SeqCst);
        CALLBACK_RECEIVED.store(false, Ordering::SeqCst);

        // Check accessibility permissions
        if AXIsProcessTrusted() == 0 {
            println!("❌ Accessibility permission not granted");
            println!("   Please enable accessibility permissions for this test");
            panic!("Accessibility permissions required");
        }
        println!("✅ Accessibility permissions granted");

        // Create observer for system-wide notifications
        // We use PID 0 to create an observer that can monitor system-wide events
        let mut observer: AXObserverRef = std::ptr::null_mut();
        let result = AXObserverCreate(0, observer_callback, &mut observer);

        if !ax_error_is_success(result) || observer.is_null() {
            // If PID 0 doesn't work, try with current process
            let pid = std::process::id() as pid_t;
            let result2 = AXObserverCreate(pid, observer_callback, &mut observer);
            if !ax_error_is_success(result2) || observer.is_null() {
                panic!(
                    "Failed to create AXObserver: {} (tried both PID 0 and {})",
                    result2, pid
                );
            }
            println!("✅ Created AXObserver for current process (PID {})", pid);
        } else {
            println!("✅ Created AXObserver for system-wide monitoring");
        }

        // Create application UI element for a specific app to monitor
        // Let's try monitoring Finder (PID can be found via Activity Monitor)
        let finder_app = AXUIElementCreateApplication(1); // PID 1 is usually launchd, let's use system element instead

        // Actually, let's monitor mouse moved events on the system element
        let system_element = AXUIElementCreateSystemWide();

        // Add notification for mouse moved (more frequent than focus changes for testing)
        let notification_cstr = CString::new("AXMouseMoved").unwrap();
        let notification_cf = CFStringCreateWithCString(
            kCFAllocatorDefault,
            notification_cstr.as_ptr(),
            kCFStringEncodingUTF8,
        );

        // Try different approach - monitor current application
        let current_pid = std::process::id() as pid_t;
        let current_app = AXUIElementCreateApplication(current_pid as i64);

        let add_result =
            AXObserverAddNotification(observer, current_app, notification_cf, std::ptr::null_mut());

        CFRelease(notification_cf as _);

        if !ax_error_is_success(add_result) {
            println!("❌ Failed to add mouse moved notification: {}", add_result);
            println!("   Trying focus change notification instead...");

            // Try focus change notification
            let focus_cstr = CString::new(kAXFocusedUIElementChangedNotification).unwrap();
            let focus_cf = CFStringCreateWithCString(
                kCFAllocatorDefault,
                focus_cstr.as_ptr(),
                kCFStringEncodingUTF8,
            );

            let add_result2 =
                AXObserverAddNotification(observer, current_app, focus_cf, std::ptr::null_mut());

            CFRelease(focus_cf as _);

            if !ax_error_is_success(add_result2) {
                panic!(
                    "Failed to add any notification: mouse={}, focus={}",
                    add_result, add_result2
                );
            }
            println!("✅ Added focus change notification");
        } else {
            println!("✅ Added mouse moved notification");
        }

        // Get RunLoop source
        let run_loop_source = AXObserverGetRunLoopSource(observer);
        if run_loop_source.is_null() {
            panic!("Failed to get RunLoop source");
        }
        println!("✅ Got RunLoop source");

        // Add to current RunLoop
        CFRunLoopAddSource(
            CFRunLoopGetCurrent(),
            run_loop_source,
            kCFRunLoopDefaultMode,
        );
        println!("✅ Added to RunLoop");

        // Start monitoring in a separate thread
        let monitoring_thread = std::thread::spawn(move || {
            println!("\n📊 Starting 5-second monitoring period...");
            println!("   Switch between applications to trigger callbacks");

            let start_time = Instant::now();

            // Run for 5 seconds
            CFRunLoopRunInMode(kCFRunLoopDefaultMode, 5.0, 0);

            let elapsed = start_time.elapsed();
            println!(
                "\n📊 Monitoring complete after {:.2}s",
                elapsed.as_secs_f64()
            );
        });

        // Wait for monitoring to complete
        monitoring_thread.join().unwrap();

        // Check results
        let callback_count = CALLBACK_COUNT.load(Ordering::SeqCst);
        let callback_received = CALLBACK_RECEIVED.load(Ordering::SeqCst);

        println!("\n=== Results ===");
        println!("Callbacks received: {}", callback_count);
        println!("At least one callback: {}", callback_received);

        // Clean up
        CFRelease(observer as _);

        // Verdict
        println!("\n=== Technical Verdict ===");
        if callback_received {
            println!("✅ AXObserver is FEASIBLE - callbacks work correctly");
            println!("   - Received {} callbacks during test", callback_count);
            println!("   - Event-driven approach is viable");
        } else {
            println!("❌ AXObserver is NOT FEASIBLE - no callbacks received");
            println!("   - Polling approach should be used instead");
        }
    }
}

#[test]
#[ignore]
fn test_axobserver_performance_comparison() {
    println!("\n=== Performance Comparison: Polling vs AXObserver ===\n");

    // Test polling approach CPU usage
    println!("1. Testing polling approach (30ms interval)...");
    let polling_start = Instant::now();
    let mut poll_count = 0;

    while polling_start.elapsed() < Duration::from_secs(3) {
        // Simulate cursor position check
        std::thread::sleep(Duration::from_millis(30));
        poll_count += 1;
    }

    println!("   Polling: {} checks in 3 seconds", poll_count);
    println!("   Expected ~100 checks (3000ms / 30ms)");

    // Note: Actual CPU usage measurement would require system tools
    println!("\n2. AXObserver approach:");
    println!("   - Only triggers on actual focus changes");
    println!("   - Zero CPU usage when idle");
    println!("   - More efficient for long-running processes");
}

#[test]
#[ignore]
fn test_cursor_position_retrieval() {
    println!("\n=== Cursor Position Retrieval Test ===\n");

    unsafe {
        // Test getting cursor position from focused element
        let system_wide = AXUIElementCreateSystemWide();

        // Get focused element
        let focused_str = CString::new("AXFocusedUIElement").unwrap();
        let focused_attr = CFStringCreateWithCString(
            std::ptr::null(),
            focused_str.as_ptr(),
            kCFStringEncodingUTF8,
        );

        let mut focused_element: CFTypeRef = std::ptr::null_mut();
        let result = AXUIElementCopyAttributeValue(
            system_wide,
            focused_attr,
            &mut focused_element as *mut CFTypeRef,
        );

        CFRelease(focused_attr as _);

        if ax_error_is_success(result) && !focused_element.is_null() {
            println!("✅ Got focused element");

            // Try to get position
            let position_str = CString::new("AXPosition").unwrap();
            let position_attr = CFStringCreateWithCString(
                std::ptr::null(),
                position_str.as_ptr(),
                kCFStringEncodingUTF8,
            );

            let mut position_value: CFTypeRef = std::ptr::null_mut();
            let pos_result = AXUIElementCopyAttributeValue(
                focused_element as AXUIElementRef,
                position_attr,
                &mut position_value as *mut CFTypeRef,
            );

            CFRelease(position_attr as _);

            if ax_error_is_success(pos_result) && !position_value.is_null() {
                println!("✅ Position retrieval WORKS from focused element");
                CFRelease(position_value);
            } else {
                println!("❌ Position retrieval FAILED (error: {})", pos_result);
                println!("   This suggests text cursor position is not available");
                println!("   Mouse cursor fallback is appropriate");
            }

            CFRelease(focused_element);
        } else {
            println!("❌ Could not get focused element");
        }
    }
}
