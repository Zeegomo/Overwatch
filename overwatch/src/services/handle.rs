// std
use std::marker::PhantomData;
// crates
use futures::future::{abortable, AbortHandle};
use tokio::runtime::Handle;
use tracing::instrument;
// internal
use crate::overwatch::handle::OverwatchHandle;
use crate::services::relay::{relay, InboundRelay, OutboundRelay};
use crate::services::settings::{SettingsNotifier, SettingsUpdater};
use crate::services::state::{StateHandle, StateOperator, StateUpdater};
use crate::services::{ServiceCore, ServiceId, ServiceState};

// TODO: Abstract handle over state, to diferentiate when the service is running and when it is not
// that way we can expose a better API depending on what is happenning. Would get rid of the probably
// unnecessary Option and cloning.
/// Service handle
/// This is used to access different parts of the service
pub struct ServiceHandle<S: ServiceCore> {
    /// Message channel relay
    /// Would be None if service is not running
    /// Will contain the channel if service is running
    outbound_relay: Option<OutboundRelay<S::Message>>,
    /// Handle to overwatch
    overwatch_handle: OverwatchHandle,
    settings: SettingsUpdater<S::Settings>,
    initial_state: S::State,
    _marker: PhantomData<S>,
}

/// Service core resources
/// It contains whatever is necessary to start a new service runner
pub struct ServiceStateHandle<S: ServiceCore> {
    /// Relay channel to communicate with the service runner
    pub inbound_relay: InboundRelay<S::Message>,
    /// Overwatch handle
    pub overwatch_handle: OverwatchHandle,
    pub settings_reader: SettingsNotifier<S::Settings>,
    pub state_updater: StateUpdater<S::State>,
    pub _lifecycle_handler: (),
}

/// Main service executor
/// It is the object that hold the necessary information for the service to run
pub struct ServiceRunner<S: ServiceCore> {
    service_state: ServiceStateHandle<S>,
    state_handle: StateHandle<S::State, S::StateOperator>,
}

impl<S: ServiceCore> ServiceHandle<S> {
    pub fn new(settings: S::Settings, overwatch_handle: OverwatchHandle) -> Self {
        let initial_state: S::State = S::State::from_settings(&settings);

        let settings = SettingsUpdater::new(settings);

        Self {
            outbound_relay: None,
            settings,
            initial_state,
            overwatch_handle,
            _marker: PhantomData::default(),
        }
    }

    pub fn id(&self) -> ServiceId {
        S::SERVICE_ID
    }

    /// Service runtime getter
    /// it is easily cloneable and can be done on demand
    pub fn runtime(&self) -> &Handle {
        self.overwatch_handle.runtime()
    }

    /// Overwatch handle
    /// it is easily cloneable and can be done on demand
    pub fn overwatch_handle(&self) -> &OverwatchHandle {
        &self.overwatch_handle
    }

    /// Request a relay with this service
    pub fn relay_with(&self) -> Option<OutboundRelay<S::Message>> {
        self.outbound_relay.clone()
    }

    /// Update settings
    pub fn update_settings(&self, settings: S::Settings) {
        self.settings.update(settings)
    }

    /// Build a runner for this service
    pub fn service_runner(&mut self) -> ServiceRunner<S> {
        // TODO: add proper status handling here, a service should be able to produce a runner if it is already running.
        let (inbound_relay, outbound_relay) = relay::<S::Message>(S::SERVICE_RELAY_BUFFER_SIZE);
        let settings_reader = self.settings.notifier();
        // add relay channel to handle
        self.outbound_relay = Some(outbound_relay);
        let settings = self.settings.notifier().get_updated_settings();
        let operator = S::StateOperator::from_settings::<S::Settings>(settings);
        let (state_handle, state_updater) =
            StateHandle::<S::State, S::StateOperator>::new(self.initial_state.clone(), operator);

        let service_state = ServiceStateHandle {
            inbound_relay,
            overwatch_handle: self.overwatch_handle.clone(),
            state_updater,
            settings_reader,
            _lifecycle_handler: (),
        };

        ServiceRunner {
            service_state,
            state_handle,
        }
    }
}

impl<S: ServiceCore> ServiceStateHandle<S> {
    pub fn id(&self) -> ServiceId {
        S::SERVICE_ID
    }
}

impl<S: ServiceCore> ServiceRunner<S> {
    /// Spawn the service main loop and handle it lifecycle
    /// Return a handle to abort execution manually
    #[instrument(skip(self), fields(service_id=S::SERVICE_ID))]
    pub fn run(self) -> AbortHandle {
        let ServiceRunner {
            service_state,
            state_handle,
            ..
        } = self;

        let runtime = service_state.overwatch_handle.runtime().clone();
        let service = S::init(service_state);
        let (runner, abortable_handle) = abortable(service.run());

        runtime.spawn(runner);
        runtime.spawn(state_handle.run());

        // TODO: Handle service lifecycle
        // TODO: this handle should not scape this scope, it should actually be handled in the lifecycle part mentioned above
        abortable_handle
    }
}
