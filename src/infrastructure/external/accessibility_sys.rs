//! Low-level macOS Accessibility API bindings
//! 
//! This module provides direct FFI bindings to the macOS Accessibility API functions
//! that are not available in the standard crates.

use core_foundation_sys::base::{Boolean, CFIndex, CFTypeID, CFTypeRef};
use core_foundation_sys::string::CFStringRef;
use std::os::raw::{c_long, c_void};

// AXUIElement type
pub type AXUIElementRef = *mut c_void;

// AXError type
pub type AXError = i32;

// AXError constants
#[allow(non_upper_case_globals, dead_code)]
pub const kAXErrorSuccess: AXError = 0;
#[allow(non_upper_case_globals, dead_code)]
pub const kAXErrorFailure: AXError = -25200;
#[allow(non_upper_case_globals, dead_code)]
pub const kAXErrorIllegalArgument: AXError = -25201;
#[allow(non_upper_case_globals, dead_code)]
pub const kAXErrorInvalidUIElement: AXError = -25202;
#[allow(non_upper_case_globals, dead_code)]
pub const kAXErrorInvalidUIElementObserver: AXError = -25203;
#[allow(non_upper_case_globals, dead_code)]
pub const kAXErrorCannotComplete: AXError = -25204;
#[allow(non_upper_case_globals, dead_code)]
pub const kAXErrorAttributeUnsupported: AXError = -25205;
#[allow(non_upper_case_globals, dead_code)]
pub const kAXErrorActionUnsupported: AXError = -25206;
#[allow(non_upper_case_globals, dead_code)]
pub const kAXErrorNotificationUnsupported: AXError = -25207;
#[allow(non_upper_case_globals, dead_code)]
pub const kAXErrorNotImplemented: AXError = -25208;
#[allow(non_upper_case_globals, dead_code)]
pub const kAXErrorNotificationAlreadyRegistered: AXError = -25209;
#[allow(non_upper_case_globals, dead_code)]
pub const kAXErrorNotificationNotRegistered: AXError = -25210;
#[allow(non_upper_case_globals, dead_code)]
pub const kAXErrorAPIDisabled: AXError = -25211;
#[allow(non_upper_case_globals, dead_code)]
pub const kAXErrorNoValue: AXError = -25212;
#[allow(non_upper_case_globals, dead_code)]
pub const kAXErrorParameterizedAttributeUnsupported: AXError = -25213;
#[allow(non_upper_case_globals, dead_code)]
pub const kAXErrorNotEnoughPrecision: AXError = -25214;

// Link with the correct framework
#[cfg_attr(target_os = "macos", link(name = "ApplicationServices", kind = "framework"))]
#[allow(dead_code)]
unsafe extern "C" {
    // Basic accessibility functions
    pub fn AXIsProcessTrusted() -> Boolean;
    pub fn AXIsProcessTrustedWithOptions(options: CFTypeRef) -> Boolean;
    
    // UIElement functions
    pub fn AXUIElementGetTypeID() -> CFTypeID;
    pub fn AXUIElementCreateSystemWide() -> AXUIElementRef;
    pub fn AXUIElementCreateApplication(pid: c_long) -> AXUIElementRef;
    
    // Attribute functions
    pub fn AXUIElementCopyAttributeNames(
        element: AXUIElementRef,
        names: *mut CFTypeRef,
    ) -> AXError;
    
    pub fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> AXError;
    
    pub fn AXUIElementSetAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: CFTypeRef,
    ) -> AXError;
    
    pub fn AXUIElementIsAttributeSettable(
        element: AXUIElementRef,
        attribute: CFStringRef,
        settable: *mut Boolean,
    ) -> AXError;
    
    pub fn AXUIElementGetAttributeValueCount(
        element: AXUIElementRef,
        attribute: CFStringRef,
        count: *mut CFIndex,
    ) -> AXError;
    
    // Action functions
    pub fn AXUIElementCopyActionNames(
        element: AXUIElementRef,
        names: *mut CFTypeRef,
    ) -> AXError;
    
    pub fn AXUIElementPerformAction(
        element: AXUIElementRef,
        action: CFStringRef,
    ) -> AXError;
}

// Helper to check if an error is success
#[allow(dead_code)]
pub fn ax_error_is_success(error: AXError) -> bool {
    error == kAXErrorSuccess
}