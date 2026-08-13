//! Step 11 — two devices that cannot see each other, and one that is asleep.
//!
//! Step 8 showed sharing with no service in existence. These are the two cases
//! it cannot cover: Mira is on another network, and later her phone is in her
//! pocket. Both need something in the middle.
//!
//! What matters is how little that something is. It speaks the same session
//! vocabulary a peer does — a service is a peer that also stores — and every
//! object it carries is opaque to it. It never learns which collection, which
//! members, or a byte of content. Mira's verification is the same code as
//! step 7, because an object is valid on its own terms and where it waited
//! changes nothing.
//!
//! Run with `cargo run -p portalis-nexus-demo --bin 11-service`.

use portalis_nexus_client::{KnownPeers, NEXUS_ALPN, NexusEndpoint, RelayMode, Request, Session};
use portalis_nexus_demo::{Core, NOW, Person, a_collection_with, decode, encode, section};
use portalis_nexus_protocol::derive_device_id;
use portalis_nexus_storage::embedded::Embedded;
use portalis_nexus_storage::service::{Service, unpack};

const NAME: &str = "Iceland, 2019";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let directory = std::env::temp_dir().join(format!("portalis-service-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory)?;

    let ada = Core::new("Ada", 1);
    let mut mira = Core::new("Mira", 2);
    let operator = Person::new("the service", 7);
    let mira_device = derive_device_id(&mira.person.public_key());

    section("A service, and the little it knows");
    let store = Embedded::open(directory.join("service.redb"))?;
    let service = Service::new(store);
    let endpoint = bind(&operator).await?;
    let address = endpoint.addr_when_ready().await;
    println!("  one file, no replica set, and a QUIC listener");
    println!("  It answers the same requests a peer does, and can read none of");
    println!("  what it carries.");

    // The service talks to anybody: it is a post office, not a friend.
    let serving = tokio::spawn(serve(endpoint, service));

    section("Ada publishes; Mira is on another network");
    let (collection, descriptors) = a_collection_with(NAME, &ada.person, 2);
    let (collection, publication) = ada.publish_to(&collection, &[&ada, &mira], &descriptors, NOW);
    let body = encode(&publication);

    let ada_endpoint = bind(&ada.person).await?;
    let to_service = KnownPeers::new().verified(operator.public_key());
    let session = Session::connect(&ada_endpoint, address.clone(), &to_service).await?;
    session
        .request(Request::Deliver {
            device: mira_device,
            body: body.clone(),
        })
        .await?;
    println!(
        "  revision {} · {} bytes left for a device that is not there",
        collection.number(),
        body.len()
    );
    session.close();

    section("Mira's phone comes back");
    let mira_endpoint = bind(&mira.person).await?;
    let session = Session::connect(&mira_endpoint, address, &to_service).await?;
    let waiting = unpack(&session.request(Request::Collect).await?)?;
    println!("  collected {} item(s)", waiting.len());

    let received = mira.follow(&decode(&waiting[0])?, &ada, NAME).await?;
    println!(
        "  verified revision {} · {} entries · role {:?}",
        received.collection.number(),
        received.descriptors.len(),
        received.collection.role
    );
    println!("  The same verification as step 7, unchanged. It does not matter");
    println!("  that these bytes waited in a service rather than arriving from");
    println!("  Ada directly.");
    assert_eq!(received.descriptors, descriptors);

    section("Collecting empties it");
    let again = unpack(&session.request(Request::Collect).await?)?;
    println!("  a second collect → {} item(s)", again.len());
    assert!(again.is_empty());
    session.close();

    serving.abort();
    let _ = std::fs::remove_dir_all(&directory);

    section("And the service stayed optional");
    println!("  Step 8's demo runs with no service in existence at all. This");
    println!("  one exists for the cases that cannot: a device on another");
    println!("  network, and a device that is asleep.");

    println!("\nTwo devices that never saw each other completed a share, through");
    println!("something that could not read a byte of it.");
    Ok(())
}

/// The service's side: answer whoever connects, for as long as it runs.
async fn serve(endpoint: NexusEndpoint, service: Service) {
    // A post office talks to anybody. What a caller may *do* is decided by
    // the request, and by which device the session authenticated.
    let anybody = KnownPeers::new().to_anybody();
    while let Some(incoming) = endpoint.accept().await {
        let Ok(connection) = incoming.await else {
            continue;
        };
        let Ok(session) = Session::accept(&endpoint, connection, &anybody) else {
            continue;
        };
        // Whose mailbox to drain comes from the authenticated key, never from
        // the request — a service that let a caller name someone else's would
        // hand anybody anybody's post.
        let caller = derive_device_id(&session.remote());
        while let Ok((request, responder)) = session.next_request().await {
            let answer = service
                .answer(caller, &request)
                .map(|answer| answer.0)
                .unwrap_or_default();
            let _ = responder.answer(&answer).await;
        }
    }
}

async fn bind(person: &Person) -> anyhow::Result<NexusEndpoint> {
    Ok(NexusEndpoint::bind(
        person.secret_bytes(),
        vec![NEXUS_ALPN.to_vec()],
        RelayMode::Disabled,
    )
    .await?)
}
