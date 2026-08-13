use portalis_nexus_server::{DEFAULT_DATABASE, DEFAULT_LISTEN_ADDR, ServerConfig, Storage};

#[test]
fn a_server_told_nothing_about_storage_reports_it_and_names_both_options() {
    let config = ServerConfig::from_listen_value(None).expect("the default address is valid");
    let error = config
        .storage()
        .expect_err("a server without durable storage must not start");

    assert_eq!(config.listen_addr.to_string(), DEFAULT_LISTEN_ADDR);
    assert_eq!(config.database, DEFAULT_DATABASE);
    // Both are named, because the point of having two engines is that the
    // service does not pick one for you.
    assert!(error.to_string().contains("PORTALIS_NEXUS_DATA_DIR"));
    assert!(error.to_string().contains("PORTALIS_NEXUS_MONGODB_URI"));
    assert!(std::error::Error::source(&error).is_none());
}

#[test]
fn either_engine_can_be_the_one_configured() {
    let mongo = ServerConfig {
        listen_addr: DEFAULT_LISTEN_ADDR.parse().expect("the default is valid"),
        mongodb_uri: Some("mongodb://nexus.example/".to_owned()),
        data_dir: None,
        node_secret: None,
        database: "nexus".to_owned(),
    };
    let embedded = ServerConfig {
        mongodb_uri: None,
        data_dir: Some(std::path::PathBuf::from("/var/lib/portalis")),
        ..mongo.clone()
    };

    assert_eq!(
        mongo.storage(),
        Ok(Storage::Mongo {
            uri: "mongodb://nexus.example/".to_owned(),
            database: "nexus".to_owned(),
        })
    );
    assert_eq!(
        embedded.storage(),
        Ok(Storage::Embedded {
            data_dir: std::path::PathBuf::from("/var/lib/portalis"),
        })
    );
}
