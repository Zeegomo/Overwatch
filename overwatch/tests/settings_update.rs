use async_trait::async_trait;
use overwatch::overwatch::OverwatchRunner;
use overwatch::services::handle::{ServiceHandle, ServiceStateHandle};
use overwatch::services::relay::RelayMessage;
use overwatch::services::state::{NoOperator, NoState};
use overwatch::services::{ServiceCore, ServiceData, ServiceId};
use overwatch_derive::Services;
use std::time::Duration;
use tokio::time::sleep;

pub struct SettingsService {
    state: ServiceStateHandle<Self>,
}

type SettingsServiceSettings = String;

#[derive(Clone, Debug)]
pub struct SettingsMsg;

impl RelayMessage for SettingsMsg {}

impl ServiceData for SettingsService {
    const SERVICE_ID: ServiceId = "FooService";
    type Settings = SettingsServiceSettings;
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    type Message = SettingsMsg;
}

#[async_trait]
impl ServiceCore for SettingsService {
    fn init(state: ServiceStateHandle<Self>) -> Self {
        Self { state }
    }

    async fn run(mut self) {
        let Self {
            state:
                ServiceStateHandle {
                    mut settings_reader,
                    ..
                },
        } = self;

        let print = async move {
            let mut asserted = false;
            for _ in 0..10 {
                let new_settings = settings_reader.get_updated_settings();
                if new_settings.as_str() == "New settings" {
                    asserted = true;
                }
                sleep(Duration::from_millis(50)).await;
            }
            // TODO: when [this](https://github.com/ockam-network/ockam/issues/2479)
            // or (https://github.com/tokio-rs/tokio/issues/2002) lands
            // update so this panic is not just a print and the test get actually aborted
            assert!(asserted);
        };
        print.await;
    }
}

#[derive(Services)]
struct TestApp {
    settings_service: ServiceHandle<SettingsService>,
}

#[test]
fn settings_service_update_settings() {
    let mut settings: TestAppServiceSettings = TestAppServiceSettings {
        settings_service: SettingsServiceSettings::default(),
    };
    let overwatch = OverwatchRunner::<TestApp>::run(settings.clone(), None);
    let handle = overwatch.handle().clone();
    let mut handle2 = handle.clone();
    settings.settings_service = "New settings".to_string();
    overwatch.spawn(async move { handle.clone().update_settings::<TestApp>(settings).await });

    overwatch.spawn(async move {
        sleep(Duration::from_secs(1)).await;
        handle2.shutdown().await;
    });

    overwatch.wait_finished();
}
