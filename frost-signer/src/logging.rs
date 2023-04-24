use tracing_subscriber::layer::SubscriberExt;

pub fn initiate_tracing_subscriber() -> Result<(), tracing::subscriber::SetGlobalDefaultError> {
    let env_filter = tracing_subscriber::filter::EnvFilter::from_default_env();

    let subscriber = tracing_subscriber::FmtSubscriber::new().with(env_filter);

    tracing::subscriber::set_global_default(subscriber)
}
