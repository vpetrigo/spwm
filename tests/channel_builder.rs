use spwm::{SpwmChannel, SpwmChannelBuilder, SpwmError, SpwmState};

#[test]
fn builder_standard() {
    let init_fn = || -> Result<SpwmChannel, SpwmError> {
        SpwmChannelBuilder::new(100_000)?
            .on_off_callback(|_: &SpwmState| {})
            .period_callback(|| {})
            .freq_hz(100)?
            .duty_cycle(50)?
            .build()
    };

    let r = init_fn();

    assert!(r.is_ok());
}

#[test]
fn builder_with_invalid_hardware_frequency() {
    let init_fn = || -> Result<SpwmChannel, SpwmError> {
        SpwmChannelBuilder::new(0)?
            .on_off_callback(|_: &SpwmState| {})
            .period_callback(|| {})
            .freq_hz(100)?
            .duty_cycle(50)?
            .build()
    };

    let r = init_fn();

    assert!(r.is_err());
    assert_eq!(r.err().unwrap(), SpwmError::InvalidHardwareFrequency);
}

#[test]
fn builder_with_invalid_frequency() {
    let init_fn = || -> Result<SpwmChannel, SpwmError> {
        SpwmChannelBuilder::new(100_000)?
            .on_off_callback(|_: &SpwmState| {})
            .period_callback(|| {})
            .freq_hz(0)?
            .duty_cycle(50)?
            .build()
    };

    let r = init_fn();

    assert!(r.is_err());
    assert_eq!(r.err().unwrap(), SpwmError::InvalidFrequency);
}

#[test]
fn builder_with_invalid_duty_cycle() {
    let init_fn = || -> Result<SpwmChannel, SpwmError> {
        SpwmChannelBuilder::new(100_000)?
            .on_off_callback(|_: &SpwmState| {})
            .period_callback(|| {})
            .freq_hz(100)?
            .duty_cycle(101)?
            .build()
    };

    let r = init_fn();

    assert!(r.is_err());
    assert_eq!(r.err().unwrap(), SpwmError::InvalidDutyCycle);
}
