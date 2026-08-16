//! Whether this device is actually talking to a Nexus service.
//!
//! `Connectivity` used to be derived from whether the app was in the
//! foreground: it read `Connecting` whenever the app was active and
//! `LocalOnly` whenever it was not, without a socket ever being opened. A
//! person who had configured no service at all was told the app was
//! connecting to one, forever.
//!
//! This is the one place that answers the question, and it answers it by
//! having tried. Nothing here infers a connection from a setting, from a
//! lifecycle flag, or from the absence of an error.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::watch;

use crate::core::events::{Connectivity, Path, PeerTrust, Security};
use crate::projection::state::PortalisState;

/// How long to wait before dialling again after a failure, and how often to
/// check that a live connection is still live.
///
/// One interval for both because they are the same question asked from two
/// states, and a person watching a status line is owed an answer at the same
/// pace either way.
pub const RETRY_INTERVAL: Duration = Duration::from_secs(5);

/// How much is known about who is on the other end of a service connection.
///
/// Known because the person configured it: pasting a Node ID *is* comparing a
/// fingerprint, and it is the only comparison this relationship has.
const SERVICE_PEER: PeerTrust = PeerTrust::Known;

/// What the app is able to reach right now.
///
/// Separated from the dialling so the decision can be tested without a
/// network: given what was configured and what happened when it was tried,
/// this is what a person should be told.
///
/// `path` is passed in rather than assumed. It used to be the constant
/// `Path::Direct`, on the reasoning that the app dialled a direct address and
/// the service refused relays — true at the time, and a claim the code had no
/// way to notice becoming false. Now that a service can be found by Node ID
/// and reached through a relay, the only honest source for this is the
/// transport.
#[must_use]
pub fn connectivity_for(
    configured: bool,
    reached: Option<Path>,
    since_unix_ns: u64,
) -> Connectivity {
    match (configured, reached) {
        // Nothing to reach is not a failure to reach it. A first run has no
        // service and is working exactly as intended.
        (false, _) => Connectivity::LocalOnly,
        (true, Some(path)) => Connectivity::Online(Security {
            path,
            peer: SERVICE_PEER,
        }),
        // Configured and unreachable. Degraded rather than LocalOnly: the
        // person expects a service, so saying "local only" would describe the
        // symptom as though it were the arrangement.
        (true, None) => Connectivity::Degraded { since_unix_ns },
    }
}

/// Keeps the projection's connectivity equal to what a session can do.
pub(crate) async fn follow_service(
    states: watch::Sender<PortalisState>,
    endpoint: Arc<dyn ConfiguredEndpoint>,
    mut shutdown: super::supervisor::Shutdown,
) {
    let mut session: Option<Arc<dyn LiveSession>> = None;
    let mut failing_since = 0_u64;

    loop {
        let configured = endpoint.configured();
        // A live session that has stopped being live is not a live session.
        // Ask rather than remember: the socket knows and this does not.
        if session.as_ref().is_some_and(|open| !open.is_live()) {
            session = None;
        }
        if !configured {
            session = None;
            // A handle belongs to the service that issued it. Pointing the app
            // somewhere else, or nowhere, does not leave this device still
            // holding the name it was given.
            publish_handle(&states, None);
        } else if session.is_none() {
            // Announced before dialling, because a dial can take a while and
            // silence during it reads as nothing happening.
            //
            // Only while there is still reason to expect it to work, though.
            // Every attempt after a failure used to re-announce this, so a
            // service that was not there at all spent most of each cycle
            // claiming to be connecting — a dial takes longer to time out
            // than the pause between tries. A person watching that sees
            // something perpetually about to succeed, when what is true is
            // that it failed and is being retried, which the failed state
            // already says.
            if failing_since == 0 {
                publish(&states, Connectivity::Connecting);
            }
            let established = tokio::select! {
                () = shutdown.requested() => return,
                opened = endpoint.establish() => opened,
            };
            if let Some(identified) = established {
                publish_handle(&states, Some(identified.handle));
                session = Some(identified.session);
            }
        } else if let Some(open) = session.as_ref() {
            // The keep-alive. A socket that is merely open proves nothing:
            // this is the round trip that tells the service the device is
            // still here — which is what its presence for everyone else is
            // derived from — and tells this device the service still answers.
            let answered = tokio::select! {
                () = shutdown.requested() => return,
                answered = open.ping() => answered,
            };
            if !answered {
                session = None;
            }
        }

        // Asked every pass, not once at connect: a session that began relayed
        // and has since found a direct path is a different thing to report,
        // and nothing about it announces itself.
        let reached = session.as_ref().map(|open| open.path());
        if reached.is_some() {
            failing_since = 0;
        } else if configured && failing_since == 0 {
            failing_since = crate::core::transfers::unix_time_ns();
        }
        publish(
            &states,
            connectivity_for(configured, reached, failing_since),
        );

        tokio::select! {
            () = shutdown.requested() => return,
            () = tokio::time::sleep(RETRY_INTERVAL) => {}
        }
    }
}

fn publish(states: &watch::Sender<PortalisState>, connectivity: Connectivity) {
    states.send_if_modified(|state| {
        if state.connectivity == connectivity {
            return false;
        }
        state.connectivity = connectivity;
        true
    });
}

fn publish_handle(states: &watch::Sender<PortalisState>, handle: Option<String>) {
    states.send_if_modified(|state| {
        if state.device.handle == handle {
            return false;
        }
        state.device.handle = handle;
        true
    });
}

/// A service connection this device has proved its identity on.
///
/// The handle comes with it because the two are inseparable: the service
/// issues it as the answer to "who is this device", so there is no moment
/// where a session exists and the name it was given does not.
pub(crate) struct Identified {
    /// This device's `username#discriminator`, as the service assigned it.
    pub handle: String,
    pub session: Arc<dyn LiveSession>,
}

/// What this device has been told to talk to, and how to reach it.
///
/// A trait so the worker can be driven without a network. Dialling a real
/// service in a unit test would make the test a measurement of somebody's
/// wifi, and the thing worth testing is what a person is told — which is
/// decided here, not in the socket.
#[async_trait::async_trait]
pub(crate) trait ConfiguredEndpoint: Send + Sync {
    /// Whether a service has been set up at all.
    fn configured(&self) -> bool;

    /// Dials it and proves who this device is, answering `None` when either
    /// half did not work.
    ///
    /// Both, rather than dialling alone: an unauthenticated connection cannot
    /// make a single request the app has any use for, so reporting it as a
    /// reached service would put `Online` back to meaning less than it says.
    async fn establish(&self) -> Option<Identified>;
}

/// An established session, for as long as it lasts.
#[async_trait::async_trait]
pub(crate) trait LiveSession: Send + Sync {
    /// Whether the socket is still open.
    fn is_live(&self) -> bool;

    /// The path traffic is taking right now.
    fn path(&self) -> Path;

    /// Round-trips a keep-alive, answering `false` when the service did not.
    async fn ping(&self) -> bool;
}

/// The real one: whatever is in the device's settings, dialled for real.
pub(crate) struct Configured;

#[async_trait::async_trait]
impl ConfiguredEndpoint for Configured {
    fn configured(&self) -> bool {
        crate::nexus_settings::nexus_endpoint_config()
            .ok()
            .and_then(|config| config.endpoint_addr().ok().flatten())
            .is_some()
    }

    async fn establish(&self) -> Option<Identified> {
        let client = match crate::nexus::connect_configured().await {
            Ok(client) => client?,
            Err(error) => {
                crate::log::clog!("nexus", "could not reach the Nexus service: {error:#}");
                return None;
            }
        };
        let identity = match crate::device::current_nexus_identity() {
            Ok(identity) => identity,
            Err(error) => {
                crate::log::clog!("nexus", "this device has no usable identity: {error:#}");
                return None;
            }
        };
        let device_name = crate::device::device_identity()
            .map(|device| device.nickname)
            .unwrap_or_default();
        match crate::nexus::identify(&client, &identity, &device_name).await {
            Ok(handle) => {
                crate::log::clog!("nexus", "this device is {handle}");
                Some(Identified {
                    handle,
                    session: Arc::new(Connected(client)),
                })
            }
            Err(error) => {
                crate::log::clog!("nexus", "the Nexus service refused this device: {error}");
                None
            }
        }
    }
}

/// A real client, held for as long as the service keeps answering.
struct Connected(portalis_nexus_client::NexusClient);

#[async_trait::async_trait]
impl LiveSession for Connected {
    fn is_live(&self) -> bool {
        self.0.is_connected()
    }

    fn path(&self) -> Path {
        // Only a confirmed direct path counts as direct. `Mixed` means part
        // of the traffic is relayed, and `Unavailable` means nothing is
        // confirmed yet — overstating either would tell a person their data
        // is taking a route it may not be.
        match self.0.path() {
            portalis_nexus_client::ConnectionPath::Direct => Path::Direct,
            portalis_nexus_client::ConnectionPath::Relay
            | portalis_nexus_client::ConnectionPath::Mixed
            | portalis_nexus_client::ConnectionPath::Unavailable => Path::Relayed,
        }
    }

    async fn ping(&self) -> bool {
        // The nonce only has to differ between one ping and the next; the
        // client matches the reply by message id, not by this.
        self.0
            .ping(crate::core::transfers::unix_time_ns())
            .await
            .is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every answer this worker can give, and why each is the honest one.
    #[test]
    fn what_a_person_is_told_follows_from_what_was_tried() {
        assert_eq!(
            connectivity_for(false, None, 0),
            Connectivity::LocalOnly,
            "a first run has no service, which is not a failure to reach one"
        );
        assert_eq!(
            connectivity_for(true, Some(Path::Direct), 0),
            Connectivity::Online(Security {
                path: Path::Direct,
                peer: SERVICE_PEER
            })
        );
        assert_eq!(
            connectivity_for(true, Some(Path::Relayed), 0),
            Connectivity::Online(Security {
                path: Path::Relayed,
                peer: SERVICE_PEER
            }),
            "a relayed service is reached, and saying so is the whole point"
        );
        assert_eq!(
            connectivity_for(true, None, 42),
            Connectivity::Degraded { since_unix_ns: 42 },
            "configured and unreachable is a fault, not an arrangement"
        );
    }

    /// A configured service that cannot be reached says so, and keeps saying
    /// so — the app used to report Connecting forever whether or not anything
    /// was listening, and whether or not anything had been configured.
    #[tokio::test]
    async fn an_unreachable_service_is_reported_rather_than_awaited() {
        struct Unreachable;

        #[async_trait::async_trait]
        impl ConfiguredEndpoint for Unreachable {
            fn configured(&self) -> bool {
                true
            }
            async fn establish(&self) -> Option<Identified> {
                None
            }
        }

        let states = watch::Sender::new(PortalisState {
            connectivity: Connectivity::LocalOnly,
            ..empty_state()
        });
        let mut watching = states.subscribe();
        let (stop, shutdown) = shutdown_pair();
        let worker = tokio::spawn(follow_service(states, Arc::new(Unreachable), shutdown));

        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if matches!(
                    watching.borrow_and_update().connectivity,
                    Connectivity::Degraded { .. }
                ) {
                    return;
                }
                watching.changed().await.expect("the worker is running");
            }
        })
        .await
        .expect("an unreachable service is reported");

        // And stays reported. Retrying is not news, and a status that flips
        // back to "connecting" before every attempt describes a service that
        // is not there as one that is nearly here.
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                watching.changed().await.expect("the worker is running");
                assert_ne!(
                    watching.borrow_and_update().connectivity,
                    Connectivity::Connecting,
                    "a retry after a failure must not read as a first attempt"
                );
            }
        })
        .await
        .expect_err("the worker keeps retrying without saying anything new");

        let _ = stop.send(true);
        let _ = worker.await;
    }

    /// Nothing configured is local-only, and stays that way without dialling.
    #[tokio::test]
    async fn nothing_configured_is_not_a_connection_being_attempted() {
        struct Absent;

        #[async_trait::async_trait]
        impl ConfiguredEndpoint for Absent {
            fn configured(&self) -> bool {
                false
            }
            async fn establish(&self) -> Option<Identified> {
                panic!("nothing configured must not be dialled");
            }
        }

        let states = watch::Sender::new(PortalisState {
            connectivity: Connectivity::Connecting,
            ..empty_state()
        });
        let mut watching = states.subscribe();
        let (stop, shutdown) = shutdown_pair();
        let worker = tokio::spawn(follow_service(states, Arc::new(Absent), shutdown));

        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if watching.borrow_and_update().connectivity == Connectivity::LocalOnly {
                    return;
                }
                watching.changed().await.expect("the worker is running");
            }
        })
        .await
        .expect("an unconfigured device is local only");

        let _ = stop.send(true);
        let _ = worker.await;
    }

    /// A session that is whatever a test says it is.
    struct Fake {
        /// Whether pings are answered. Turning this off is a service that
        /// stopped answering without the socket noticing.
        answers: std::sync::atomic::AtomicBool,
        pings: std::sync::atomic::AtomicUsize,
        /// Relayed first, so a test can flip it and stand for iroh upgrading
        /// a connection to a direct path partway through.
        direct: std::sync::atomic::AtomicBool,
    }

    impl Fake {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                answers: std::sync::atomic::AtomicBool::new(true),
                pings: std::sync::atomic::AtomicUsize::new(0),
                direct: std::sync::atomic::AtomicBool::new(false),
            })
        }
    }

    #[async_trait::async_trait]
    impl LiveSession for Fake {
        fn is_live(&self) -> bool {
            true
        }
        fn path(&self) -> Path {
            if self.direct.load(std::sync::atomic::Ordering::SeqCst) {
                Path::Direct
            } else {
                Path::Relayed
            }
        }
        async fn ping(&self) -> bool {
            self.pings.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.answers.load(std::sync::atomic::Ordering::SeqCst)
        }
    }

    struct Reachable(Arc<Fake>);

    #[async_trait::async_trait]
    impl ConfiguredEndpoint for Reachable {
        fn configured(&self) -> bool {
            true
        }
        async fn establish(&self) -> Option<Identified> {
            Some(Identified {
                handle: "ada#7Q4K".to_owned(),
                session: Arc::clone(&self.0) as Arc<dyn LiveSession>,
            })
        }
    }

    /// The handle is the service's answer to who this device is, so it
    /// appears when a session does — the app cannot know it any other way.
    #[tokio::test]
    async fn a_device_learns_its_handle_by_identifying_itself() {
        let session = Fake::new();
        let states = watch::Sender::new(empty_state());
        let mut watching = states.subscribe();
        let (stop, shutdown) = shutdown_pair();
        let worker = tokio::spawn(follow_service(
            states,
            Arc::new(Reachable(Arc::clone(&session))),
            shutdown,
        ));

        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                {
                    let state = watching.borrow_and_update();
                    if state.device.handle.as_deref() == Some("ada#7Q4K") {
                        assert!(
                            matches!(state.connectivity, Connectivity::Online(_)),
                            "a session that identified this device is a reached service"
                        );
                        return;
                    }
                }
                watching.changed().await.expect("the worker is running");
            }
        })
        .await
        .expect("an identified device knows its handle");

        let _ = stop.send(true);
        let _ = worker.await;
    }

    /// iroh reaches a peer through a relay while it negotiates a direct path,
    /// then switches without the connection dropping. The reported path has
    /// to follow that, which is exactly what a value read once at connect
    /// time could not do — and what the old `Path::Direct` constant asserted
    /// regardless.
    #[tokio::test]
    async fn a_connection_upgraded_to_direct_stops_being_reported_as_relayed() {
        let session = Fake::new();
        let states = watch::Sender::new(empty_state());
        let mut watching = states.subscribe();
        let (stop, shutdown) = shutdown_pair();
        let worker = tokio::spawn(follow_service(
            states,
            Arc::new(Reachable(Arc::clone(&session))),
            shutdown,
        ));

        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if matches!(
                    watching.borrow_and_update().connectivity,
                    Connectivity::Online(Security {
                        path: Path::Relayed,
                        ..
                    })
                ) {
                    return;
                }
                watching.changed().await.expect("the worker is running");
            }
        })
        .await
        .expect("a relayed session is reported as relayed");

        session
            .direct
            .store(true, std::sync::atomic::Ordering::SeqCst);

        tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                watching.changed().await.expect("the worker is running");
                if matches!(
                    watching.borrow_and_update().connectivity,
                    Connectivity::Online(Security {
                        path: Path::Direct,
                        ..
                    })
                ) {
                    return;
                }
            }
        })
        .await
        .expect("the upgrade to a direct path is noticed");

        let _ = stop.send(true);
        let _ = worker.await;
    }

    /// A socket can stay open against a service that has stopped answering.
    /// The keep-alive is what notices, and what a person is told follows it.
    #[tokio::test]
    async fn a_service_that_stops_answering_stops_being_online() {
        let session = Fake::new();
        let states = watch::Sender::new(empty_state());
        let mut watching = states.subscribe();
        let (stop, shutdown) = shutdown_pair();
        let worker = tokio::spawn(follow_service(
            states,
            Arc::new(Reachable(Arc::clone(&session))),
            shutdown,
        ));

        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if matches!(
                    watching.borrow_and_update().connectivity,
                    Connectivity::Online(_)
                ) {
                    return;
                }
                watching.changed().await.expect("the worker is running");
            }
        })
        .await
        .expect("the session starts out reached");

        session
            .answers
            .store(false, std::sync::atomic::Ordering::SeqCst);

        tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                watching.changed().await.expect("the worker is running");
                if matches!(
                    watching.borrow_and_update().connectivity,
                    Connectivity::Degraded { .. }
                ) {
                    return;
                }
            }
        })
        .await
        .expect("a service that stopped answering is reported");

        assert!(
            session.pings.load(std::sync::atomic::Ordering::SeqCst) > 0,
            "the keep-alive is what proves a session, so it has to be sent"
        );

        let _ = stop.send(true);
        let _ = worker.await;
    }

    /// A shutdown signal a test can pull, without a whole supervisor.
    fn shutdown_pair() -> (watch::Sender<bool>, super::super::supervisor::Shutdown) {
        let (stop, signal) = watch::channel(false);
        (
            stop,
            super::super::supervisor::Shutdown::from_signal(signal),
        )
    }

    fn empty_state() -> PortalisState {
        PortalisState {
            device: crate::projection::state::DeviceState {
                name: String::new(),
                handle: None,
                fingerprint: String::new(),
                devices: 0,
            },
            connectivity: Connectivity::LocalOnly,
            contacts: Vec::new(),
            collections: Vec::new(),
            alerts: Vec::new(),
        }
    }
}

#[cfg(test)]
mod live_tests {
    use super::*;

    /// Dials a service that is actually listening, through the same code the
    /// app uses. Ignored by default because it needs one running — the point
    /// is that "connected" means a handshake completed, and nothing short of
    /// a real one can demonstrate that.
    ///
    /// `PORTALIS_NEXUS_ADDR` is optional. Leaving it out is the stronger run:
    /// it proves the service was found by Node ID alone, which is what a
    /// person setting Portalis up actually has to do.
    ///
    /// ```text
    /// tool/nexus_server.sh
    /// PORTALIS_NEXUS_NODE_ID=… [PORTALIS_NEXUS_ADDR=…] \
    ///   cargo test --lib reaches_a_running_service -- --ignored
    /// ```
    #[tokio::test]
    #[ignore = "needs a service already listening; see tool/nexus_server.sh"]
    async fn reaches_a_running_service() {
        let node_id = std::env::var("PORTALIS_NEXUS_NODE_ID").expect("a node id to dial");
        let endpoint =
            portalis_nexus_client::EndpointAddr::new(node_id.parse().expect("a valid node id"));
        let endpoint = match std::env::var("PORTALIS_NEXUS_ADDR") {
            Ok(address) if !address.trim().is_empty() => endpoint.with_direct_addresses([address
                .trim()
                .parse::<std::net::SocketAddr>()
                .expect("a valid socket address")]),
            _ => endpoint,
        };

        struct Listening(portalis_nexus_client::EndpointAddr);

        #[async_trait::async_trait]
        impl ConfiguredEndpoint for Listening {
            fn configured(&self) -> bool {
                true
            }
            async fn establish(&self) -> Option<Identified> {
                let client = crate::nexus::connect(self.0.clone()).await.ok()?;
                // A fresh keypair each run, rather than this machine's real
                // identity: the test registers, and a device that is already
                // enrolled would exercise only the other half of that.
                let mut secret = [0_u8; 32];
                rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut secret);
                let identity = crate::nexus::NexusIdentity::generate(
                    crate::domain::identity::DeviceIdentity::from_bytes(&secret),
                );
                // Panics rather than retrying: in a live test a service that
                // refuses this device is the result, and swallowing it would
                // leave the loop reporting a timeout instead of the reason.
                let handle = crate::nexus::identify(&client, &identity, "Ada's laptop")
                    .await
                    .expect("the service accepts a device it has never seen");
                Some(Identified {
                    handle,
                    session: Arc::new(super::Connected(client)),
                })
            }
        }

        let states = watch::Sender::new(crate::projection::state::PortalisState {
            device: crate::projection::state::DeviceState {
                name: String::new(),
                handle: None,
                fingerprint: String::new(),
                devices: 0,
            },
            connectivity: Connectivity::LocalOnly,
            contacts: Vec::new(),
            collections: Vec::new(),
            alerts: Vec::new(),
        });
        let mut watching = states.subscribe();
        let (stop, signal) = watch::channel(false);
        let worker = tokio::spawn(follow_service(
            states,
            Arc::new(Listening(endpoint)),
            super::super::supervisor::Shutdown::from_signal(signal),
        ));

        let handle = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                {
                    let state = watching.borrow_and_update();
                    if matches!(state.connectivity, Connectivity::Online(_)) {
                        return state.device.handle.clone();
                    }
                }
                watching.changed().await.expect("the worker is running");
            }
        })
        .await
        .expect("a listening service is reached");

        let handle = handle.expect("a reached service has already said who this device is");
        let (username, discriminator) = handle
            .split_once('#')
            .unwrap_or_else(|| panic!("the service issued {handle}, which is not a handle"));
        assert!(!username.is_empty(), "a handle names somebody");
        assert!(
            !discriminator.is_empty(),
            "the discriminator is what makes {username} findable"
        );

        let _ = stop.send(true);
        let _ = worker.await;
    }
}
