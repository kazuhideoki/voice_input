use voice_input::application::StackService;

#[test]
fn test_application_module_exports() {
    // Test that we can import and create StackService from the application module
    let _service = StackService::new();
}

#[test]
fn test_module_visibility() {
    // Test that the StackService is properly exported and accessible
    use voice_input::application;

    let _service = application::StackService::new();
}
