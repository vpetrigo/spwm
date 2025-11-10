#![no_std]

mod channel;

use channel::SpwmChannelBuilder;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use crate::channel::SpwmChannelFreqHzBuildState;
pub use channel::SpwmChannel;

pub enum SpwmState {
    On,
    Off,
}

#[derive(Debug)]
pub enum SpwmError {
    InvalidChannel,
    InvalidFrequency,
    InvalidDutyCycle,
    CallbackSetError,
    NoChannelSlotAvailable,
}

pub type OnOffCallback = fn(&SpwmState);
pub type PeriodCallback = fn();

pub type TimerStartCallback = fn();
pub type TimerStopCallback = fn();

pub type ChannelId = usize;

const FREQUENCY_DIFFERENCE_REQUIRED: u32 = 100;

#[derive(Default)]
struct ChannelSlot {
    channel: Option<SpwmChannel>,
}

pub struct Spwm<const N: usize> {
    active: AtomicBool,
    channels: [SpwmChannel; N],
    upd_channels: [ChannelSlot; N],
    freq_hz: u32,
    start_callback: Option<TimerStartCallback>,
    stop_callback: Option<TimerStopCallback>,
}

impl<const N: usize> Spwm<N> {
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
            upd_channels: core::array::from_fn(|_| ChannelSlot::default()),
            start_callback,
            stop_callback,
        }
    }

    pub fn create_channel(&self) -> SpwmChannelBuilder<SpwmChannelFreqHzBuildState> {
        SpwmChannelBuilder::new(self.freq_hz)
    }

    pub fn register_channel(&mut self, channel: SpwmChannel) -> Result<ChannelId, SpwmError> {
        for (i, slot) in self.upd_channels.iter_mut().enumerate() {
            if slot.channel.is_none() {
                slot.channel = Some(channel);

                return Ok(i);
            }
        }

        Err(SpwmError::NoChannelSlotAvailable)
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
        // if channel >= self.channels.len() {
        //     return Err(SpwmError::InvalidChannel);
        // }
        //
        // if duty_cycle > MAX_DUTY_CYCLE {
        //     return Err(SpwmError::InvalidDutyCycle);
        // }
        //
        // let pwm_channel = &self.channels[channel];
        // let current_period = pwm_channel.period_ticks.load(Ordering::Relaxed);
        // let on_time = current_period / 100 * u32::from(duty_cycle);
        //
        // pwm_channel.update_on_ticks(on_time);

        Ok(())
    }

    /// Sets a callback function to handle the on/off state changes for a specific PWM channel.
    ///
    /// This method sets a user-provided callback function, which will be invoked when the on/off
    /// state of the specified channel changes.
    ///
    /// # Parameters
    /// - `channel`: The index of the channel for which the callback is being set. The index must
    ///   be within the range of available channels.
    /// - `callback`: The callback function of type `OnOffCallback` to handle the on/off state changes.
    ///
    /// # Returns
    /// - `Ok(())`: If the callback was successfully set for the specified channel.
    /// - `Err(SpwmError::InvalidChannel)`: If the specified channel index is out of range.
    ///
    /// # Errors
    /// This function will return `Err(SpwmError::InvalidChannel)` if the supplied channel index
    /// is greater than or equal to the total number of channels available in the `self.channels` array.
    pub fn set_channel_on_off_callback(
        &self,
        channel: usize,
        callback: OnOffCallback,
    ) -> Result<(), SpwmError> {
        if channel >= self.channels.len() {
            return Err(SpwmError::InvalidChannel);
        }

        self.channels[channel]
            .set_on_off_callback(callback)
            .map_err(|_| SpwmError::CallbackSetError)
    }

    /// Sets the callback function to handle the channel's period update.
    ///
    /// # Parameters
    /// - `channel`: The index of the channel for which the period callback is to be set.
    ///   Must be less than the total number of channels.
    /// - `callback`: The callback function that will be executed when the channel's period is updated.
    ///
    /// # Returns
    /// - `Ok(())`: Indicates that the callback was successfully set.
    /// - `Err(SpwmError::InvalidChannel)`: Returned if the given `channel` index is out of range.
    ///
    /// # Errors
    /// This function will return an error if the `channel` index is equal to or greater than the number
    /// of available channels.
    pub fn set_channel_period_callback(
        &self,
        channel: usize,
        callback: PeriodCallback,
    ) -> Result<(), SpwmError> {
        if channel >= self.channels.len() {
            return Err(SpwmError::InvalidChannel);
        }

        self.channels[channel]
            .set_period_callback(callback)
            .map_err(|_| SpwmError::CallbackSetError)
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
    pub fn enable(&self, channel: usize) -> Result<(), SpwmError> {
        if channel >= self.channels.len() {
            return Err(SpwmError::InvalidChannel);
        }

        let mut current = self.active.load(Ordering::Relaxed);

        loop {
            match self.active.compare_exchange_weak(
                current,
                true,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => break,
                Err(value) => current = value,
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

                if current_ticks == (period_ticks - 1) {
                    let update_ticks = channel.update_on_ticks.load(Ordering::Relaxed);

                    channel.counter_reset();

                    if let Some(callback) = channel.period_callback.get() {
                        callback();
                    }

                    if update_ticks != on_ticks {
                        channel.set_on_ticks(update_ticks);
                    }

                    let on_ticks = channel.on_ticks.load(Ordering::Relaxed);

                    if on_ticks != 0
                        && let Some(callback) = channel.on_off_callback.get()
                    {
                        callback(&SpwmState::On);
                    }
                } else if current_ticks == on_ticks
                    && let Some(callback) = channel.on_off_callback.get()
                {
                    callback(&SpwmState::Off);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::ops::Not;

    const PERIODS_FOR_TEST: u32 = 50u32;
    static TEST_ENABLED: AtomicBool = AtomicBool::new(false);
    static TEST_ON_OFF: AtomicBool = AtomicBool::new(false);
    static TEST_PERIOD: AtomicU32 = AtomicU32::new(0);

    fn start_test_callback() {
        TEST_ENABLED.store(true, Ordering::SeqCst);
    }

    fn stop_test_callback() {
        TEST_ENABLED.store(false, Ordering::SeqCst);
    }

    fn on_off_test_callback(state: &SpwmState) {
        match state {
            SpwmState::On => TEST_ON_OFF.store(true, Ordering::Relaxed),
            SpwmState::Off => TEST_ON_OFF.store(false, Ordering::Relaxed),
        }
    }

    fn period_test_callback() {
        TEST_PERIOD.fetch_add(1, Ordering::Relaxed);
    }

    #[test]
    fn construct_spwm() {
        let spwm = Spwm::<4>::new(100_000, None, None);
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

        let spwm = Spwm::<4>::new(100_000, Some(start_test_callback), Some(stop_test_callback));
        let _ = spwm.set_channel_frequency(0, 1000);
        let _ = spwm.set_channel_duty_cycle(0, 50);
        let _ = spwm.enable(0);
        assert!(TEST_ENABLED.load(Ordering::Relaxed));
        let _ = spwm.disable(0);
        assert!(TEST_ENABLED.load(Ordering::Relaxed).not());
        let _ = spwm.enable(1);
        assert!(TEST_ENABLED.load(Ordering::Relaxed));
        let _ = spwm.disable(1);
        assert!(TEST_ENABLED.load(Ordering::Relaxed).not());
    }

    #[test]
    fn on_off_callback_for_single_channel_100_duty_cycle() {
        TEST_ON_OFF.store(false, Ordering::Relaxed);
        TEST_PERIOD.store(0, Ordering::Relaxed);

        let sim_timer_freq = 100_000;
        let channel0_freq = 1000;
        let channel0_duty_cycle = 100;

        let spwm = Spwm::<4>::new(sim_timer_freq, None, None);
        let _ = spwm.set_channel_frequency(0, channel0_freq);
        let _ = spwm.set_channel_duty_cycle(0, channel0_duty_cycle);
        let result = spwm.set_channel_on_off_callback(0, on_off_test_callback);
        assert!(result.is_ok());
        let result = spwm.set_channel_period_callback(0, period_test_callback);
        assert!(result.is_ok());
        let _ = spwm.enable(0);

        assert!(TEST_ON_OFF.load(Ordering::Relaxed));
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

        assert!(TEST_ON_OFF.load(Ordering::Relaxed));
    }

    #[test]
    fn on_off_callback_for_single_channel_50_duty_cycle() {
        TEST_ON_OFF.store(false, Ordering::Relaxed);
        TEST_PERIOD.store(0, Ordering::Relaxed);

        let sim_timer_freq = 100_000;
        let channel0_freq = 1000;
        let channel0_duty_cycle = 50;

        let spwm = Spwm::<4>::new(sim_timer_freq, None, None);
        let _ = spwm.set_channel_frequency(0, channel0_freq);
        let _ = spwm.set_channel_duty_cycle(0, channel0_duty_cycle);
        let result = spwm.set_channel_on_off_callback(0, on_off_test_callback);
        assert!(result.is_ok());
        let result = spwm.set_channel_period_callback(0, period_test_callback);
        assert!(result.is_ok());
        let _ = spwm.enable(0);

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
                assert!(TEST_ON_OFF.load(Ordering::Relaxed).not());
            }
        }

        assert!(TEST_ON_OFF.load(Ordering::Relaxed).not());
    }

    #[test]
    fn on_off_callback_for_single_channel_0_duty_cycle() {
        TEST_ON_OFF.store(false, Ordering::Relaxed);
        TEST_PERIOD.store(0, Ordering::Relaxed);

        let sim_timer_freq = 100_000;
        let channel0_freq = 1000;
        let channel0_duty_cycle = 0;

        let spwm = Spwm::<4>::new(sim_timer_freq, None, None);
        let _ = spwm.set_channel_frequency(0, channel0_freq);
        let _ = spwm.set_channel_duty_cycle(0, channel0_duty_cycle);
        let result = spwm.set_channel_on_off_callback(0, on_off_test_callback);
        assert!(result.is_ok());
        let result = spwm.set_channel_period_callback(0, period_test_callback);
        assert!(result.is_ok());
        let _ = spwm.enable(0);

        assert!(TEST_ON_OFF.load(Ordering::Relaxed).not());
        let channel0_period = sim_timer_freq / channel0_freq;
        let mut expected_period = 1;

        for i in 0..(PERIODS_FOR_TEST * channel0_period) {
            spwm.irq_handler();

            if i == channel0_period {
                assert_eq!(TEST_PERIOD.load(Ordering::Relaxed), expected_period);
                assert!(TEST_ON_OFF.load(Ordering::Relaxed).not());
                expected_period += 1;
            }
        }

        assert!(TEST_ON_OFF.load(Ordering::Relaxed).not());
    }

    #[test]
    fn on_off_callback_for_single_channel_disabled_50_duty_cycle() {
        TEST_ON_OFF.store(false, Ordering::Relaxed);
        TEST_PERIOD.store(0, Ordering::Relaxed);

        let sim_timer_freq = 100_000;
        let channel0_freq = 1000;
        let channel0_duty_cycle = 50;

        let spwm = Spwm::<4>::new(sim_timer_freq, None, None);
        let _ = spwm.set_channel_frequency(0, channel0_freq);
        let _ = spwm.set_channel_duty_cycle(0, channel0_duty_cycle);
        let result = spwm.set_channel_on_off_callback(0, on_off_test_callback);
        assert!(result.is_ok());
        let result = spwm.set_channel_period_callback(0, period_test_callback);
        assert!(result.is_ok());
        let _ = spwm.enable(0);
        let _ = spwm.disable(0);

        assert!(TEST_ON_OFF.load(Ordering::Relaxed).not());

        let channel0_period = sim_timer_freq / channel0_freq;
        let expected_period = 0;

        for i in 0..(PERIODS_FOR_TEST * channel0_period) {
            spwm.irq_handler();

            if i == channel0_period {
                assert_eq!(TEST_PERIOD.load(Ordering::Relaxed), expected_period);
                assert!(TEST_ON_OFF.load(Ordering::Relaxed).not());
            }
        }

        assert_eq!(TEST_PERIOD.load(Ordering::Relaxed), expected_period);
        assert!(TEST_ON_OFF.load(Ordering::Relaxed).not());
    }

    #[test]
    fn build_check() {
        // SpwmChannelBuilder::default().
        let init_fn = || -> Result<SpwmChannel, SpwmError> {
            SpwmChannelBuilder::new(100_000)
                .on_off_callback(|_: &SpwmState| {})
                .period_callback(|| {})
                .freq_hz(100)?
                .duty_cycle(50)?
                .build()
        };

        let r = init_fn();

        assert!(r.is_ok());

        match r {
            Ok(_) => {}
            Err(SpwmError::InvalidFrequency) => {}
            _ => panic!("Unexpected error"),
        }
    }
}
