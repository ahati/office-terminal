use kitty_pptx_display::*;

#[test]
fn test_capability_probe_env_detection() {
    std::env::set_var("KITTY_WINDOW_ID", "1");
    let detected = CapabilityProbe::probe().expect("probe failed");
    match detected {
        DetectedProtocol::Kitty => {},
        _ => panic!("Expected Kitty protocol detection from KITTY_WINDOW_ID"),
    }
    std::env::remove_var("KITTY_WINDOW_ID");
}

#[test]
fn test_kitty_driver_caps() {
    let driver = KittyDriver::new(PlacementMode::Interactive);
    let caps = driver.caps();
    assert!(caps.alpha);
    assert!(caps.placeholders);
    assert!(caps.terminal_side_scaling);
    assert_eq!(caps.resident_ids, 8);
    assert_eq!(driver.placement_mode(), PlacementMode::Interactive);
}
