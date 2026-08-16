//! Step 8 — two devices, one machine, real QUIC, and no service anywhere.
//!
//! Not a service that is running and unused: one that was never started, and
//! whose absence nothing here notices. Both endpoints are real, the bytes
//! cross a real QUIC connection with TLS and ALPN.
//!
//! Two things this shows beyond "it works". **An unknown peer is refused at
//! the handshake**, before the request, so a stranger cannot learn which
//! collections exist by asking. **Security is reported as it is** — direct or
//! relayed, and how well the remote key is known — both read off the
//! connection rather than assumed.
//!
//! Run with `cargo run -p portalis-nexus-demo --bin 08-two-peers`.

use portalis_nexus_client::{
    Discovery, EndpointAddr, KnownPeers, NEXUS_ALPN, NexusEndpoint, PeerTrust, RelayMode, Request,
    Session, SessionError,
};
use portalis_nexus_demo::{Core, NOW, Person, a_collection_with, decode, encode, section, short};

const NAME: &str = "Iceland, 2019";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let ada = Core::new("Ada", 1);
    let mut mira = Core::new("Mira", 2);
    let stranger = Person::new("a stranger", 9);

    section("No service is running");
    println!("  Nothing was started: no address registered, no handle resolved,");
    println!("  no mailbox polled. Two endpoints, on one network.");
    let ada_endpoint = endpoint(&ada.person).await?;
    let mira_endpoint = endpoint(&mira.person).await?;
    println!("  Ada  {}", short(ada_endpoint.id().as_bytes()));
    println!("  Mira {}", short(mira_endpoint.id().as_bytes()));

    section("Ada publishes, and serves whoever she knows");
    let (collection, descriptors) = a_collection_with(NAME, &ada.person, 2);
    let (collection, publication) = ada.publish_to(&collection, &[&ada, &mira], &descriptors, NOW);
    let bytes = encode(&publication);
    println!(
        "  revision {} · {} bytes on the wire",
        collection.number(),
        bytes.len()
    );

    // Where to send packets. On a local network this comes from discovery; a
    // service would supply it for a device elsewhere, which is the one thing
    // a service is for.
    let address = ada_endpoint.addr_when_ready().await;
    let serving = tokio::spawn(serve(
        ada_endpoint,
        KnownPeers::new().verified(mira.person.public_key()),
        bytes,
    ));

    section("Mira connects, asks, and verifies");
    let session = Session::connect(
        &mira_endpoint,
        address.clone(),
        &KnownPeers::new().verified(ada.person.public_key()),
    )
    .await?;
    let security = session.security();
    println!(
        "  path {:?} · peer {:?} — read off the handshake",
        security.path, security.peer
    );
    assert_eq!(security.peer, PeerTrust::Known);

    let answer = session
        .request(Request::Publication {
            collection_id: *collection.id.as_bytes(),
        })
        .await?;
    let received = mira.follow(&decode(&answer)?, &ada, NAME).await?;
    println!(
        "  verified revision {} · {} entries",
        received.collection.number(),
        received.descriptors.len()
    );
    println!("  The same verification as step 7. Arriving over QUIC rather than");
    println!("  a function call changes nothing about an object.");
    assert_eq!(received.descriptors, descriptors);
    session.close();

    section("A stranger is refused");
    refuse_the_stranger(&stranger, &ada, address, *collection.id.as_bytes()).await?;

    serving.abort();
    println!("\nTwo devices shared a collection over QUIC with no service in");
    println!("existence. A service is for reaching devices that are not on your");
    println!("network — not for making this work.");
    Ok(())
}

/// Ada's side: accept whoever connects, answer the ones she knows.
async fn serve(endpoint: NexusEndpoint, known: KnownPeers, publication: Vec<u8>) {
    while let Some(incoming) = endpoint.accept().await {
        let Ok(connection) = incoming.await else {
            continue;
        };
        // An unknown peer never becomes a session, so there is nothing to
        // answer with and no request is ever read.
        let Ok(session) = Session::accept(&endpoint, connection, &known) else {
            continue;
        };
        while let Ok((Request::Publication { .. }, responder)) = session.next_request().await {
            let _ = responder.answer(&publication).await;
        }
    }
}

/// Someone who has Ada's address and key, and no place in her contacts.
async fn refuse_the_stranger(
    stranger: &Person,
    ada: &Core,
    address: EndpointAddr,
    collection_id: [u8; 16],
) -> anyhow::Result<()> {
    let endpoint = endpoint(stranger).await?;
    let knows_ada = KnownPeers::new().verified(ada.person.public_key());

    match Session::connect(&endpoint, address, &knows_ada).await {
        Err(SessionError::UnknownPeer) => println!("  refused at the handshake"),
        Err(error) => println!("  refused: {error}"),
        Ok(session) => {
            // They reach Ada, but Ada does not know them, so her side never
            // becomes a session and the request goes unanswered.
            let asked = tokio::time::timeout(
                std::time::Duration::from_millis(500),
                session.request(Request::Publication { collection_id }),
            )
            .await;
            println!("  they dialled Ada and asked; she never answered");
            anyhow::ensure!(
                asked.is_err() || asked.is_ok_and(|inner| inner.is_err()),
                "an unknown peer must get nothing"
            );
        }
    }
    println!("  Refused before the request, so a stranger cannot learn which");
    println!("  collections exist by asking about them.");
    Ok(())
}

async fn endpoint(person: &Person) -> anyhow::Result<NexusEndpoint> {
    // No relay: a relay is a service of sorts, and on one network there is
    // nothing for one to do. No discovery either — this demo hands one peer's
    // address to the other directly, so there is nothing left to look up, and
    // a demo that reached for multicast or a name server would depend on the
    // network it happens to be run on.
    Ok(NexusEndpoint::bind(
        person.secret_bytes(),
        vec![NEXUS_ALPN.to_vec()],
        RelayMode::Disabled,
        Discovery::Disabled,
    )
    .await?)
}
