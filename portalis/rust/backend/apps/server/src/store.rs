//! The store this server reads and writes.
//!
//! One enum rather than a generic parameter: every layer above the ports is
//! concrete, and swapping the backend should not change a handler's type. Each
//! method dispatches once and awaits the arm it chose.

use portalis_nexus_server_core::{
    DeviceId, DeviceRecord, EnvelopeRepository, FriendRepository, FriendshipEdge, FriendshipRecord,
    IdentityRepository, InMemoryIdentities, KeyEnvelopePage, KeyEnvelopeRecord, RepositoryError,
    ShareId, ShareMembershipRecord, ShareRecord, ShareRepository, ShareSnapshotRecord,
    UserDirectory, UserId, UserRecord,
};

use portalis_nexus_storage::embedded::Embedded;

/// Where durable identity and friend state lives.
#[derive(Debug)]
pub enum NexusStore {
    /// Held in memory and lost on restart. The default for local runs, the
    /// demo, and tests.
    Memory(Box<InMemoryIdentities>),
    /// Durable, and a directory of files rather than a server to operate.
    /// The one durable engine a node runs (ADR-0002).
    Embedded(Box<Embedded>),
}

impl Default for NexusStore {
    fn default() -> Self {
        Self::Memory(Box::default())
    }
}

impl NexusStore {
    #[must_use]
    pub fn embedded(store: Embedded) -> Self {
        Self::Embedded(Box::new(store))
    }

    /// Makes an in-memory store fail, for exercising degraded paths. Has no
    /// effect on a durable store, which fails for real reasons.
    pub fn set_unavailable(&self, unavailable: bool) {
        if let Self::Memory(memory) = self {
            memory.set_unavailable(unavailable);
        }
    }

    /// Fails only the device listing on an in-memory store, for reaching a
    /// caller's second failure path once its first read has succeeded.
    pub fn set_devices_unavailable(&self, unavailable: bool) {
        if let Self::Memory(memory) = self {
            memory.set_devices_unavailable(unavailable);
        }
    }

    /// Names the backend, for logs and readiness reporting.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Memory(_) => "memory",
            Self::Embedded(_) => "embedded",
        }
    }
}

impl UserDirectory for NexusStore {
    async fn find_user(&self, user_id: UserId) -> Result<Option<UserRecord>, RepositoryError> {
        match self {
            Self::Memory(store) => store.find_user(user_id).await,
            Self::Embedded(store) => store.find_user(user_id).await,
        }
    }

    async fn find_user_by_handle(
        &self,
        normalized_username: &str,
        discriminator: &str,
    ) -> Result<Option<UserRecord>, RepositoryError> {
        match self {
            Self::Memory(store) => {
                store
                    .find_user_by_handle(normalized_username, discriminator)
                    .await
            }
            Self::Embedded(store) => {
                store
                    .find_user_by_handle(normalized_username, discriminator)
                    .await
            }
        }
    }
}

impl IdentityRepository for NexusStore {
    async fn insert_registration(
        &self,
        user: UserRecord,
        device: DeviceRecord,
    ) -> Result<(), RepositoryError> {
        match self {
            Self::Memory(store) => store.insert_registration(user, device).await,
            Self::Embedded(store) => store.insert_registration(user, device).await,
        }
    }

    async fn find_device(
        &self,
        device_id: DeviceId,
    ) -> Result<Option<DeviceRecord>, RepositoryError> {
        match self {
            Self::Memory(store) => store.find_device(device_id).await,
            Self::Embedded(store) => store.find_device(device_id).await,
        }
    }

    async fn list_devices(&self, user: UserId) -> Result<Vec<DeviceRecord>, RepositoryError> {
        match self {
            Self::Memory(store) => store.list_devices(user).await,
            Self::Embedded(store) => store.list_devices(user).await,
        }
    }

    async fn link_device(&self, device: DeviceRecord) -> Result<(), RepositoryError> {
        match self {
            Self::Memory(store) => store.link_device(device).await,
            Self::Embedded(store) => store.link_device(device).await,
        }
    }

    async fn touch_device(
        &self,
        device_id: DeviceId,
        at_unix_ns: u64,
    ) -> Result<(), RepositoryError> {
        match self {
            Self::Memory(store) => store.touch_device(device_id, at_unix_ns).await,
            Self::Embedded(store) => store.touch_device(device_id, at_unix_ns).await,
        }
    }

    async fn revoke_device(
        &self,
        device_id: DeviceId,
        at_unix_ns: u64,
    ) -> Result<(), RepositoryError> {
        match self {
            Self::Memory(store) => store.revoke_device(device_id, at_unix_ns).await,
            Self::Embedded(store) => store.revoke_device(device_id, at_unix_ns).await,
        }
    }
}

impl FriendRepository for NexusStore {
    async fn find_friendship(
        &self,
        edge: FriendshipEdge,
    ) -> Result<Option<FriendshipRecord>, RepositoryError> {
        match self {
            Self::Memory(store) => store.find_friendship(edge).await,
            Self::Embedded(store) => store.find_friendship(edge).await,
        }
    }

    async fn save_friendship(
        &self,
        record: FriendshipRecord,
        expected_version: u64,
    ) -> Result<(), RepositoryError> {
        match self {
            Self::Memory(store) => store.save_friendship(record, expected_version).await,
            Self::Embedded(store) => store.save_friendship(record, expected_version).await,
        }
    }

    async fn list_friendships(
        &self,
        user: UserId,
    ) -> Result<Vec<FriendshipRecord>, RepositoryError> {
        match self {
            Self::Memory(store) => store.list_friendships(user).await,
            Self::Embedded(store) => store.list_friendships(user).await,
        }
    }
}

impl EnvelopeRepository for NexusStore {
    async fn put_key_envelope(&self, envelope: KeyEnvelopeRecord) -> Result<(), RepositoryError> {
        match self {
            Self::Memory(store) => store.put_key_envelope(envelope).await,
            Self::Embedded(store) => store.put_key_envelope(envelope).await,
        }
    }

    async fn list_key_envelopes(
        &self,
        recipient_device_id: DeviceId,
        after_share_id: Option<ShareId>,
    ) -> Result<KeyEnvelopePage, RepositoryError> {
        match self {
            Self::Memory(store) => {
                store
                    .list_key_envelopes(recipient_device_id, after_share_id)
                    .await
            }
            Self::Embedded(store) => {
                store
                    .list_key_envelopes(recipient_device_id, after_share_id)
                    .await
            }
        }
    }
}

impl ShareRepository for NexusStore {
    async fn find_share(&self, share_id: ShareId) -> Result<Option<ShareRecord>, RepositoryError> {
        match self {
            Self::Memory(store) => store.find_share(share_id).await,
            Self::Embedded(store) => store.find_share(share_id).await,
        }
    }

    async fn save_publication(
        &self,
        share: ShareRecord,
        snapshot: ShareSnapshotRecord,
        expected_revision: Option<u64>,
    ) -> Result<(), RepositoryError> {
        match self {
            Self::Memory(store) => {
                store
                    .save_publication(share, snapshot, expected_revision)
                    .await
            }
            Self::Embedded(store) => {
                store
                    .save_publication(share, snapshot, expected_revision)
                    .await
            }
        }
    }

    async fn find_snapshot(
        &self,
        share_id: ShareId,
        revision: u64,
    ) -> Result<Option<ShareSnapshotRecord>, RepositoryError> {
        match self {
            Self::Memory(store) => store.find_snapshot(share_id, revision).await,
            Self::Embedded(store) => store.find_snapshot(share_id, revision).await,
        }
    }

    async fn grant_share_access(
        &self,
        membership: ShareMembershipRecord,
    ) -> Result<(), RepositoryError> {
        match self {
            Self::Memory(store) => store.grant_share_access(membership).await,
            Self::Embedded(store) => store.grant_share_access(membership).await,
        }
    }

    async fn revoke_share_access(
        &self,
        share_id: ShareId,
        user_id: UserId,
    ) -> Result<(), RepositoryError> {
        match self {
            Self::Memory(store) => store.revoke_share_access(share_id, user_id).await,
            Self::Embedded(store) => store.revoke_share_access(share_id, user_id).await,
        }
    }

    async fn has_share_access(
        &self,
        share_id: ShareId,
        user_id: UserId,
    ) -> Result<bool, RepositoryError> {
        match self {
            Self::Memory(store) => store.has_share_access(share_id, user_id).await,
            Self::Embedded(store) => store.has_share_access(share_id, user_id).await,
        }
    }

    async fn list_authorized_shares(
        &self,
        user_id: UserId,
    ) -> Result<Vec<ShareRecord>, RepositoryError> {
        match self {
            Self::Memory(store) => store.list_authorized_shares(user_id).await,
            Self::Embedded(store) => store.list_authorized_shares(user_id).await,
        }
    }

    async fn list_share_members(&self, share_id: ShareId) -> Result<Vec<UserId>, RepositoryError> {
        match self {
            Self::Memory(store) => store.list_share_members(share_id).await,
            Self::Embedded(store) => store.list_share_members(share_id).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ADA: UserId = [1; 16];
    const GRACE: UserId = [2; 16];

    fn user() -> UserRecord {
        UserRecord {
            user_id: ADA,
            username: "Ada".to_owned(),
            normalized_username: "ada".to_owned(),
            discriminator: "7Q2XZ".to_owned(),
            created_at_unix_ns: 0,
        }
    }

    fn device() -> DeviceRecord {
        DeviceRecord {
            device_id: [1; 32],
            user_id: ADA,
            public_key: [1; 32],
            encryption_public_key: [2; 32],
            created_at_unix_ns: 0,
            last_authenticated_at_unix_ns: None,
            revoked_at_unix_ns: None,
        }
    }

    fn unavailable<T: std::fmt::Debug>(outcome: &Result<T, RepositoryError>) -> bool {
        matches!(outcome, Err(RepositoryError::Unavailable(_)))
    }

    #[tokio::test]
    async fn a_forced_fault_reaches_the_in_memory_backend_only() {
        let memory = NexusStore::default();
        assert_eq!(memory.kind(), "memory");
        memory.set_unavailable(true);
        assert!(unavailable(&memory.find_user(ADA).await));

        memory.set_unavailable(false);
        assert!(
            !unavailable(&memory.find_user(ADA).await),
            "clearing the fault puts the store back in service"
        );
    }

    /// Every method dispatched to the embedded engine, against a real file.
    ///
    /// The point of `NexusStore` is that the service cannot tell which engine
    /// is underneath, and the only way that stays true is to drive all of it
    /// through each one. The engine's own behaviour is the storage crate's
    /// conformance suite; what is checked here is that the wiring reaches it.
    #[tokio::test]
    async fn every_operation_reaches_the_embedded_engine() {
        let directory = std::env::temp_dir().join(format!(
            "portalis-store-embedded-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&directory);
        let store = NexusStore::embedded(
            portalis_nexus_storage::embedded::Embedded::open(&directory).expect("opens"),
        );
        assert_eq!(store.kind(), "embedded");

        store
            .insert_registration(user(), device())
            .await
            .expect("registers");
        assert_eq!(store.find_user(ADA).await.expect("reads"), Some(user()));
        assert_eq!(
            store
                .find_user_by_handle("ada", "7Q2XZ")
                .await
                .expect("reads"),
            Some(user())
        );
        assert_eq!(
            store.find_device(device().device_id).await.expect("reads"),
            Some(device())
        );
        assert_eq!(store.list_devices(ADA).await.expect("reads").len(), 1);
        // A second device joins an account that already exists, which is the
        // one arm registration cannot reach: the first device arrives with the
        // user, so linking is only ever the second one onward.
        store
            .link_device(DeviceRecord {
                device_id: [11; 32],
                public_key: [11; 32],
                ..device()
            })
            .await
            .expect("links");
        assert_eq!(store.list_devices(ADA).await.expect("reads").len(), 2);
        store
            .touch_device(device().device_id, 5)
            .await
            .expect("touches");
        store
            .revoke_device(device().device_id, 6)
            .await
            .expect("revokes");

        collections_friends_and_keys(&store).await;

        // Faults are an in-memory affair; a durable engine fails for real
        // reasons, and saying so is these calls having no effect.
        store.set_unavailable(true);
        store.set_devices_unavailable(true);
        assert_eq!(
            store.find_user(ADA).await.expect("still reads"),
            Some(user())
        );

        let _ = std::fs::remove_dir_all(&directory);
    }

    /// The rest of the dispatch, so every arm is walked rather than the
    /// identity half only.
    async fn collections_friends_and_keys(store: &NexusStore) {
        let share = ShareRecord {
            share_id: [3; 16],
            owner: ADA,
            revision: 1,
            snapshot_id: [4; 32],
            capsule: b"sealed".to_vec(),
            capsule_signature: vec![9; 64],
            created_at_unix_ns: 1,
            updated_at_unix_ns: 1,
        };
        let snapshot = ShareSnapshotRecord {
            share_id: share.share_id,
            revision: 1,
            snapshot_id: share.snapshot_id,
            capsule: share.capsule.clone(),
            capsule_signature: share.capsule_signature.clone(),
            created_at_unix_ns: 1,
        };
        store
            .save_publication(share.clone(), snapshot.clone(), None)
            .await
            .expect("publishes");
        assert_eq!(
            store.find_share(share.share_id).await.expect("reads"),
            Some(share.clone())
        );
        assert_eq!(
            store.find_snapshot(share.share_id, 1).await.expect("reads"),
            Some(snapshot)
        );
        store
            .grant_share_access(ShareMembershipRecord {
                share_id: share.share_id,
                user_id: GRACE,
                granted_at_unix_ns: 1,
            })
            .await
            .expect("grants");
        assert!(
            store
                .has_share_access(share.share_id, GRACE)
                .await
                .expect("reads")
        );
        assert_eq!(
            store
                .list_share_members(share.share_id)
                .await
                .expect("reads"),
            vec![GRACE]
        );
        assert_eq!(
            store.list_authorized_shares(GRACE).await.expect("reads"),
            vec![share.clone()]
        );
        store
            .revoke_share_access(share.share_id, GRACE)
            .await
            .expect("revokes");

        friends_and_keys(store, share.share_id).await;
    }

    /// Friendships and sealed keys, split out only because the whole walk is
    /// longer than one function should be.
    async fn friends_and_keys(store: &NexusStore, share_id: ShareId) {
        let edge = FriendshipEdge::between(ADA, GRACE).expect("two people");
        let friendship = FriendshipRecord {
            edge,
            requested_by: ADA,
            state: portalis_nexus_protocol::v1::FriendshipState::Pending,
            version: 1,
            created_at_unix_ns: 1,
            updated_at_unix_ns: 1,
        };
        store
            .save_friendship(friendship.clone(), 0)
            .await
            .expect("saves");
        assert_eq!(
            store.find_friendship(edge).await.expect("reads"),
            Some(friendship.clone())
        );
        assert_eq!(
            store.list_friendships(ADA).await.expect("reads"),
            vec![friendship]
        );

        store
            .put_key_envelope(KeyEnvelopeRecord {
                share_id,
                recipient_device_id: device().device_id,
                ephemeral_public_key: [5; 32],
                ciphertext: b"sealed key".to_vec(),
                created_at_unix_ns: 1,
            })
            .await
            .expect("stores");
        assert_eq!(
            store
                .list_key_envelopes(device().device_id, None)
                .await
                .expect("reads")
                .envelopes
                .len(),
            1
        );
    }
}
