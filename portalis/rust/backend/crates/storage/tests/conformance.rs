//! One suite, both engines.
//!
//! "Storage is a trait with two engines" (D5) means nothing unless the two
//! answer the same questions the same way. So these tests are written against
//! `server-core`'s repository traits and know nothing about redb, a file, or a
//! connection string — every one of them runs twice.
//!
//! In memory is the double the service's own tests use, and the embedded
//! engine is what a self-hoster runs. When the `MongoDB` engine moves into this
//! crate it joins the list below and either passes or is not finished.
//!
//! What is deliberately *not* here: anything about how a store is built. A
//! suite that had to know how to open each engine would grow an engine-shaped
//! branch per test, which is the thing it exists to prevent.

use portalis_nexus_server_core::{
    DeviceRecord, IdentityRepository, InMemoryIdentities, RepositoryError, ShareMembershipRecord,
    ShareRecord, ShareRepository, ShareSnapshotRecord, UserDirectory, UserRecord,
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

/// Runs `suite` against every engine, so a difference between them is a
/// failure rather than something nobody looked for.
async fn against_every_engine<F, Fut>(name: &str, suite: F)
where
    F: Fn(Engine) -> Fut,
    Fut: Future<Output = ()>,
{
    suite(Engine::Memory(Box::default())).await;

    let directory = std::env::temp_dir().join(format!(
        "portalis-conformance-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("a scratch directory");
    suite(Engine::Embedded(Box::new(
        Embedded::open(directory.join("service.redb")).expect("opens"),
    )))
    .await;
    let _ = std::fs::remove_dir_all(&directory);
}

/// One engine, named so a failure says which. Boxed because the two differ
/// enough in size that holding them inline would size every one by the larger.
enum Engine {
    Memory(Box<InMemoryIdentities>),
    Embedded(Box<Embedded>),
}

// The suite speaks only the traits; this is the one place that knows there is
// more than one engine, and it exists so a test never has to.
macro_rules! delegate {
    ($self:ident, $trait:ident :: $method:ident $(, $argument:expr)*) => {
        match $self {
            Engine::Memory(store) => $trait::$method(store.as_ref() $(, $argument)*).await,
            Engine::Embedded(store) => $trait::$method(store.as_ref() $(, $argument)*).await,
        }
    };
}

impl Engine {
    async fn insert_registration(
        &self,
        user: UserRecord,
        device: DeviceRecord,
    ) -> Result<(), RepositoryError> {
        delegate!(self, IdentityRepository::insert_registration, user, device)
    }

    async fn find_user(&self, id: [u8; 16]) -> Result<Option<UserRecord>, RepositoryError> {
        delegate!(self, UserDirectory::find_user, id)
    }

    async fn find_user_by_handle(
        &self,
        normalized: &str,
        discriminator: &str,
    ) -> Result<Option<UserRecord>, RepositoryError> {
        delegate!(
            self,
            UserDirectory::find_user_by_handle,
            normalized,
            discriminator
        )
    }

    async fn find_device(&self, id: [u8; 32]) -> Result<Option<DeviceRecord>, RepositoryError> {
        delegate!(self, IdentityRepository::find_device, id)
    }

    async fn list_devices(&self, user: [u8; 16]) -> Result<Vec<DeviceRecord>, RepositoryError> {
        delegate!(self, IdentityRepository::list_devices, user)
    }

    async fn link_device(&self, device: DeviceRecord) -> Result<(), RepositoryError> {
        delegate!(self, IdentityRepository::link_device, device)
    }

    async fn touch_device(&self, id: [u8; 32], at: u64) -> Result<(), RepositoryError> {
        delegate!(self, IdentityRepository::touch_device, id, at)
    }

    async fn revoke_device(&self, id: [u8; 32], at: u64) -> Result<(), RepositoryError> {
        delegate!(self, IdentityRepository::revoke_device, id, at)
    }

    async fn find_share(&self, id: [u8; 16]) -> Result<Option<ShareRecord>, RepositoryError> {
        delegate!(self, ShareRepository::find_share, id)
    }

    async fn save_publication(
        &self,
        share: ShareRecord,
        snapshot: ShareSnapshotRecord,
        expected: Option<u64>,
    ) -> Result<(), RepositoryError> {
        delegate!(
            self,
            ShareRepository::save_publication,
            share,
            snapshot,
            expected
        )
    }

    async fn find_snapshot(
        &self,
        id: [u8; 16],
        revision: u64,
    ) -> Result<Option<ShareSnapshotRecord>, RepositoryError> {
        delegate!(self, ShareRepository::find_snapshot, id, revision)
    }

    async fn grant_share_access(
        &self,
        membership: ShareMembershipRecord,
    ) -> Result<(), RepositoryError> {
        delegate!(self, ShareRepository::grant_share_access, membership)
    }

    async fn revoke_share_access(
        &self,
        share: [u8; 16],
        user: [u8; 16],
    ) -> Result<(), RepositoryError> {
        delegate!(self, ShareRepository::revoke_share_access, share, user)
    }

    async fn has_share_access(
        &self,
        share: [u8; 16],
        user: [u8; 16],
    ) -> Result<bool, RepositoryError> {
        delegate!(self, ShareRepository::has_share_access, share, user)
    }

    async fn list_authorized_shares(
        &self,
        user: [u8; 16],
    ) -> Result<Vec<ShareRecord>, RepositoryError> {
        delegate!(self, ShareRepository::list_authorized_shares, user)
    }

    async fn list_share_members(&self, share: [u8; 16]) -> Result<Vec<[u8; 16]>, RepositoryError> {
        delegate!(self, ShareRepository::list_share_members, share)
    }
}

#[tokio::test]
async fn a_registration_is_all_or_nothing_in_either_engine() {
    against_every_engine("registration", |store| async move {
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
async fn a_handle_finds_its_user_in_either_engine() {
    against_every_engine("handles", |store| async move {
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
async fn devices_are_linked_listed_touched_and_revoked_in_either_engine() {
    against_every_engine("devices", |store| async move {
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
async fn publishing_is_a_compare_and_set_in_either_engine() {
    against_every_engine("publish", |store| async move {
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
async fn membership_is_granted_revoked_and_listed_in_either_engine() {
    against_every_engine("membership", |store| async move {
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
async fn what_was_never_stored_is_absent_in_either_engine() {
    against_every_engine("absent", |store| async move {
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
