use core::sync::atomic::{AtomicBool, AtomicU32};
use spwm::{ChannelId, OnOffCallback, PeriodCallback, Spwm, SpwmChannel, SpwmError, SpwmState};
use std::sync::Mutex;
use std::sync::atomic::Ordering;
use std::vec::Vec;

const PERIODS_FOR_TEST: u32 = 50u32;
static TEST_ON_OFF: AtomicBool = AtomicBool::new(false);
static TEST_PERIOD: AtomicU32 = AtomicU32::new(0);
static TEST_LOCK: Mutex<()> = Mutex::new(());

fn on_off_test_callback(state: &SpwmState) {
    match state {
        SpwmState::On => TEST_ON_OFF.store(true, Ordering::Relaxed),
        SpwmState::Off => TEST_ON_OFF.store(false, Ordering::Relaxed),
    }
}

fn period_test_callback() {
    TEST_PERIOD.fetch_add(1, Ordering::Relaxed);
}

fn test_create_pwm_channel<const N: usize>(
    spwm: &Spwm<N>,
    channel_freq_hz: u32,
    duty_cycle: u8,
) -> Result<SpwmChannel, SpwmError> {
    spwm.create_channel()
        .freq_hz(channel_freq_hz)
        .duty_cycle(duty_cycle)
        .on_off_callback(|_| {})
        .period_callback(|| {})
        .build()
}

fn test_create_pwm_channel_with_callbacks<const N: usize>(
    spwm: &Spwm<N>,
    channel_freq_hz: u32,
    duty_cycle: u8,
    on_off_callback: OnOffCallback,
    period_callback: PeriodCallback,
) -> Result<SpwmChannel, SpwmError> {
    spwm.create_channel()
        .freq_hz(channel_freq_hz)
        .duty_cycle(duty_cycle)
        .on_off_callback(on_off_callback)
        .period_callback(period_callback)
        .build()
}

#[test]
fn construct_spwm_single_channel() {
    let base_freq = 100_000;
    let mut spwm = Spwm::<4>::new(base_freq);
    let channel = test_create_pwm_channel(&spwm, 1000, 50);

    assert!(channel.is_ok());
    let channel = channel.unwrap();

    let result = channel.update_duty_cycle(25);
    assert!(result.is_ok());
    let result = channel.update_duty_cycle(100);
    assert!(result.is_ok());
    let result = channel.update_duty_cycle(0);
    assert!(result.is_ok());

    let result = channel.update_frequency(500, base_freq);
    assert!(result.is_ok());
    let result = channel.update_frequency(100, base_freq);
    assert!(result.is_ok());
    let result = channel.update_frequency(10, base_freq);
    assert!(result.is_ok());
    let result = spwm.register_channel(channel);
    assert!(result.is_ok());
    let channel_id = result.unwrap();
    let channel = spwm.get_channel(channel_id);
    assert!(channel.is_some());
    let channel = channel.unwrap();
    let result = channel.update_duty_cycle(25);
    assert!(result.is_ok());
    let result = channel.update_frequency(1000, base_freq);
    assert!(result.is_ok());
}

#[test]
fn construct_spwm_multiple_channels() {
    let mut spwm = Spwm::<4>::new(100_000);
    let test_channel_params = [(1000, 50), (500, 50), (100, 50), (10, 50)];
    let mut channel_ids: Vec<ChannelId> = Vec::with_capacity(test_channel_params.len());

    for param in test_channel_params {
        let channel = test_create_pwm_channel(&spwm, param.0, param.1);
        assert!(channel.is_ok());
        let result = spwm.register_channel(channel.unwrap());
        assert!(result.is_ok());
        channel_ids.push(result.unwrap());
    }

    for channel_id in channel_ids {
        let channel = spwm.get_channel(channel_id);
        assert!(channel.is_some());
        let result = channel.unwrap().update_duty_cycle(10);
        assert!(result.is_ok());
    }
}

#[test]
fn construct_spwm_more_than_available_channels() {
    let mut spwm = Spwm::<4>::new(100_000);
    let test_channel_params = [(1000, 50), (500, 50), (100, 50), (10, 50)];
    let mut channel_ids: Vec<ChannelId> = Vec::with_capacity(test_channel_params.len());

    for param in test_channel_params {
        let channel = test_create_pwm_channel(&spwm, param.0, param.1);
        assert!(channel.is_ok());
        let result = spwm.register_channel(channel.unwrap());
        assert!(result.is_ok());
        channel_ids.push(result.unwrap());
    }

    for param in test_channel_params {
        let channel = test_create_pwm_channel(&spwm, param.0, param.1);
        assert!(channel.is_ok());
        let result = spwm.register_channel(channel.unwrap());
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), SpwmError::NoChannelSlotAvailable);
    }
}

#[test]
fn channel_multiple_enable_disable_calls() {
    let mut spwm = Spwm::<4>::new(100_000);
    let test_channel_param = (100, 10);
    let channel = test_create_pwm_channel(&spwm, test_channel_param.0, test_channel_param.1);
    assert!(channel.is_ok());
    let result = spwm.register_channel(channel.unwrap());
    assert!(result.is_ok());
    let channel_id = result.unwrap();
    let channel = spwm.get_channel(channel_id);
    assert!(channel.is_some());
    let channel = channel.unwrap();
    let result = channel.enable();
    assert!(result.is_ok());
    let result = channel.enable();
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), SpwmError::AlreadyEnabled);
    let result = channel.enable();
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), SpwmError::AlreadyEnabled);
    let result = channel.disable();
    assert!(result.is_ok());
    let result = channel.disable();
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), SpwmError::AlreadyDisabled);
    let result = channel.disable();
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), SpwmError::AlreadyDisabled);
}

#[test]
fn construct_spwm_invalid_freq_and_duty_cycle() {
    let spwm = Spwm::<4>::new(100_000);
    let test_invalid_freq_setup = [0, 100_001, 500_000];
    let test_invalid_duty_cycle_setup = [101, 255];

    for freq in test_invalid_freq_setup {
        let channel = test_create_pwm_channel(&spwm, freq, 50);
        assert!(
            channel.is_err(),
            "Successful construction with an invalid frequency: {freq}"
        );
        assert_eq!(channel.unwrap_err(), SpwmError::InvalidFrequency);
    }

    for duty_cycle in test_invalid_duty_cycle_setup {
        let channel = test_create_pwm_channel(&spwm, 1000, duty_cycle);
        assert!(
            channel.is_err(),
            "Successful construction with an invalid duty cycle: {duty_cycle}"
        );
        assert_eq!(channel.unwrap_err(), SpwmError::InvalidDutyCycle);
    }
}

#[test]
fn on_off_callback_for_single_channel_100_duty_cycle() {
    let _lock = TEST_LOCK.lock().unwrap();
    TEST_ON_OFF.store(false, Ordering::Relaxed);
    TEST_PERIOD.store(0, Ordering::Relaxed);

    let sim_timer_freq = 100_000;
    let channel0_freq = 1000;
    let channel0_duty_cycle = 100;

    let mut spwm = Spwm::<4>::new(sim_timer_freq);
    let channel = test_create_pwm_channel_with_callbacks(
        &spwm,
        channel0_freq,
        channel0_duty_cycle,
        on_off_test_callback,
        period_test_callback,
    );
    assert!(channel.is_ok());
    let channel = channel.unwrap();
    let result = spwm.register_channel(channel);
    assert!(result.is_ok());
    let channel_id = result.unwrap();
    let channel = spwm.get_channel(channel_id);
    assert!(channel.is_some());
    let channel = channel.unwrap();
    let result = channel.enable();
    assert!(result.is_ok());
    let channel0_period = sim_timer_freq / channel0_freq;
    let mut expected_period = 1;

    for i in 0..(PERIODS_FOR_TEST * channel0_period) {
        spwm.irq_handler();

        if i == channel0_period {
            assert_eq!(TEST_PERIOD.load(Ordering::Relaxed), expected_period);
            assert!(TEST_ON_OFF.load(Ordering::Relaxed));
            expected_period += 1;
        }
    }
}

#[test]
fn on_off_callback_for_single_channel_50_duty_cycle() {
    let _lock = TEST_LOCK.lock().unwrap();
    TEST_ON_OFF.store(false, Ordering::Relaxed);
    TEST_PERIOD.store(0, Ordering::Relaxed);

    let sim_timer_freq = 100_000;
    let channel0_freq = 1000;
    let channel0_duty_cycle = 50;

    let mut spwm = Spwm::<4>::new(sim_timer_freq);
    let channel = test_create_pwm_channel_with_callbacks(
        &spwm,
        channel0_freq,
        channel0_duty_cycle,
        on_off_test_callback,
        period_test_callback,
    );
    assert!(channel.is_ok());
    let result = spwm.register_channel(channel.unwrap());
    assert!(result.is_ok());
    let channel_id = result.unwrap();
    let channel = spwm.get_channel(channel_id);
    assert!(channel.is_some());
    let channel = channel.unwrap();
    let result = channel.enable();
    assert!(result.is_ok());
    assert!(TEST_ON_OFF.load(Ordering::Relaxed));
    let channel0_period = sim_timer_freq / channel0_freq;
    let channel0_on_ticks = channel0_period / 100 * u32::from(channel0_duty_cycle);
    let mut expected_period = 0;

    for i in 0..(PERIODS_FOR_TEST * channel0_period - 1) {
        spwm.irq_handler();
        // |-----|___________|-----|____________
        // ^ - check for ON state
        //       ^ - check for OFF state
        //                   ^ - check for period update
        if (i % channel0_period) == 0 {
            assert_eq!(TEST_PERIOD.load(Ordering::Relaxed), expected_period);
            assert!(TEST_ON_OFF.load(Ordering::Relaxed));
            expected_period += 1;
        } else if (i % channel0_on_ticks) == 0 {
            assert!(!TEST_ON_OFF.load(Ordering::Relaxed));
        }
    }

    assert!(!TEST_ON_OFF.load(Ordering::Relaxed));
}

#[test]
fn on_off_callback_for_single_channel_0_duty_cycle() {
    let _lock = TEST_LOCK.lock().unwrap();
    TEST_ON_OFF.store(false, Ordering::Relaxed);
    TEST_PERIOD.store(0, Ordering::Relaxed);

    let sim_timer_freq = 100_000;
    let channel0_freq = 1000;
    let channel0_duty_cycle = 0;

    let mut spwm = Spwm::<4>::new(sim_timer_freq);
    let channel = test_create_pwm_channel_with_callbacks(
        &spwm,
        channel0_freq,
        channel0_duty_cycle,
        on_off_test_callback,
        period_test_callback,
    );
    assert!(channel.is_ok());
    let result = spwm.register_channel(channel.unwrap());
    assert!(result.is_ok());
    let channel_id = result.unwrap();
    let channel = spwm.get_channel(channel_id);
    assert!(channel.is_some());
    let channel = channel.unwrap();
    let result = channel.enable();
    assert!(result.is_ok());
    assert!(!TEST_ON_OFF.load(Ordering::Relaxed));
    let channel0_period = sim_timer_freq / channel0_freq;
    let mut expected_period = 1;

    for i in 0..(PERIODS_FOR_TEST * channel0_period) {
        spwm.irq_handler();

        if i == channel0_period {
            assert_eq!(TEST_PERIOD.load(Ordering::Relaxed), expected_period);
            assert!(!TEST_ON_OFF.load(Ordering::Relaxed));
            expected_period += 1;
        }
    }

    assert!(!TEST_ON_OFF.load(Ordering::Relaxed));
}

#[test]
fn on_off_callback_for_single_channel_disabled_50_duty_cycle() {
    let _lock = TEST_LOCK.lock().unwrap();
    TEST_ON_OFF.store(false, Ordering::Relaxed);
    TEST_PERIOD.store(0, Ordering::Relaxed);

    let sim_timer_freq = 100_000;
    let channel0_freq = 1000;
    let channel0_duty_cycle = 50;

    let mut spwm = Spwm::<4>::new(sim_timer_freq);
    let channel = test_create_pwm_channel_with_callbacks(
        &spwm,
        channel0_freq,
        channel0_duty_cycle,
        on_off_test_callback,
        period_test_callback,
    );

    assert!(channel.is_ok());
    let channel_id = spwm.register_channel(channel.unwrap());
    assert!(channel_id.is_ok());
    let channel_id = channel_id.unwrap();
    let channel = spwm.get_channel(channel_id);
    assert!(channel.is_some());
    let channel = channel.unwrap();
    let result = channel.enable();
    assert!(result.is_ok());
    let result = channel.disable();
    assert!(result.is_ok());

    assert!(!TEST_ON_OFF.load(Ordering::Relaxed));

    let channel0_period = sim_timer_freq / channel0_freq;
    let expected_period = 0;

    for i in 0..(PERIODS_FOR_TEST * channel0_period) {
        spwm.irq_handler();

        if i == channel0_period {
            assert_eq!(TEST_PERIOD.load(Ordering::Relaxed), expected_period);
            assert!(!TEST_ON_OFF.load(Ordering::Relaxed));
        }
    }

    assert_eq!(TEST_PERIOD.load(Ordering::Relaxed), expected_period);
    assert!(!TEST_ON_OFF.load(Ordering::Relaxed));
}
