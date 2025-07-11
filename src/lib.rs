#![no_std]

use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

pub enum SpwmState {
    On,
    Off,
}

#[derive(Debug)]
pub enum SpwmError {
    InvalidChannel,
    InvalidFrequency,
    InvalidDutyCycle,
}

pub type OnOffCallback = fn(SpwmState);
pub type PeriodCallback = fn();

pub type TimerStartCallback = fn();
pub type TimerStopCallback = fn();

const FREQUENCY_DIFFERENCE_REQUIRED: u32 = 100;
const MAX_DUTY_CYCLE: u8 = 100;

#[derive(Default)]
struct SpwmChannel {
    period_ticks: AtomicU32,
    on_ticks: AtomicU32,
    update_on_ticks: AtomicU32,
    counter: AtomicU32,
    enabled: AtomicBool,
    on_off_callback: Option<OnOffCallback>,
    period_callback: Option<PeriodCallback>,
}

pub struct Spwm {
    active: AtomicBool,
    freq_hz: u32,
    channels: [SpwmChannel; 4],
    start_callback: Option<TimerStartCallback>,
    stop_callback: Option<TimerStopCallback>,
}

impl Spwm {
    #[must_use]
    pub fn new(
        freq_hz: u32,
        start_callback: Option<TimerStartCallback>,
        stop_callback: Option<TimerStopCallback>,
    ) -> Self {
        Self {
            active: AtomicBool::new(false),
            freq_hz,
            channels: core::array::from_fn(|_| SpwmChannel::default()),
            start_callback,
            stop_callback,
        }
    }

    ///
    ///
    /// # Arguments
    ///
    /// * `channel`:
    ///
    /// returns: `Result<(), SpwmError>`
    ///
    /// # Errors
    /// - `SpwmError::InvalidChannel` - specified PWM channel index is out of range
    pub fn set_channel_frequency(&self, channel: usize, freq_hz: u32) -> Result<(), SpwmError> {
        if channel >= self.channels.len() {
            return Err(SpwmError::InvalidChannel);
        }

        if freq_hz > self.freq_hz * FREQUENCY_DIFFERENCE_REQUIRED {
            return Err(SpwmError::InvalidFrequency);
        }

        let period_ticks = self.freq_hz / freq_hz;
        self.channels[channel].set_period_ticks(period_ticks);

        Ok(())
    }

    ///
    ///
    /// # Arguments
    ///
    /// * `channel`:
    ///
    /// returns: `Result<(), SpwmError>`
    ///
    /// # Errors
    /// - `SpwmError::InvalidChannel` - specified PWM channel index is out of range
    pub fn set_channel_duty_cycle(&self, channel: usize, duty_cycle: u8) -> Result<(), SpwmError> {
        if channel >= self.channels.len() {
            return Err(SpwmError::InvalidChannel);
        }

        if duty_cycle > MAX_DUTY_CYCLE {
            return Err(SpwmError::InvalidDutyCycle);
        }

        let pwm_channel = &self.channels[channel];
        let current_period = pwm_channel.period_ticks.load(Ordering::Relaxed);
        let on_time = current_period / 100 * u32::from(duty_cycle);

        pwm_channel.update_on_ticks(on_time);

        Ok(())
    }

    pub fn set_channel_on_off_callback(&self, channel: usize, callback: OnOffCallback) {}

    pub fn set_channel_period_callback(&self, channel: usize, callback: PeriodCallback) {}

    ///
    ///
    /// # Arguments
    ///
    /// * `channel`:
    ///
    /// returns: `Result<(), SpwmError>`
    ///
    /// # Errors
    /// - `SpwmError::InvalidChannel` - specified PWM channel index is out of range
    pub fn enable(&self, channel: usize) -> Result<(), SpwmError> {
        if channel >= self.channels.len() {
            return Err(SpwmError::InvalidChannel);
        }

        loop {
            let current = false;

            match self.active.compare_exchange_weak(
                current,
                true,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => break,
                Err(value) => {
                    if value {
                        return Ok(());
                    }
                }
            }
        }

        self.channels[channel].enable();

        // notify a user about an event that the T/C should start
        if let Some(callback) = self.start_callback {
            callback();
        }

        Ok(())
    }

    ///
    ///
    /// # Arguments
    ///
    /// * `channel`:
    ///
    /// returns: `Result<(), SpwmError>`
    ///
    /// # Errors
    /// - `SpwmError::InvalidChannel` - specified PWM channel index is out of range
    pub fn disable(&self, channel: usize) -> Result<(), SpwmError> {
        if channel >= self.channels.len() {
            return Err(SpwmError::InvalidChannel);
        }

        self.channels[channel].disable();

        if self
            .channels
            .iter()
            .any(|channel| channel.enabled.load(Ordering::Relaxed))
        {
            return Ok(());
        }

        if self
            .active
            .compare_exchange(true, false, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            // notify a user about an event that T/C is not required anymore, because all
            // PWM channels are disabled at this point
            if let Some(callback) = self.stop_callback {
                callback();
            }
        }

        Ok(())
    }

    pub fn irq_handler(&self) {
        for channel in &self.channels {
            if channel.enabled.load(Ordering::Relaxed) {
                let current_ticks = channel.counter_tick();
                let period_ticks = channel.period_ticks.load(Ordering::Relaxed);
                let on_ticks = channel.on_ticks.load(Ordering::Relaxed);

                if current_ticks == period_ticks {
                    let update_ticks = channel.update_on_ticks.load(Ordering::Relaxed);

                    channel.counter_reset();

                    if let Some(callback) = channel.period_callback {
                        callback();
                    }

                    if update_ticks != on_ticks {
                        channel.set_on_ticks(update_ticks);
                    }

                    if update_ticks != 0 {
                        if let Some(callback) = channel.on_off_callback {
                            callback(SpwmState::On);
                        }
                    }
                } else if current_ticks == on_ticks {
                    if let Some(callback) = channel.on_off_callback {
                        callback(SpwmState::Off);
                    }
                }
            }
        }
    }
}

impl SpwmChannel {
    fn enable(&self) {
        self.enabled.store(true, Ordering::SeqCst);
    }

    fn disable(&self) {
        self.enabled.store(false, Ordering::SeqCst);
        self.counter.store(0, Ordering::SeqCst);

        if let Some(callback) = self.on_off_callback {
            callback(SpwmState::Off);
        }
    }

    fn counter_tick(&self) -> u32 {
        self.counter.fetch_add(1, Ordering::SeqCst)
    }

    fn counter_reset(&self) {
        self.counter.store(0, Ordering::SeqCst);
    }

    fn set_period_ticks(&self, period_ticks: u32) {
        self.period_ticks.store(period_ticks, Ordering::SeqCst);
    }

    fn update_on_ticks(&self, on_ticks: u32) {
        if self.enabled.load(Ordering::Relaxed) {
            self.update_on_ticks.store(on_ticks, Ordering::SeqCst);
        } else {
            self.on_ticks.store(on_ticks, Ordering::SeqCst);
        }
    }

    fn set_on_ticks(&self, on_ticks: u32) {
        self.on_ticks.store(on_ticks, Ordering::SeqCst);
    }

    fn set_on_off_callback(&mut self, on_off_callback: Option<OnOffCallback>) {
        self.on_off_callback = on_off_callback;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_ENABLED: AtomicBool = AtomicBool::new(false);

    fn start_test_callback() {
        TEST_ENABLED.store(true, Ordering::SeqCst);
    }

    fn stop_test_callback() {
        TEST_ENABLED.store(false, Ordering::SeqCst);
    }

    #[test]
    fn construct_spwm() {
        let spwm = Spwm::new(100_000, None, None);
        let result = spwm.set_channel_frequency(0, 1000);
        assert!(
            result.is_ok(),
            "Unable to set the channel's frequency: {result:?}"
        );
        let result = spwm.set_channel_duty_cycle(0, 50);
        assert!(
            result.is_ok(),
            "Unable to set the channel's duty cycle: {result:?}"
        );

        let result = spwm.set_channel_frequency(100, 1000);
        assert!(result.is_err(), "Unexpected set the channel's frequency");
        let result = spwm.set_channel_duty_cycle(100, 50);
        assert!(
            result.is_err(),
            "Unexpected set the channel's duty cycle: {result:?}"
        );
    }

    #[test]
    fn start_stop_spwm() {
        TEST_ENABLED.store(false, Ordering::Relaxed);

        let spwm = Spwm::new(100_000, Some(start_test_callback), Some(stop_test_callback));
        let _ = spwm.set_channel_frequency(0, 1000);
        let _ = spwm.set_channel_duty_cycle(0, 50);
        let _ = spwm.enable(0);
        assert!(TEST_ENABLED.load(Ordering::Relaxed));
        let _ = spwm.disable(0);
        assert!(!TEST_ENABLED.load(Ordering::Relaxed));
        let _ = spwm.enable(1);
        assert!(TEST_ENABLED.load(Ordering::Relaxed));
        let _ = spwm.disable(1);
        assert!(!TEST_ENABLED.load(Ordering::Relaxed));
    }

    #[test]
    fn on_off_callback_for_single_channel() {
        let spwm = Spwm::new(100_000, None, None);
        let _ = spwm.set_channel_frequency(0, 1000);
        let _ = spwm.set_channel_duty_cycle(0, 50);
    }

    #[test]
    fn it_works_2() {
        assert_eq!(2 + 2, 4);
    }
}
