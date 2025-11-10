use crate::{FREQUENCY_DIFFERENCE_REQUIRED, OnOffCallback, PeriodCallback, SpwmError, SpwmState};
use core::cell::OnceCell;
use core::marker::PhantomData;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

const MAX_DUTY_CYCLE: u8 = 100;

pub struct SpwmChannelFreqHzBuildState {}
pub struct SpwmChannelDutyCycleBuildState {}
pub struct SpwmChannelFinalizedBuildState {}

#[derive(Default)]
pub struct SpwmChannel {
    pub(crate) period_ticks: AtomicU32,
    pub(crate) on_ticks: AtomicU32,
    pub(crate) update_on_ticks: AtomicU32,
    pub(crate) counter: AtomicU32,
    pub(crate) enabled: AtomicBool,
    pub(crate) on_off_callback: OnceCell<OnOffCallback>,
    pub(crate) period_callback: OnceCell<PeriodCallback>,
}

impl SpwmChannel {
    pub fn enable(&self) {
        self.enabled.store(true, Ordering::SeqCst);

        if let Some(callback) = self.on_off_callback.get()
            && self.on_ticks.load(Ordering::Relaxed) != 0
        {
            callback(&SpwmState::On);
        }
    }

    pub fn disable(&self) {
        self.enabled.store(false, Ordering::SeqCst);
        self.counter.store(0, Ordering::SeqCst);

        if let Some(callback) = self.on_off_callback.get() {
            callback(&SpwmState::Off);
        }
    }

    pub fn counter_tick(&self) -> u32 {
        self.counter.fetch_add(1, Ordering::SeqCst)
    }

    pub fn counter_reset(&self) {
        self.counter.store(0, Ordering::SeqCst);
    }

    pub fn set_period_ticks(&self, period_ticks: u32) {
        self.period_ticks.store(period_ticks, Ordering::SeqCst);
    }

    pub fn update_on_ticks(&self, on_ticks: u32) {
        if self.enabled.load(Ordering::Relaxed) {
            self.update_on_ticks.store(on_ticks, Ordering::SeqCst);
        } else {
            self.on_ticks.store(on_ticks, Ordering::SeqCst);
            self.update_on_ticks.store(on_ticks, Ordering::SeqCst);
        }
    }

    pub fn set_on_ticks(&self, on_ticks: u32) {
        self.on_ticks.store(on_ticks, Ordering::SeqCst);
    }

    pub fn set_on_off_callback(&self, on_off_callback: OnOffCallback) -> Result<(), OnOffCallback> {
        self.on_off_callback.set(on_off_callback)
    }

    pub fn set_period_callback(
        &self,
        period_callback: PeriodCallback,
    ) -> Result<(), PeriodCallback> {
        self.period_callback.set(period_callback)
    }
}

pub struct SpwmChannelBuilder<T> {
    hardware_freq_hz: u32,
    channel_freq_hz: u32,
    duty_cycle: u8,
    on_off_callback: Option<OnOffCallback>,
    period_callback: Option<PeriodCallback>,
    _phantom: PhantomData<T>,
}

impl<T> SpwmChannelBuilder<T> {
    pub fn on_off_callback(mut self, on_off_callback: OnOffCallback) -> Self {
        self.on_off_callback = Some(on_off_callback);
        self
    }

    pub fn period_callback(mut self, period_callback: PeriodCallback) -> Self {
        self.period_callback = Some(period_callback);
        self
    }
}

impl SpwmChannelBuilder<SpwmChannelFreqHzBuildState> {
    pub(crate) fn new(hardware_freq_hz: u32) -> Self {
        Self {
            hardware_freq_hz,
            channel_freq_hz: 0,
            duty_cycle: 0,
            on_off_callback: None,
            period_callback: None,
            _phantom: PhantomData,
        }
    }

    pub fn freq_hz(
        self,
        freq_hz: u32,
    ) -> Result<SpwmChannelBuilder<SpwmChannelDutyCycleBuildState>, SpwmError> {
        if self.hardware_freq_hz / FREQUENCY_DIFFERENCE_REQUIRED < freq_hz {
            return Err(SpwmError::InvalidFrequency);
        }

        Ok(SpwmChannelBuilder {
            hardware_freq_hz: self.hardware_freq_hz,
            channel_freq_hz: freq_hz,
            duty_cycle: 0,
            on_off_callback: self.on_off_callback,
            period_callback: self.period_callback,
            _phantom: PhantomData,
        })
    }
}

impl SpwmChannelBuilder<SpwmChannelDutyCycleBuildState> {
    pub fn duty_cycle(
        self,
        duty_cycle: u8,
    ) -> Result<SpwmChannelBuilder<SpwmChannelFinalizedBuildState>, SpwmError> {
        if duty_cycle > MAX_DUTY_CYCLE {
            return Err(SpwmError::InvalidDutyCycle);
        }

        Ok(SpwmChannelBuilder {
            hardware_freq_hz: self.hardware_freq_hz,
            channel_freq_hz: self.channel_freq_hz,
            duty_cycle,
            on_off_callback: self.on_off_callback,
            period_callback: self.period_callback,
            _phantom: PhantomData,
        })
    }
}

impl SpwmChannelBuilder<SpwmChannelFinalizedBuildState> {
    pub fn build(self) -> Result<SpwmChannel, SpwmError> {
        let period_ticks = self.hardware_freq_hz / self.channel_freq_hz;
        let on_time = period_ticks / 100 * u32::from(self.duty_cycle);
        let channel = SpwmChannel::default();

        if self.on_off_callback.is_none() || self.period_callback.is_none() {
            return Err(SpwmError::CallbackSetError);
        }

        channel.set_period_ticks(period_ticks);
        channel.update_on_ticks(on_time);
        channel
            .set_on_off_callback(self.on_off_callback.unwrap())
            .unwrap();
        channel
            .set_period_callback(self.period_callback.unwrap())
            .unwrap();

        Ok(channel)
    }
}
