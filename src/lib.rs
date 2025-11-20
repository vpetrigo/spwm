#![no_std]
mod channel;

use core::sync::atomic::Ordering;

pub use channel::{SpwmChannel, SpwmChannelBuilder, SpwmChannelFreqHzBuildState};

/// Represents the output state of a PWM channel.
pub enum SpwmState {
    /// Output is in the "on" (high) state
    On,
    /// Output is in the "off" (low) state
    Off,
}

/// Errors that can occur during SPWM operations.
#[derive(Debug, PartialEq)]
pub enum SpwmError {
    /// The specified hardware timer frequency is not valid
    InvalidHardwareFrequency,
    /// The specified channel index is out of range
    InvalidChannel,
    /// The requested frequency is too high for the configured hardware timer frequency
    InvalidFrequency,
    /// The duty cycle value is greater than 100
    InvalidDutyCycle,
    /// Failed to set a callback (already set or required callback missing)
    CallbackSetError,
    /// A PWM channel already enabled
    AlreadyEnabled,
    /// A PWM channel enable operation failed
    EnableFailed,
    /// A PWM channel already disabled
    AlreadyDisabled,
    /// A PWM channel disable operation failed
    DisableFailed,
    /// No free channel slots available for registration
    NoChannelSlotAvailable,
}

/// Callback invoked when a channel's output state changes.
///
/// # Parameters
/// - `state`: The new state of the channel output
pub type OnOffCallback = fn(&SpwmState);

/// Callback invoked at the end of each PWM period.
pub type PeriodCallback = fn();

/// Callback invoked when the first channel is enabled (timer should start).
pub type TimerStartCallback = fn();

/// Callback invoked when all channels are disabled (timer can stop).
pub type TimerStopCallback = fn();

/// Unique identifier for a registered channel.
pub type ChannelId = usize;

#[derive(Default)]
struct ChannelSlot {
    channel: Option<SpwmChannel>,
}

pub struct Spwm<const N: usize> {
    channel_slots: [ChannelSlot; N],
    freq_hz: u32,
}

impl<const N: usize> Spwm<N> {
    #[must_use]
    pub fn new(
        freq_hz: u32
    ) -> Self {
        Self {
            freq_hz,
            channel_slots: core::array::from_fn(|_| ChannelSlot::default()),
        }
    }

    pub fn create_channel(
        &self,
    ) -> Result<SpwmChannelBuilder<SpwmChannelFreqHzBuildState>, SpwmError> {
        SpwmChannelBuilder::new(self.freq_hz)
    }

    pub fn register_channel(&mut self, channel: SpwmChannel) -> Result<ChannelId, SpwmError> {
        for (i, slot) in self.channel_slots.iter_mut().enumerate() {
            if slot.channel.is_none() {
                slot.channel = Some(channel);

                return Ok(i);
            }
        }

        Err(SpwmError::NoChannelSlotAvailable)
    }

    pub fn get_channel(&self, channel_id: ChannelId) -> Option<&SpwmChannel> {
        self.channel_slots.get(channel_id)?.channel.as_ref()
    }

    pub fn irq_handler(&self) {
        for slot in &self.channel_slots {
            if let Some(ref channel) = slot.channel
                && channel.enabled.load(Ordering::Relaxed)
            {
                let current_ticks = channel.counter_tick();
                let period_ticks = channel.period_ticks.load(Ordering::Relaxed);
                let on_ticks = channel.on_ticks.load(Ordering::Relaxed);

                if current_ticks >= (period_ticks - 1) {
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
