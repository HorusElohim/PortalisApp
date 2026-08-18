//! One suite, one engine.
//!
//! A storage seam (ADR-0002) means nothing unless its one implementation
//! answers every question the same way every time. So these tests are
//! written against `server-core`'s repository traits and know nothing about
//! redb or a file — they run against `Embedded`, the one engine a node
//! actually runs.
//!
//! What is deliberately *not* here: anything about how the store is built. A
//! suite that had to grow branches per engine would be the thing an earlier
//! version of this file existed to prevent when there were two.

use portalis_nexus_server_core::{
    DeviceRecord, EnvelopeRepository, FriendRepository, FriendshipEdge, FriendshipRecord,
    IdentityRepository, KeyEnvelopeRecord, RepositoryError, ShareMembershipRecord, ShareRecord,
    ShareRepository, ShareSnapshotRecord, UserDirectory, UserRecord,
};
use portalis_nexus_storage::embedded::Embedded;

const ADA: [u8; 16] = [1; 16];
const GRACE: [u8; 16] = [2; 16];
const SHARE: [u8; 16] = [3; 16];
const OTHER_SHARE: [u8; 16] = [4; 16];

fn user(id: [u8; 16], username: &str, discriminator: &str) -> UserRecord {
    UserRecord {
        user_id: id,
        username: username.to_owned(),
        normalized_username: username.to_lowercase(),
        discriminator: discriminator.to_owned(),
        created_at_unix_ns: 1,
    }
}

fn device(id: u8, owner: [u8; 16]) -> DeviceRecord {
    DeviceRecord {
        device_id: [id; 32],
        user_id: owner,
        public_key: [id; 32],
        encryption_public_key: [id; 32],
        created_at_unix_ns: 1,
        last_authenticated_at_unix_ns: None,
        revoked_at_unix_ns: None,
    }
}

fn share(id: [u8; 16], revision: u64) -> ShareRecord {
    ShareRecord {
        share_id: id,
        owner: ADA,
        revision,
        snapshot_id: [7; 32],
        capsule: b"sealed".to_vec(),
        capsule_signature: vec![9; 64],
        created_at_unix_ns: 1,
        updated_at_unix_ns: revision,
    }
}

fn snapshot(id: [u8; 16], revision: u64) -> ShareSnapshotRecord {
    ShareSnapshotRecord {
        share_id: id,
        revision,
        snapshot_id: [7; 32],
        capsule: b"sealed".to_vec(),
        capsule_signature: vec![9; 64],
        created_at_unix_ns: revision,
    }
}

/// Opens a scratch `Embedded` store under a name unique to `name` and the
/// running process, and cleans it up once `suite` returns.
async fn against_the_engine<F, Fut>(name: &str, suite: F)
where
    F: FnOnce(Embedded) -> Fut,
    Fut: Future<Output = ()>,
{
    let directory = std::env::temp_dir().join(format!(
        "portalis-conformance-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("a scratch directory");
    suite(Embedded::open(directory.join("service.redb")).expect("opens")).await;
    let _ = std::fs::remove_dir_all(&directory);
}

#[tokio::test]
async fn a_registration_is_all_or_nothing() {
    against_the_engine("registration", |store| async move {
        store
            .insert_registration(user(ADA, "Ada", "7Q2XZ"), device(1, ADA))
            .await
            .expect("registers");

        assert_eq!(
            store.find_user(ADA).await.expect("reads"),
            Some(user(ADA, "Ada", "7Q2XZ"))
        );
        assert_eq!(
            store.find_device([1; 32]).await.expect("reads"),
            Some(device(1, ADA))
        );

        // A handle already claimed is refused, and the device that came with
        // it does not survive.
        assert_eq!(
            store
                .insert_registration(user(GRACE, "Ada", "7Q2XZ"), device(2, GRACE))
                .await,
            Err(RepositoryError::HandleTaken)
        );
        assert_eq!(store.find_user(GRACE).await.expect("reads"), None);
        assert_eq!(store.find_device([2; 32]).await.expect("reads"), None);
    })
    .await;
}

#[tokio::test]
async fn a_handle_finds_its_user() {
    against_the_engine("handles", |store| async move {
        store
            .insert_registration(user(ADA, "Ada", "7Q2XZ"), device(1, ADA))
            .await
            .expect("registers");

        assert_eq!(
            store
                .find_user_by_handle("ada", "7Q2XZ")
                .await
                .expect("reads"),
            Some(user(ADA, "Ada", "7Q2XZ"))
        );
        // The discriminator is part of it: the same name is another person.
        assert_eq!(
            store
                .find_user_by_handle("ada", "0000")
                .await
                .expect("reads"),
            None
        );
    })
    .await;
}

#[tokio::test]
async fn devices_are_linked_listed_touched_and_revoked() {
    against_the_engine("devices", |store| async move {
        store
            .insert_registration(user(ADA, "Ada", "7Q2XZ"), device(1, ADA))
            .await
            .expect("registers");
        store.link_device(device(2, ADA)).await.expect("links");

        assert_eq!(store.list_devices(ADA).await.expect("reads").len(), 2);
        assert_eq!(
            store.link_device(device(2, ADA)).await,
            Err(RepositoryError::DeviceExists),
            "the same device twice is refused"
        );

        store.touch_device([1; 32], 42).await.expect("touches");
        assert_eq!(
            store
                .find_device([1; 32])
                .await
                .expect("reads")
                .and_then(|device| device.last_authenticated_at_unix_ns),
            Some(42)
        );

        store.revoke_device([2; 32], 99).await.expect("revokes");
        // Revoking twice says the same thing, and the first time is when
        // authority actually ended.
        store
            .revoke_device([2; 32], 200)
            .await
            .expect("revokes again");
        assert_eq!(
            store
                .find_device([2; 32])
                .await
                .expect("reads")
                .and_then(|device| device.revoked_at_unix_ns),
            Some(99)
        );

        // A device that is not there is ignored, not reported: the caller has
        // already established it exists.
        store.touch_device([9; 32], 1).await.expect("ignores");
        store.revoke_device([9; 32], 1).await.expect("ignores");
    })
    .await;
}

#[tokio::test]
async fn publishing_is_a_compare_and_set() {
    against_the_engine("publish", |store| async move {
        store
            .save_publication(share(SHARE, 1), snapshot(SHARE, 1), None)
            .await
            .expect("creates");
        assert_eq!(
            store.find_share(SHARE).await.expect("reads"),
            Some(share(SHARE, 1))
        );

        // Expecting nothing when something is there.
        assert_eq!(
            store
                .save_publication(share(SHARE, 1), snapshot(SHARE, 1), None)
                .await,
            Err(RepositoryError::VersionConflict)
        );

        store
            .save_publication(share(SHARE, 2), snapshot(SHARE, 2), Some(1))
            .await
            .expect("advances");

        // History is immutable, and a stale expectation loses.
        assert_eq!(
            store
                .save_publication(share(SHARE, 1), snapshot(SHARE, 1), Some(2))
                .await,
            Err(RepositoryError::VersionConflict)
        );
        assert_eq!(
            store
                .save_publication(share(SHARE, 3), snapshot(SHARE, 3), Some(1))
                .await,
            Err(RepositoryError::VersionConflict)
        );
        assert_eq!(
            store.find_snapshot(SHARE, 1).await.expect("reads"),
            Some(snapshot(SHARE, 1))
        );
    })
    .await;
}

#[tokio::test]
async fn membership_is_granted_revoked_and_listed() {
    against_the_engine("membership", |store| async move {
        store
            .save_publication(share(SHARE, 1), snapshot(SHARE, 1), None)
            .await
            .expect("publishes");
        store
            .save_publication(share(OTHER_SHARE, 1), snapshot(OTHER_SHARE, 1), None)
            .await
            .expect("publishes");

        for (collection, member) in [(SHARE, ADA), (SHARE, GRACE), (OTHER_SHARE, GRACE)] {
            store
                .grant_share_access(ShareMembershipRecord {
                    share_id: collection,
                    user_id: member,
                    granted_at_unix_ns: 10,
                })
                .await
                .expect("grants");
        }

        assert!(store.has_share_access(SHARE, ADA).await.expect("reads"));
        let mut members = store.list_share_members(SHARE).await.expect("reads");
        members.sort_unstable();
        assert_eq!(members, vec![ADA, GRACE]);

        let mut readable: Vec<_> = store
            .list_authorized_shares(GRACE)
            .await
            .expect("reads")
            .into_iter()
            .map(|share| share.share_id)
            .collect();
        readable.sort_unstable();
        assert_eq!(readable, vec![SHARE, OTHER_SHARE]);

        store
            .revoke_share_access(SHARE, GRACE)
            .await
            .expect("revokes");
        assert!(!store.has_share_access(SHARE, GRACE).await.expect("reads"));
        assert_eq!(
            store
                .list_authorized_shares(GRACE)
                .await
                .expect("reads")
                .len(),
            1,
            "and the other collection is untouched"
        );
        // Revoking twice is the same statement.
        store
            .revoke_share_access(SHARE, GRACE)
            .await
            .expect("revokes again");
    })
    .await;
}

#[tokio::test]
async fn what_was_never_stored_is_absent() {
    against_the_engine("absent", |store| async move {
        assert_eq!(store.find_user(ADA).await.expect("reads"), None);
        assert_eq!(store.find_device([1; 32]).await.expect("reads"), None);
        assert!(store.list_devices(ADA).await.expect("reads").is_empty());
        assert_eq!(store.find_share(SHARE).await.expect("reads"), None);
        assert_eq!(store.find_snapshot(SHARE, 1).await.expect("reads"), None);
        assert!(!store.has_share_access(SHARE, ADA).await.expect("reads"));
        assert!(
            store
                .list_share_members(SHARE)
                .await
                .expect("reads")
                .is_empty()
        );
        assert!(
            store
                .list_authorized_shares(ADA)
                .await
                .expect("reads")
                .is_empty()
        );
    })
    .await;
}

#[tokio::test]
async fn a_friendship_is_versioned() {
    against_the_engine("friends", |store| async move {
        let edge = FriendshipEdge::between(ADA, GRACE).expect("two different people");
        let requested = FriendshipRecord {
            edge,
            requested_by: ADA,
            state: portalis_nexus_protocol::v1::FriendshipState::Pending,
            version: 1,
            created_at_unix_ns: 1,
            updated_at_unix_ns: 1,
        };

        // Version zero means "must not exist yet", which is how a first
        // request is told apart from an answer to one.
        store
            .save_friendship(requested.clone(), 0)
            .await
            .expect("the first request");
        assert_eq!(
            store.find_friendship(edge).await.expect("reads"),
            Some(requested.clone())
        );

        // A device that read the older version loses rather than overwriting.
        assert_eq!(
            store.save_friendship(requested.clone(), 0).await,
            Err(RepositoryError::VersionConflict)
        );

        let accepted = FriendshipRecord {
            state: portalis_nexus_protocol::v1::FriendshipState::Accepted,
            version: 2,
            updated_at_unix_ns: 2,
            ..requested
        };
        store
            .save_friendship(accepted.clone(), 1)
            .await
            .expect("the answer");

        assert_eq!(
            store.list_friendships(ADA).await.expect("reads"),
            vec![accepted.clone()]
        );
        assert_eq!(
            store.list_friendships(GRACE).await.expect("reads"),
            vec![accepted],
            "either half of the pair finds it"
        );
        assert!(
            store
                .list_friendships([9; 16])
                .await
                .expect("reads")
                .is_empty()
        );
    })
    .await;
}

#[tokio::test]
async fn a_sealed_key_is_replaced_and_paged() {
    against_the_engine("envelopes", |store| async move {
        let envelope = |share: [u8; 16], ciphertext: &[u8]| KeyEnvelopeRecord {
            share_id: share,
            recipient_device_id: [1; 32],
            ephemeral_public_key: [2; 32],
            ciphertext: ciphertext.to_vec(),
            created_at_unix_ns: 1,
        };

        store
            .put_key_envelope(envelope(SHARE, b"first"))
            .await
            .expect("stores");
        // A rotated key replaces rather than accumulating, so a device never
        // has to guess which of several is current.
        store
            .put_key_envelope(envelope(SHARE, b"rotated"))
            .await
            .expect("replaces");
        store
            .put_key_envelope(envelope(OTHER_SHARE, b"another"))
            .await
            .expect("stores");

        let page = store
            .list_key_envelopes([1; 32], None)
            .await
            .expect("reads");
        assert_eq!(page.envelopes.len(), 2);
        assert!(
            page.envelopes
                .iter()
                .any(|held| held.ciphertext == b"rotated"),
            "the newest, not the first"
        );
        assert_eq!(page.next_after_share_id, None, "both fit in one page");

        // Another device's post is not this one's.
        assert!(
            store
                .list_key_envelopes([9; 32], None)
                .await
                .expect("reads")
                .envelopes
                .is_empty()
        );
    })
    .await;
}
