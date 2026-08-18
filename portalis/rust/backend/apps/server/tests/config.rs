use portalis_nexus_server::{DEFAULT_LISTEN_ADDR, ServerConfig, Storage};

#[test]
fn a_server_told_nothing_about_storage_reports_it_and_names_the_option() {
    let config = ServerConfig::from_listen_value(None).expect("the default address is valid");
    let error = config
        .storage()
        .expect_err("a server without durable storage must not start");

    assert_eq!(config.listen_addr.to_string(), DEFAULT_LISTEN_ADDR);
    // The variable is named, because the service does not pick a storage
    // location for you (ADR-0002).
    assert!(error.to_string().contains("PORTALIS_NEXUS_DATA_DIR"));
    assert!(std::error::Error::source(&error).is_none());
}

#[test]
fn a_configured_data_directory_is_the_engine() {
    let embedded = ServerConfig {
        listen_addr: DEFAULT_LISTEN_ADDR.parse().expect("the default is valid"),
        data_dir: Some(std::path::PathBuf::from("/var/lib/portalis")),
        node_secret: None,
    };

    assert_eq!(
        embedded.storage(),
        Ok(Storage::Embedded {
            data_dir: std::path::PathBuf::from("/var/lib/portalis"),
        })
    );
}
